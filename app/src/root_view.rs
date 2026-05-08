use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use terminal_backend::{PtySession, TerminalState};
use warpui::{
    elements::{
        Container, DispatchEventResult, EventHandler, Flex, MainAxisSize, Padding,
        ParentElement, Rect, Shrinkable, Stack, Text,
    },
    fonts::FamilyId,
    AppContext, Element, Entity, EventContext, FocusContext, SingletonEntity as _, TypedActionView,
    View, ViewContext,
};

/// Font size for terminal text.
const FONT_SIZE: f32 = 14.0;

use crate::terminal_view::terminal_keystroke_to_bytes;

pub struct RootView {
    terminal_state: Arc<Mutex<TerminalState>>,
    pty: Arc<Mutex<PtySession>>,
    update_rx: RefCell<Option<futures::channel::mpsc::UnboundedReceiver<()>>>,
    /// Font family for rendering cell characters, loaded lazily on first focus.
    font_family: Option<FamilyId>,
}

impl RootView {
    /// Create a new root view.
    pub fn new(
        terminal_state: Arc<Mutex<TerminalState>>,
        pty: Arc<Mutex<PtySession>>,
        update_rx: futures::channel::mpsc::UnboundedReceiver<()>,
    ) -> Self {
        Self {
            terminal_state,
            pty,
            update_rx: RefCell::new(Some(update_rx)),
            font_family: None,
        }
    }
}

impl Entity for RootView {
    type Event = ();
}

impl View for RootView {
    fn ui_name() -> &'static str {
        "PureWarpRootView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let state = self.terminal_state.lock().unwrap();
        let visible_rows = state.visible_rows_ref();
        let bg_color = state.palette.background;
        let fg_color = state.palette.foreground;
        let cursor_color = state.palette.cursor;
        let cursor_row = state.cursor.row;
        let cursor_col = state.cursor.col;
        let cursor_visible = state.cursor.visible;
        let font_family = self.font_family;

        let pty = self.pty.clone();

        // ── Row-based rendering (much faster than per-cell) ──
        // Each row becomes a single Text element containing concatenated characters.
        let mut row_elements: Vec<Box<dyn Element>> = Vec::with_capacity(visible_rows.len());

        for (row_idx, row) in visible_rows.iter().enumerate() {
            let cells: &[warp_terminal::model::grid::cell::Cell] = &row[..];

            // Build the row text: collect every printable character.
            let row_text: String = cells
                .iter()
                .map(|c| if c.c == '\0' || c.c.is_ascii_control() { ' ' } else { c.c })
                .collect();

            let is_cursor_row = row_idx == cursor_row && cursor_visible;

            let row_element = if let Some(fid) = font_family {
                if is_cursor_row && cursor_col < cells.len() {
                    // Row containing the cursor.
                    // For now render the whole row in foreground colour;
                    // cursor highlighting will be refined later.
                    Text::new_inline(row_text, fid, FONT_SIZE)
                        .with_color(fg_color)
                        .finish()
                } else {
                    Text::new_inline(row_text, fid, FONT_SIZE)
                        .with_color(fg_color)
                        .finish()
                }
            } else {
                // Font not yet loaded – show a coloured placeholder row.
                let row_bg = if is_cursor_row {
                    cursor_color
                } else {
                    bg_color
                };
                Rect::new().with_background_color(row_bg).finish()
            };

            row_elements.push(Shrinkable::new(1.0, row_element).finish());
        }

        let grid = Flex::column()
            .with_main_axis_size(MainAxisSize::Max)
            .with_children(row_elements)
            .finish();

        // Padding: inset the grid from window edges.
        let padded_grid = Container::new(grid)
            .with_padding(Padding::uniform(16.0))
            .finish();

        // Full-window background behind the grid.
        let content = Stack::new()
            .with_child(Rect::new().with_background_color(bg_color).finish())
            .with_child(padded_grid)
            .finish();

        // ── Keyboard input via outermost EventHandler ──
        EventHandler::new(content)
            .on_keydown(
                move |_event_ctx: &mut EventContext,
                      _app: &AppContext,
                      keystroke: &warpui::keymap::Keystroke|
                      -> DispatchEventResult {
                    eprintln!("Key event received: {:?}", keystroke);
                    let bytes = terminal_keystroke_to_bytes(keystroke);
                    if !bytes.is_empty() {
                        eprintln!("  -> sending {} bytes to PTY", bytes.len());
                        if let Ok(pty_guard) = pty.lock() {
                            if let Err(e) = pty_guard.write_input(&bytes) {
                                eprintln!("  -> PTY write error: {}", e);
                            }
                        }
                    }
                    DispatchEventResult::StopPropagation
                },
            )
            .finish()
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        // Lazily load a monospace font on first focus.
        if self.font_family.is_none() {
            let fid = warpui::fonts::Cache::handle(ctx).update(
                ctx,
                |cache: &mut warpui::fonts::Cache, _| {
                    cache
                        .load_system_font("Menlo")
                        .or_else(|_| cache.load_system_font("Monaco"))
                        .or_else(|_| cache.load_system_font("Courier"))
                        .expect("Should load a monospace system font")
                },
            );
            self.font_family = Some(fid);
            ctx.notify();
        }

        if let Some(rx) = self.update_rx.borrow_mut().take() {
            let stream = rx.map(|_| ());
            ctx.spawn_stream_local(
                stream,
                |_view, _item, ctx| {
                    ctx.notify();
                },
                |_view, ctx| {
                    log::info!("PTY stream ended – closing window");
                    ctx.close_window();
                },
            );
        }
    }

    fn keymap_context(&self, _app: &AppContext) -> warpui::keymap::Context {
        let mut ctx = warpui::keymap::Context::default();
        ctx.set.insert("PureWarpTerminalView");
        ctx
    }
}

impl TypedActionView for RootView {
    type Action = ();
}
