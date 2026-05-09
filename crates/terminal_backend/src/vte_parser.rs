//! VTE (Virtual Terminal Emulator) parser.
//!
//! Implements the `vte::Perform` trait to process ANSI/VT escape sequences
//! and update the terminal state accordingly.

use std::io::Write;

use crate::terminal_state::TerminalState;
use warp_terminal::model::grid::Dimensions;
use vte::{Params, Perform};

/// A VTE parser handler that writes into a `TerminalState`.
pub struct VteHandler<'a> {
    pub state: &'a mut TerminalState,
    /// Optional PTY writer for sending responses (e.g. DSR cursor position).
    pub writer: Option<&'a mut (dyn Write + Send)>,
}

impl<'a> VteHandler<'a> {
    pub fn new(state: &'a mut TerminalState) -> Self {
        Self { state, writer: None }
    }

    pub fn with_writer(
        state: &'a mut TerminalState,
        writer: &'a mut (dyn Write + Send),
    ) -> Self {
        Self {
            state,
            writer: Some(writer),
        }
    }
}

impl Perform for VteHandler<'_> {
    /// A printable character.
    fn print(&mut self, c: char) {
        self.state.write_char(c);
    }

    /// A C0 control character (0x00-0x1F, excluding ESC, and 0x7F).
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                // Line Feed
                self.state.cursor.col = 0; // Technically LF shouldn't reset col, but most shells do
                self.state.cursor.row += 1;
                if self.state.cursor.row >= self.state.rows {
                    self.state.scroll_up(1);
                    self.state.cursor.row = self.state.rows - 1;
                }
                self.state.dirty = true;
            }
            b'\r' => {
                // Carriage Return
                self.state.cursor.col = 0;
                self.state.dirty = true;
            }
            b'\t' => {
                // Horizontal Tab
                let tab_stop = 8;
                let next = ((self.state.cursor.col / tab_stop) + 1) * tab_stop;
                self.state.cursor.col = next.min(self.state.columns().saturating_sub(1));
                self.state.dirty = true;
            }
            0x08 => {
                // Backspace
                if self.state.cursor.col > 0 {
                    self.state.cursor.col -= 1;
                }
                self.state.dirty = true;
            }
            0x0B | 0x0C => {
                // Vertical Tab / Form Feed -> treated as newline
                self.state.cursor.row += 1;
                if self.state.cursor.row >= self.state.rows {
                    self.state.scroll_up(1);
                    self.state.cursor.row = self.state.rows - 1;
                }
                self.state.dirty = true;
            }
            0x07 => {
                // Bell - ignored for now
            }
            0x0E => {
                // Shift Out - switch to G1 charset (ignored)
            }
            0x0F => {
                // Shift In - switch to G0 charset (ignored)
            }
            _ => {}
        }
    }

    /// Start of an OSC, DCS, APC, SOS, or PM sequence.
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    /// A byte in an OSC, DCS, APC, SOS, or PM string.
    fn put(&mut self, _byte: u8) {}

    /// End of an OSC, DCS, APC, SOS, or PM string.
    fn unhook(&mut self) {}

    /// OSC (Operating System Command) dispatch.
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        // Parse OSC code
        let code_str = String::from_utf8_lossy(params[0]);
        let code: Result<u16, _> = code_str.parse();

        match code {
            Ok(0) | Ok(2) => {
                // OSC 0/2: Set window title (ignored for now)
            }
            Ok(1) => {
                // OSC 1: Set icon name (ignored for now)
            }
            Ok(4) => {
                // OSC 4: Set color
                if params.len() >= 3 {
                    let _color_idx: Result<u8, _> =
                        String::from_utf8_lossy(params[1]).parse();
                    // We could store custom palette colors here
                }
            }
            Ok(10) | Ok(11) => {
                // OSC 10/11: Set foreground/background color (query)
            }
            _ => {
                log::trace!("Unhandled OSC code: {}", code_str);
            }
        }
    }

    /// CSI (Control Sequence Introducer) dispatch.
    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'A' => {
                // Cursor Up
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.row = self.state.cursor.row.saturating_sub(n.max(1));
            }
            'B' => {
                // Cursor Down
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.row =
                    (self.state.cursor.row + n.max(1)).min(self.state.rows.saturating_sub(1));
            }
            'C' => {
                // Cursor Forward
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.col =
                    (self.state.cursor.col + n.max(1)).min(self.state.cols.saturating_sub(1));
            }
            'D' => {
                // Cursor Back
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.col = self.state.cursor.col.saturating_sub(n.max(1));
            }
            'E' => {
                // Cursor Next Line
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.col = 0;
                self.state.cursor.row =
                    (self.state.cursor.row + n.max(1)).min(self.state.rows.saturating_sub(1));
            }
            'F' => {
                // Cursor Previous Line
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.col = 0;
                self.state.cursor.row = self.state.cursor.row.saturating_sub(n.max(1));
            }
            'G' => {
                // Cursor Horizontal Absolute
                let col = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.col =
                    (col.saturating_sub(1)).min(self.state.cols.saturating_sub(1));
            }
            'H' | 'f' => {
                // Cursor Position
                let row = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1);
                let col = params
                    .iter()
                    .nth(1)
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1);
                self.state.set_cursor_pos(row as u32, col as u32);
            }
            'J' => {
                // Erase in Display
                match params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0) {
                    0 => self.state.clear_to_end_of_screen(),
                    1 => self.state.clear_to_beginning_of_screen(),
                    2 | 3 => self.state.clear_screen(),
                    _ => {}
                }
            }
            'K' => {
                // Erase in Line
                match params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0) {
                    0 => self.state.clear_to_end_of_line(),
                    1 => self.state.clear_to_beginning_of_line(),
                    2 => self.state.clear_line(),
                    _ => {}
                }
            }
            'L' => {
                // Insert Lines
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.insert_lines(n.max(1));
            }
            'M' => {
                // Delete Lines
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.delete_lines(n.max(1));
            }
            'P' => {
                // Delete Characters
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.delete_chars(n.max(1));
            }
            'X' => {
                // Erase Characters
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.erase_chars(n.max(1));
            }
            'd' => {
                // Vertical Line Position Absolute
                let row = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                self.state.cursor.row =
                    (row.saturating_sub(1)).min(self.state.rows.saturating_sub(1));
            }
            'h' => {
                // Set Mode
                self.handle_set_mode(params);
            }
            'l' => {
                // Reset Mode
                self.handle_reset_mode(params);
            }
            'm' => {
                // Character Attributes (SGR)
                self.handle_sgr(params);
            }
            'n' => {
                // Device Status Report
                if params.iter().next().and_then(|p| p.first().copied()) == Some(6) {
                    // Cursor position report — respond with ESC[row;colR
                    let row = self.state.cursor.row.saturating_add(1);
                    let col = self.state.cursor.col.saturating_add(1);
                    let response = format!("\x1b[{};{}R", row, col);
                    log::trace!("DSR: cursor position report -> {}", response);
                    if let Some(ref mut writer) = self.writer {
                        let _ = writer.write_all(response.as_bytes());
                        let _ = writer.flush();
                    }
                }
            }
            'r' => {
                // Set Scrolling Region
                let top = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1);
                let bottom = params
                    .iter()
                    .nth(1)
                    .and_then(|p| p.first().copied())
                    .unwrap_or(self.state.rows as u16);
                if top < bottom && bottom as usize <= self.state.rows {
                    self.state.scroll_region =
                        Some((top.saturating_sub(1) as usize, bottom.saturating_sub(1) as usize));
                } else {
                    self.state.scroll_region = None;
                }
            }
            's' => {
                // Save Cursor Position
                self.state.cursor.saved_row = self.state.cursor.row;
                self.state.cursor.saved_col = self.state.cursor.col;
            }
            'u' => {
                // Restore Cursor Position
                self.state.cursor.row = self.state.cursor.saved_row;
                self.state.cursor.col = self.state.cursor.saved_col;
            }
            '@' => {
                // Insert Characters — shift right and clear gap
                let n = params.iter().next().and_then(|p| p.first().copied()).unwrap_or(1) as usize;
                let row = self.state.cursor.row;
                let start = self.state.cursor.col;
                let count = n.min(self.state.cols - start);
                // Shift cells right
                for col in (start..(self.state.cols - count)).rev() {
                    let cell = self.state.visible_rows_mut()[row][col].clone();
                    self.state.visible_rows_mut()[row][col + count] = cell;
                }
                // Clear the gap
                for col in start..(start + count) {
                    self.state.visible_rows_mut()[row][col] = Default::default();
                }
                self.state.dirty = true;
            }
            _ => {
                log::trace!(
                    "Unhandled CSI action: {} with params {:?}",
                    action,
                    params.iter().map(|p| p[0]).collect::<Vec<_>>()
                );
            }
        }
    }

    /// ESC dispatch (escape sequences that start with ESC but are not CSI, OSC, etc.).
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                // DECSC: Save Cursor
                self.state.cursor.saved_row = self.state.cursor.row;
                self.state.cursor.saved_col = self.state.cursor.col;
            }
            b'8' => {
                // DECRC: Restore Cursor
                self.state.cursor.row = self.state.cursor.saved_row;
                self.state.cursor.col = self.state.cursor.saved_col;
            }
            b'D' => {
                // IND: Index (move cursor down, scroll if needed)
                self.state.cursor.row += 1;
                if self.state.cursor.row >= self.state.rows {
                    self.state.scroll_up(1);
                    self.state.cursor.row = self.state.rows - 1;
                }
                self.state.dirty = true;
            }
            b'M' => {
                // RI: Reverse Index (move cursor up, scroll if needed)
                if self.state.cursor.row > 0 {
                    self.state.cursor.row -= 1;
                } else {
                    self.state.scroll_down(1);
                }
                self.state.dirty = true;
            }
            b'E' => {
                // NEL: Next Line
                self.state.cursor.col = 0;
                self.state.cursor.row += 1;
                if self.state.cursor.row >= self.state.rows {
                    self.state.scroll_up(1);
                    self.state.cursor.row = self.state.rows - 1;
                }
                self.state.dirty = true;
            }
            b'H' => {
                // HTS: Horizontal Tab Set (ignored for now)
            }
            b'c' => {
                // RIS: Reset to Initial State
                self.state.clear_screen();
                self.state.cursor = Default::default();
                self.state.cursor.visible = self.state.mode.show_cursor;
                self.state.reset_sgr();
                self.state.scroll_region = None;
                self.state.dirty = true;
            }
            _ => {
                log::trace!("Unhandled ESC byte: 0x{:02x} ({})", byte, byte as char);
            }
        }
    }
}

