use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use futures::StreamExt;
use pathfinder_color::ColorU;
use terminal_backend::{ColorPalette, PtySession, TerminalState};
use warp_terminal::model::ansi::{Color as AnsiColor, NamedColor};
use warp_terminal::model::grid::cell::{Cell, Flags};
use warp_terminal::model::grid::Dimensions;
use warpui::{
    elements::{
        Container, DispatchEventResult, EventHandler, Flex, Padding,
        ParentElement, Rect, Stack, Text,
    },
    fonts::FamilyId,
    AppContext, Element, Entity, EventContext, FocusContext, TypedActionView,
    View, ViewContext,
};

/// Font size for terminal text.
const FONT_SIZE: f32 = 14.0;

use crate::terminal_view::terminal_keystroke_to_bytes;

/// Resolve an ANSI cell color into a concrete RGBA color using the palette.
fn resolve_color(c: &AnsiColor, palette: &ColorPalette, bold: bool) -> ColorU {
    match c {
        AnsiColor::Named(named) => match named {
            NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => {
                palette.foreground
            }
            NamedColor::Background => palette.background,
            NamedColor::Cursor => palette.cursor,
            NamedColor::Black | NamedColor::DimBlack => palette.colors[0],
            NamedColor::Red | NamedColor::DimRed => palette.colors[1],
            NamedColor::Green | NamedColor::DimGreen => palette.colors[2],
            NamedColor::Yellow | NamedColor::DimYellow => palette.colors[3],
            NamedColor::Blue | NamedColor::DimBlue => palette.colors[4],
            NamedColor::Magenta | NamedColor::DimMagenta => palette.colors[5],
            NamedColor::Cyan | NamedColor::DimCyan => palette.colors[6],
            NamedColor::White | NamedColor::DimWhite => palette.colors[7],
            NamedColor::BrightBlack => palette.colors[8],
            NamedColor::BrightRed => palette.colors[9],
            NamedColor::BrightGreen => palette.colors[10],
            NamedColor::BrightYellow => palette.colors[11],
            NamedColor::BrightBlue => palette.colors[12],
            NamedColor::BrightMagenta => palette.colors[13],
            NamedColor::BrightCyan => palette.colors[14],
            NamedColor::BrightWhite => palette.colors[15],
        },
        AnsiColor::Indexed(idx) => indexed_color(*idx, palette, bold),
        AnsiColor::Spec(c) => *c,
    }
}

/// Resolve a 256-color indexed value into RGBA.
fn indexed_color(idx: u8, palette: &ColorPalette, bold: bool) -> ColorU {
    if idx < 16 {
        // Bold may shift 0..7 to bright variants 8..15
        let i = if bold && idx < 8 { (idx + 8) as usize } else { idx as usize };
        palette.colors[i]
    } else if idx < 232 {
        // 6x6x6 color cube
        let n = idx - 16;
        let r = (n / 36) % 6;
        let g = (n / 6) % 6;
        let b = n % 6;
        let to_byte = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        ColorU::new(to_byte(r), to_byte(g), to_byte(b), 0xFF)
    } else {
        // 232..255: grayscale ramp
        let v = 8 + (idx - 232).saturating_mul(10);
        ColorU::new(v, v, v, 0xFF)
    }
}

/// Returns true when the cell's background is the terminal's default background.
fn is_default_bg(c: &AnsiColor) -> bool {
    matches!(c, AnsiColor::Named(NamedColor::Background))
}

/// Returns true when the cell's foreground is the terminal's default foreground.
fn is_default_fg(c: &AnsiColor) -> bool {
    matches!(
        c,
        AnsiColor::Named(NamedColor::Foreground)
            | AnsiColor::Named(NamedColor::BrightForeground)
            | AnsiColor::Named(NamedColor::DimForeground)
    )
}

