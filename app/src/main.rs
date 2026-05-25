mod root_view;
mod terminal_config;
mod terminal_view;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
#[allow(deprecated)]
use cocoa::appkit::NSApp;
#[allow(deprecated)]
use cocoa::base::nil;
#[allow(deprecated)]
use cocoa::foundation::NSRect;
use objc::{msg_send, sel, sel_impl};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use terminal_backend::{PtySession, TerminalState};
use warpui::{
    platform::{self, AppCallbacks, WindowBounds},
    AddWindowOptions, AssetProvider,
};

/// Font size used for terminal rendering (must match root_view::FONT_SIZE).
const FONT_SIZE: f32 = 14.0;

// C global set to 1 by windowWillStartLiveResize: and cleared to 0 by
// windowDidEndLiveResize: in window.m. Used to defer SIGWINCH until the user
// releases the mouse, preventing shell-redraw spam that produces blank lines.
extern "C" {
    static purewarp_is_live_resizing: i32;
}

/// Measured pixel dimensions of a single terminal character cell.
struct CellMetrics {
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, RustEmbed)]
#[folder = "assets"]
pub struct Assets;

pub static ASSETS: Assets = Assets;

impl AssetProvider for Assets {
    fn get(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        <Assets as RustEmbed>::get(path)
            .map(|f| f.data)
            .ok_or_else(|| anyhow!("no asset exists at path {}", path))
    }
}