impl VteHandler<'_> {
    /// Handle CSI Set Mode (CSI ? ... h or CSI ... h)
    fn handle_set_mode(&mut self, params: &Params) {
        for param in params.iter() {
            match param[0] {
                1 => self.state.mode.app_cursor = true, // Cursor Keys Mode
                3 => {
                    // DECCOLM: 132-column mode (ignored)
                }
                4 => {
                    // Insert mode
                    self.state.mode.insert = true;
                }
                6 => {
                    // DECOM: Origin mode
                    self.state.mode.origin = true;
                }
                7 => self.state.mode.auto_wrap = true, // Auto-wrap
                25 => {
                    self.state.mode.show_cursor = true;
                    self.state.cursor.visible = true;
                }
                2004 => self.state.mode.bracketed_paste = true,
                _ => {}
            }
        }
    }

    /// Handle CSI Reset Mode (CSI ? ... l or CSI ... l)
    fn handle_reset_mode(&mut self, params: &Params) {
        for param in params.iter() {
            match param[0] {
                1 => self.state.mode.app_cursor = false,
                4 => self.state.mode.insert = false,
                6 => self.state.mode.origin = false,
                7 => self.state.mode.auto_wrap = false,
                25 => {
                    self.state.mode.show_cursor = false;
                    self.state.cursor.visible = false;
                }
                2004 => self.state.mode.bracketed_paste = false,
                _ => {}
            }
        }
    }

    /// Handle SGR (Select Graphic Rendition) parameters.
    fn handle_sgr(&mut self, params: &Params) {
        let params: Vec<u16> = params.iter().map(|p| p.first().copied().unwrap_or(0)).collect();

        if params.is_empty() || params == [0] {
            self.state.reset_sgr();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.state.reset_sgr(),
                1 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::BOLD;
                }
                2 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::DIM;
                }
                3 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::ITALIC;
                }
                4 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::UNDERLINE;
                }
                5 | 6 => {
                    // Slow/fast blink - not supported
                }
                7 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::INVERSE;
                }
                8 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::HIDDEN;
                }
                9 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::STRIKEOUT;
                }
                21 => {
                    self.state.current_flags |=
                        warp_terminal::model::grid::cell::Flags::DOUBLE_UNDERLINE;
                }
                22 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::BOLD
                            | warp_terminal::model::grid::cell::Flags::DIM,
                    );
                }
                23 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::ITALIC,
                    );
                }
                24 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::UNDERLINE
                            | warp_terminal::model::grid::cell::Flags::DOUBLE_UNDERLINE,
                    );
                }
                25 => {
                    // Blink off
                }
                27 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::INVERSE,
                    );
                }
                28 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::HIDDEN,
                    );
                }
                29 => {
                    self.state.current_flags.remove(
                        warp_terminal::model::grid::cell::Flags::STRIKEOUT,
                    );
                }
                30..=37 => {
                    // Standard foreground colors
                    self.state.set_fg_color((params[i] - 30) as usize);
                }
                38 => {
                    // Extended foreground color
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        // 256-color mode
                        self.state.set_fg_color(params[i + 2] as usize);
                        i += 2;
                    }
                }
                39 => {
                    self.state.reset_fg();
                }
                40..=47 => {
                    // Standard background colors
                    self.state.set_bg_color((params[i] - 40) as usize);
                }
                48 => {
                    // Extended background color
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.state.set_bg_color(params[i + 2] as usize);
                        i += 2;
                    }
                }
                49 => {
                    self.state.reset_bg();
                }
                90..=97 => {
                    // Bright foreground colors
                    self.state.set_fg_color((params[i] - 90 + 8) as usize);
                }
                100..=107 => {
                    // Bright background colors
                    self.state.set_bg_color((params[i] - 100 + 8) as usize);
                }
                _ => {
                    log::trace!("Unhandled SGR param: {}", params[i]);
                }
            }
            i += 1;
        }
        self.state.dirty = true;
    }
}