/// Pick a high-contrast foreground when a cell has the default fg but an
/// explicit background.  TUI programs designed for dark themes routinely
/// emit `bg=black` while leaving `fg` at the default, expecting the
/// terminal default fg to be light.  On a light theme that produces dark
/// text on a dark bar, which is unreadable; this helper switches to a
/// light fg whenever the explicit bg is dark enough.
fn auto_contrast_fg(bg: ColorU, palette: &ColorPalette) -> ColorU {
    // Rec. 601 luminance approximation, sufficient for picking light vs. dark.
    let r = bg.r as f32;
    let g = bg.g as f32;
    let b = bg.b as f32;
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    if lum < 128.0 {
        // Dark bg: use the brightest neutral from the palette.
        palette.colors[15] // BrightWhite
    } else {
        // Light bg: keep the theme's foreground colour.
        palette.foreground
    }
}

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

        // ── Render each row by grouping consecutive cells with the same
        // (fg, bg, flags) into runs.  Each run is a single Text element,
        // optionally wrapped in a Container when it has a non-default
        // background colour.  This keeps Flex::row sub-pixel drift to a
        // minimum (one element per colour change instead of per character)
        // while still letting TUI programs draw coloured bars / panels.
        struct Run {
            text: String,
            fg: ColorU,
            bg: ColorU,
            has_bg: bool,
            is_cursor: bool,
        }

        let palette = &state.palette;
        let mut row_elements: Vec<Box<dyn Element>> = Vec::with_capacity(display_rows.len());
        for (row_idx, row) in display_rows.iter().enumerate() {
            let cells: &[Cell] = &row[..];
            let is_cursor_row = scroll_offset == 0
                && cursor_visible
                && row_idx + start == sb_len + cursor_row;

            let mut runs: Vec<Run> = Vec::new();
            for (col, cell) in cells.iter().enumerate() {
                let ch = if cell.c == '\0' || cell.c.is_ascii_control() {
                    ' '
                } else {
                    cell.c
                };
                let is_cursor = is_cursor_row && col == cursor_col;

                if is_cursor {
                    // Inverted cursor cell: solid cursor block, text uses bg colour.
                    runs.push(Run {
                        text: ch.to_string(),
                        fg: bg_color,
                        bg: cursor_color,
                        has_bg: true,
                        is_cursor: true,
                    });
                    continue;
                }

                let bold = cell.flags.contains(Flags::BOLD);
                let mut bg = resolve_color(&cell.bg, palette, false);
                let mut has_bg = !is_default_bg(&cell.bg);

                // Auto-contrast: a cell with default fg on an explicit bg
                // gets a fg derived from the bg's luminance, so TUI input
                // bars/panels remain readable on light themes.
                let mut fg = if has_bg && is_default_fg(&cell.fg) {
                    auto_contrast_fg(bg, palette)
                } else {
                    resolve_color(&cell.fg, palette, bold)
                };

                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                    has_bg = true;
                }

                // Hidden text: render as space using bg colour for fg.
                let render_ch = if cell.flags.contains(Flags::HIDDEN) { ' ' } else { ch };

                if let Some(last) = runs.last_mut() {
                    if !last.is_cursor
                        && last.fg == fg
                        && last.bg == bg
                        && last.has_bg == has_bg
                    {
                        last.text.push(render_ch);
                        continue;
                    }
                }
                runs.push(Run {
                    text: render_ch.to_string(),
                    fg,
                    bg,
                    has_bg,
                    is_cursor: false,
                });
            }

            let mut row_els: Vec<Box<dyn Element>> = Vec::with_capacity(runs.len());
            for run in runs {
                let text_el = Text::new_inline(run.text, fid, FONT_SIZE)
                    .with_color(run.fg)
                    .with_line_height_ratio(1.0)
                    .finish();
                if run.has_bg {
                    row_els.push(
                        Container::new(text_el)
                            .with_background_color(run.bg)
                            .finish(),
                    );
                } else {
                    row_els.push(text_el);
                }
            }

            if row_els.len() == 1 {
                row_elements.push(row_els.into_iter().next().unwrap());
            } else {
                row_elements.push(Flex::row().with_children(row_els).finish());
            }
        }

        let grid = Flex::column()
            .with_children(row_elements)
            .finish();

        // Top+left padding: avoid macOS traffic-light buttons
        let padded_grid = Container::new(grid)
            .with_padding(Padding::uniform(0.0).with_top(28.0).with_left(6.0))
            .finish();

        let content = Stack::new()
            .with_child(Rect::new().with_background_color(bg_color).finish())
            .with_child(padded_grid)
            .finish();

        let pty = self.pty.clone();
        let pty_for_text = self.pty.clone();
        let scroll_offset_rc = self.scroll_offset.clone();
        let max_scroll = sb_len;

        EventHandler::new(content)
            .on_keydown(
                move |_event_ctx: &mut EventContext,
                      _app: &AppContext,
                      keystroke: &warpui::keymap::Keystroke,
                      _chars: &str,
                      is_composing: bool|
                      -> DispatchEventResult {
                    // When the IME (e.g. macOS Chinese pinyin) is composing,
                    // the macOS host view dispatches KeyDown with
                    // `is_composing = true`. We must NOT forward the raw
                    // keystroke to the PTY in that case (otherwise pinyin
                    // letters leak through), AND we must return
                    // `PropagateToParent` so the host view's `handled` flag
                    // stays false and the subsequent committed text can be
                    // delivered via `TypedCharacters` (`on_typed_characters`).
                    if is_composing {
                        return DispatchEventResult::PropagateToParent;
                    }
                    let bytes = terminal_keystroke_to_bytes(keystroke);
                    if !bytes.is_empty() {
                        if let Ok(pty_guard) = pty.lock() {
                            let _ = pty_guard.write_input(&bytes);
                        }
                    }
                    DispatchEventResult::StopPropagation
                },
            )
            .on_typed_characters(
                move |_event_ctx: &mut EventContext,
                      _app: &AppContext,
                      chars: &str|
                      -> DispatchEventResult {
                    // IME-committed text (e.g. CJK characters).  Forward the
                    // raw UTF-8 bytes to the PTY so the shell receives them
                    // as if the user had typed them directly.
                    if !chars.is_empty() {
                        if let Ok(pty_guard) = pty_for_text.lock() {
                            let _ = pty_guard.write_input(chars.as_bytes());
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