fn main() -> Result<()> {
    env_logger::init();
    eprintln!("=== PureWarp starting ===");

    // Load configuration
    let config = terminal_config::load_config();
    eprintln!(
        "Config: shell={}, cols={}, rows={}",
        config.shell.program,
        config.terminal.cols,
        config.terminal.rows
    );

    // Initialize terminal state
    let terminal_state = Arc::new(Mutex::new(TerminalState::new(
        config.terminal.cols,
        config.terminal.rows,
        config.terminal.max_scrollback,
    )));

    {
        let mut state = terminal_state.lock().unwrap();
        config.theme.apply_to_palette(&mut state.palette);
    }

    // Spawn PTY session
    let rt = tokio::runtime::Runtime::new()?;
    let mut pty_session = rt.block_on(async {
        PtySession::spawn(
            terminal_state.clone(),
            config.terminal.cols as u16,
            config.terminal.rows as u16,
            Some(&config.shell.program),
        )
        .await
    })?;

    eprintln!("PTY spawned: {}", config.shell.program);

    let update_rx = pty_session
        .take_receiver()
        .expect("PTY update receiver should be available");

    #[allow(clippy::arc_with_non_send_sync)]
    let pty = Arc::new(Mutex::new(pty_session));

    // Keep the runtime alive for the background PTY reader task
    std::mem::forget(rt);

    eprintln!("Creating app builder...");

    let font_family = config.terminal.font_family.clone();

    // Shared cell metrics: measured after font loading, read by resize callback.
    let cell_metrics: Rc<RefCell<Option<CellMetrics>>> = Rc::new(RefCell::new(None));

    // Set up window resize callback to recalculate terminal grid dimensions.
    let state_for_resize = terminal_state.clone();
    let pty_for_resize = pty.clone();
    let cell_metrics_for_resize = cell_metrics.clone();
    // Last (cols, rows) for which we sent SIGWINCH, and the timestamp.
    // Updated only when we actually notify the PTY, used for throttling.
    let last_pty_size: Rc<RefCell<(usize, usize, Instant)>> =
        Rc::new(RefCell::new((0, 0, Instant::now() - Duration::from_millis(200))));
    let mut app_callbacks = AppCallbacks::default();
    #[allow(clippy::field_reassign_with_default)]
    {
    app_callbacks.on_window_resized = Some(Box::new(move |_ctx| {
        let cm = cell_metrics_for_resize.borrow();
        let cm = match cm.as_ref() {
            Some(cm) => cm,
            None => return, // Font not loaded yet.
        };

        // Get the content view size and live-resize state directly from the
        // key NSWindow, avoiding any stale AppContext data.
        #[allow(deprecated)]
        let (win_width, win_height, is_live_resize) = unsafe {
            let app = NSApp();
            if app == nil {
                return;
            }
            let window: cocoa::base::id = msg_send![app, keyWindow];
            if window == nil {
                return;
            }
            let content_view: cocoa::base::id = msg_send![window, contentView];
            if content_view == nil {
                return;
            }
            let frame: NSRect = msg_send![content_view, frame];
            // Read the C global set by windowWillStartLiveResize:/windowDidEndLiveResize:
            // in window.m. Avoids calling isInLiveResize on WarpWindow, which
            // doesn't respond to that selector despite being an NSWindow subclass.
            let live = purewarp_is_live_resizing != 0;
            (frame.size.width as f32, frame.size.height as f32, live)
        };

        if win_width <= 0.0 || win_height <= 0.0 {
            return;
        }

        // Subtract the padding used in root_view: 28px top, 6px left.
        let available_width = (win_width - 6.0).max(cm.width);
        let available_height = (win_height - 28.0).max(cm.height);
        let new_cols = ((available_width / cm.width) as usize).max(1);
        let new_rows = ((available_height / cm.height) as usize).max(1);

        if is_live_resize {
            // User is actively dragging — always update the visual grid so the
            // rendering adapts to the new window size, but throttle SIGWINCH to
            // avoid shell redraw spam that produces blank lines.
            // Send at most one SIGWINCH per 150ms during drag.
            if let Ok(mut state) = state_for_resize.lock() {
                state.resize(new_cols, new_rows);
            }
            let now = Instant::now();
            let should_notify = {
                let prev = last_pty_size.borrow();
                (new_cols != prev.0 || new_rows != prev.1)
                    && now.duration_since(prev.2) >= Duration::from_millis(150)
            };
            if should_notify {
                *last_pty_size.borrow_mut() = (new_cols, new_rows, now);
                eprintln!("Resize PTY (throttled): {}x{} cells", new_cols, new_rows);
                if let Ok(pty_guard) = pty_for_resize.lock() {
                    let _ = pty_guard.resize(new_cols as u16, new_rows as u16);
                }
            }
            return;
        }

        // Not in live resize (programmatic resize or drag just ended).
        // Send SIGWINCH if the grid dimensions changed since the last notify.
        let should_notify = {
            let prev = last_pty_size.borrow();
            new_cols != prev.0 || new_rows != prev.1
        };
        if !should_notify {
            return;
        }
        *last_pty_size.borrow_mut() = (new_cols, new_rows, Instant::now());

        eprintln!("Resize PTY: {}x{} cells", new_cols, new_rows);

        // Update terminal state grid dimensions.
        if let Ok(mut state) = state_for_resize.lock() {
            state.resize(new_cols, new_rows);
        }

        // Resize the PTY (sends SIGWINCH to the child process).
        if let Ok(pty_guard) = pty_for_resize.lock() {
            let _ = pty_guard.resize(new_cols as u16, new_rows as u16);
        }
    }));
    } // end allow block

    let app_builder =
        platform::AppBuilder::new(app_callbacks, Box::new(ASSETS), None);

    eprintln!("Running app...");

    let _ = app_builder.run(move |ctx| {
        eprintln!("App callback invoked, adding window...");

        // Preload a monospace font so it is ready before the first render.
        use warpui::SingletonEntity as _;
        let font_family = font_family.clone();
        let font_cache_handle = warpui::fonts::Cache::handle(ctx);
        let font_id = font_cache_handle.update(
            ctx,
            |cache: &mut warpui::fonts::Cache, _| {
                cache
                    .load_system_font(&font_family)
                    .or_else(|_| cache.load_system_font("SF Mono"))
                    .or_else(|_| cache.load_system_font("Menlo"))
                    .or_else(|_| cache.load_system_font("Monaco"))
                    .or_else(|_| cache.load_system_font("Courier"))
                    .expect("Should load a monospace system font")
            },
        );

        // Measure actual cell dimensions from the loaded font.
        // Use glyph advance width (the spacing between characters in layout),
        // NOT em_width (typographic/ink bounds), which is narrower than advance.
        let measured = font_cache_handle.read(ctx, |cache: &warpui::fonts::Cache, _| {
            let font = cache.select_font(font_id, Default::default());
            let cell_width = match cache.glyph_for_char(font, 'm', false) {
                Some((glyph_id, _)) => cache
                    .glyph_advance(font, FONT_SIZE, glyph_id)
                    .map(|adv| adv.x())
                    .unwrap_or_else(|_| cache.em_width(font_id, FONT_SIZE)),
                None => cache.em_width(font_id, FONT_SIZE),
            };
            let h = cache.line_height(FONT_SIZE, 1.0);
            eprintln!("Cell metrics: width={}, height={}", cell_width, h);
            (cell_width, h)
        });
        *cell_metrics.borrow_mut() = Some(CellMetrics {
            width: measured.0,
            height: measured.1,
        });

        let window_options = AddWindowOptions {
            window_bounds: WindowBounds::ExactSize(warpui::geometry::vector::vec2f(
                700.0, 450.0,
            )),
            ..Default::default()
        };
        ctx.add_window(window_options, move |_cx| {
            eprintln!("Window factory called, creating RootView...");
            root_view::RootView::new(terminal_state.clone(), pty.clone(), update_rx, font_id)
        });
        eprintln!("Window added.");
    });

    eprintln!("App exited.");
    Ok(())
}
