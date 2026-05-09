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
    ) -> Self {
        Self {
            terminal_state,
            pty,
            update_rx: RefCell::new(Some(update_rx)),
            font_family: None,
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
        let state = self.terminal_state.lock().unwrap();
        let bg_color = state.palette.background;
        let fg_color = state.palette.foreground;
        let cursor_color = state.palette.cursor;
        let cursor_row = state.cursor.row;
        let cursor_col = state.cursor.col;
        let cursor_visible = state.cursor.visible && *self.blink_on.borrow();
        let font_family = self.font_family;
        let visible_count = state.visible_rows();

        // ── Build display rows from scrollback + visible, respecting scroll offset ──
        let scroll_offset = *self.scroll_offset.borrow();
        let sb = state.scrollback_ref();
        let vis = state.visible_rows_ref();
        let sb_len = sb.len();
        let total = sb_len + vis.len();

        // Which range of the combined buffer to show.
        let start = total.saturating_sub(visible_count + scroll_offset);

        // Collect the rows for display.
        let mut display_rows: Vec<&warp_terminal::model::grid::row::Row> =
            Vec::with_capacity(visible_count);
        for i in start..(start + visible_count).min(total) {
            if i < sb_len {
                display_rows.push(&sb[i]);
            } else {
                display_rows.push(&vis[i - sb_len]);
            }
        }

        // ── Row-based rendering ──
        let mut row_elements: Vec<Box<dyn Element>> = Vec::with_capacity(display_rows.len());

        for (row_idx, row) in display_rows.iter().enumerate() {
            let cells: &[warp_terminal::model::grid::cell::Cell] = &row[..];

            // Build the row text: collect every printable character.
            let row_text: String = cells
                .iter()
                .map(|c| if c.c == '\0' || c.c.is_ascii_control() { ' ' } else { c.c })
                .collect();

            // Cursor is only visible when scroll_offset == 0 (viewing the live area).
            let is_cursor_row = scroll_offset == 0
                && cursor_visible
                && row_idx + start == sb_len + cursor_row;

            let row_element = if let Some(fid) = font_family {
                if is_cursor_row && cursor_col < cells.len() {
                    // ── Cursor row: split into before / cursor-cell / after ──
                    let before = &row_text[..cursor_col.min(row_text.len())];
                    let cursor_ch = &row_text[cursor_col..(cursor_col + 1).min(row_text.len())];
                    let after = &row_text[(cursor_col + 1).min(row_text.len())..];

                    let before_text = Text::new_inline(before.to_string(), fid, FONT_SIZE)
                        .with_color(fg_color)
                        .finish();
                    let after_text = Text::new_inline(after.to_string(), fid, FONT_SIZE)
                        .with_color(fg_color)
                        .finish();

                    // Cursor cell: inverted background/text
                    let cursor_cell = Stack::new()
                        .with_child(
                            Rect::new()
                                .with_background_color(cursor_color)
                                .finish(),
                        )
                        .with_child(
                            Text::new_inline(cursor_ch.to_string(), fid, FONT_SIZE)
                                .with_color(bg_color)
                                .finish(),
                        )
                        .finish();

                    // Flex factors proportional to character count for equal-width columns
                    let before_len = before.chars().count() as f32;
                    let after_len = after.chars().count() as f32;

                    Flex::row()
                        .with_child(
                            Shrinkable::new(before_len, before_text).finish(),
                        )
                        .with_child(
                            Shrinkable::new(1.0, cursor_cell).finish(),
                        )
                        .with_child(
                            Shrinkable::new(after_len, after_text).finish(),
                        )
                        .finish()
                } else {
                    Text::new_inline(row_text, fid, FONT_SIZE)
                        .with_color(fg_color)
                        .finish()
                }
            } else {
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

        // Padding: inset the grid from window edges (top has extra space).
        let padded_grid = Container::new(grid)
            .with_padding(Padding::uniform(16.0).with_top(32.0))
            .finish();

        // Full-window background behind the grid.
        let content = Stack::new()
            .with_child(Rect::new().with_background_color(bg_color).finish())
            .with_child(padded_grid)
            .finish();

        let pty = self.pty.clone();
        let scroll_offset_rc = self.scroll_offset.clone();
        let max_scroll = sb_len;

        // ── Keyboard + scroll wheel events via outermost EventHandler ──
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
                    let lines = (delta.y().abs() / 20.0) as usize; // 20px per "line"
                    if delta.y() > 0.0 {
                        // Scroll up = go further back
                        *offset = (*offset + lines.max(1)).min(max_scroll);
                    } else if delta.y() < 0.0 {
                        // Scroll down = go toward bottom
                        *offset = offset.saturating_sub(lines.max(1));
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
