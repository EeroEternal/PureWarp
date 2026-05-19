use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use futures::StreamExt;
use terminal_backend::{PtySession, TerminalState};
use warp_terminal::model::grid::Dimensions;
use warpui::{
    elements::{
        Container, DispatchEventResult, EventHandler, Flex,
        ParentElement, Rect, Stack, Text,
    },
    fonts::FamilyId,
    AppContext, Element, Entity, EventContext, FocusContext, TypedActionView,
    View, ViewContext,
};

/// Font size for terminal text.
const FONT_SIZE: f32 = 14.0;

use crate::terminal_view::terminal_keystroke_to_bytes;

pub struct RootView {
    terminal_state: Arc<Mutex<TerminalState>>,
    pty: Arc<Mutex<PtySession>>,
    update_rx: RefCell<Option<futures::channel::mpsc::UnboundedReceiver<()>>>,
    /// Font family for rendering cell characters.
    font_family: Option<FamilyId>,
    /// How many lines the user has scrolled back (0 = at bottom, following output).
    scroll_offset: Rc<RefCell<usize>>,
    /// Blink state: toggled by a periodic timer.
    blink_on: Rc<RefCell<bool>>,
}

impl RootView {
    /// Create a new root view.
    pub fn new(
        terminal_state: Arc<Mutex<TerminalState>>,
        pty: Arc<Mutex<PtySession>>,
        update_rx: futures::channel::mpsc::UnboundedReceiver<()>,
        font_id: FamilyId,
    ) -> Self {
        Self {
            terminal_state,
            pty,
            update_rx: RefCell::new(Some(update_rx)),
            font_family: Some(font_id),
            scroll_offset: Rc::new(RefCell::new(0)),
            blink_on: Rc::new(RefCell::new(true)),
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
        let fid = self.font_family.unwrap();
        let state = self.terminal_state.lock().unwrap();
        let bg_color = state.palette.background;
        let fg_color = state.palette.foreground;
        let cursor_color = state.palette.cursor;
        let cursor_row = state.cursor.row;
        let cursor_col = state.cursor.col;
        let cursor_visible = state.cursor.visible && *self.blink_on.borrow();
        let visible_count = state.visible_rows();

        let scroll_offset = *self.scroll_offset.borrow();
        let sb = state.scrollback_ref();
        let vis = state.visible_rows_ref();
        let sb_len = sb.len();
        let total = sb_len + vis.len();
        let start = total.saturating_sub(visible_count + scroll_offset);

        let mut display_rows: Vec<&warp_terminal::model::grid::row::Row> =
            Vec::with_capacity(visible_count);
        for i in start..(start + visible_count).min(total) {
            if i < sb_len {
                display_rows.push(&sb[i]);
            } else {
                display_rows.push(&vis[i - sb_len]);
            }
        }

        // ── Render each row as a single Text element (group by row) to avoid
        // sub-pixel per-character spacing drift in Flex::row.  The cursor cell
        // is the only element that needs separate rendering (for inverted colours).
        let mut row_elements: Vec<Box<dyn Element>> = Vec::with_capacity(display_rows.len());
        for (row_idx, row) in display_rows.iter().enumerate() {
            let cells: &[warp_terminal::model::grid::cell::Cell] = &row[..];
            let is_cursor_row = scroll_offset == 0
                && cursor_visible
                && row_idx + start == sb_len + cursor_row;

            // Build the full row string
            let row_str: String = cells
                .iter()
                .map(|c| if c.c == '\0' || c.c.is_ascii_control() { ' ' } else { c.c })
                .collect();

            if is_cursor_row && cursor_col < row_str.len() {
                // Split around the cursor cell so it can be shown with inverted colours.
                let before = &row_str[..cursor_col];
                let cursor_ch = &row_str[cursor_col..=cursor_col];
                let after = &row_str[cursor_col + 1..];

                let mut cursor_row_els: Vec<Box<dyn Element>> = Vec::with_capacity(3);
                if !before.is_empty() {
                    cursor_row_els.push(
                        Text::new_inline(before.to_string(), fid, FONT_SIZE)
                            .with_color(fg_color)
                            .with_line_height_ratio(1.0)
                            .finish(),
                    );
                }
                cursor_row_els.push(
                    Container::new(
                        Text::new_inline(cursor_ch.to_string(), fid, FONT_SIZE)
                            .with_color(bg_color)
                            .with_line_height_ratio(1.0)
                            .finish(),
                    )
                    .with_background_color(cursor_color)
                    .finish(),
                );
                if !after.is_empty() {
                    cursor_row_els.push(
                        Text::new_inline(after.to_string(), fid, FONT_SIZE)
                            .with_color(fg_color)
                            .with_line_height_ratio(1.0)
                            .finish(),
                    );
                }
                row_elements.push(Flex::row().with_children(cursor_row_els).finish());
            } else {
                // No cursor on this row – single Text element.
                row_elements.push(
                    Text::new_inline(row_str, fid, FONT_SIZE)
                        .with_color(fg_color)
                        .with_line_height_ratio(1.0)
                        .finish(),
                );
            }
        }

        let grid = Flex::column()
            .with_children(row_elements)
            .finish();

        let content = Stack::new()
            .with_child(Rect::new().with_background_color(bg_color).finish())
            .with_child(grid)
            .finish();

        let pty = self.pty.clone();
        let scroll_offset_rc = self.scroll_offset.clone();
        let max_scroll = sb_len;

        EventHandler::new(content)
            .on_keydown(
                move |_event_ctx: &mut EventContext,
                      _app: &AppContext,
                      keystroke: &warpui::keymap::Keystroke|
                      -> DispatchEventResult {
                    let bytes = terminal_keystroke_to_bytes(keystroke);
                    if !bytes.is_empty() {
                        if let Ok(pty_guard) = pty.lock() {
                            let _ = pty_guard.write_input(&bytes);
                        }
                    }
                    DispatchEventResult::StopPropagation
                },
            )
            .on_scroll_wheel(
                move |_ctx: &mut EventContext,
                      _app: &AppContext,
                      delta: &warpui::geometry::vector::Vector2F,
                      _modifiers: &warpui::event::ModifiersState|
                      -> DispatchEventResult {
                    let mut offset = scroll_offset_rc.borrow_mut();
                    let lines = (delta.y().abs() / 20.0) as usize;
                    if delta.y() > 0.0 {
                        *offset = (*offset + lines.max(1)).min(max_scroll);
                    } else if delta.y() < 0.0 {
                        *offset = offset.saturating_sub(lines.max(1));
                    }
                    DispatchEventResult::StopPropagation
                },
            )
            .finish()
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        if let Some(rx) = self.update_rx.borrow_mut().take() {
            let stream = rx.map(|_| ());
            let scroll_offset = self.scroll_offset.clone();
            ctx.spawn_stream_local(
                stream,
                move |_view, _item, ctx| {
                    // Reset scroll to bottom whenever new PTY output arrives.
                    *scroll_offset.borrow_mut() = 0;
                    ctx.notify();
                },
                |_view, ctx| {
                    log::info!("PTY stream ended – closing window");
                    ctx.close_window();
                },
            );
        }

        // Force a re-render on focus to ensure font textures are uploaded.
        ctx.notify();

        // ── Cursor blink timer ──
        let blink_on = self.blink_on.clone();
        let (blink_tx, blink_rx) = futures::channel::mpsc::unbounded::<()>();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(530));
                let _ = blink_tx.unbounded_send(());
            }
        });
        ctx.spawn_stream_local(
            blink_rx.map(|_| ()),
            move |_view, _item, ctx| {
                let mut b = blink_on.borrow_mut();
                *b = !*b;
                ctx.notify();
            },
            |_, _| {},
        );
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
