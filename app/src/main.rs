mod root_view;
mod terminal_config;
mod terminal_view;

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use terminal_backend::{PtySession, TerminalState};
use warpui::{
    platform::{self, WindowBounds},
    AddWindowOptions, AssetProvider,
};

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

    let pty = Arc::new(Mutex::new(pty_session));

    // Keep the runtime alive for the background PTY reader task
    std::mem::forget(rt);

    eprintln!("Creating app builder...");

    let app_builder =
        platform::AppBuilder::new(platform::AppCallbacks::default(), Box::new(ASSETS), None);

    eprintln!("Running app...");

    let _ = app_builder.run(move |ctx| {
        eprintln!("App callback invoked, adding window...");

        // Preload a monospace font so it is ready before the first render.
        use warpui::SingletonEntity as _;
        let font_id = warpui::fonts::Cache::handle(ctx).update(
            ctx,
            |cache: &mut warpui::fonts::Cache, _| {
                cache
                    .load_system_font("Menlo")
                    .or_else(|_| cache.load_system_font("Monaco"))
                    .or_else(|_| cache.load_system_font("Courier"))
                    .expect("Should load a monospace system font")
            },
        );

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
