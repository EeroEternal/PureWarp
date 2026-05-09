//! Terminal state management.
//!
//! Encapsulates the terminal grid (scrollback + visible rows), cursor position,
//! terminal modes, and provides the interface that both the VTE parser and the
//! rendering layer use.

use warp_terminal::model::grid::cell::{Cell, Flags};
use warp_terminal::model::grid::row::Row;
use warp_terminal::model::grid::Dimensions;

/// Represents the ANSI color palette for the terminal (16 standard colors).
#[derive(Debug, Clone)]
pub struct ColorPalette {
    pub colors: [pathfinder_color::ColorU; 16],
    pub foreground: pathfinder_color::ColorU,
    pub background: pathfinder_color::ColorU,
    pub cursor: pathfinder_color::ColorU,
}

impl Default for ColorPalette {
    fn default() -> Self {
        use pathfinder_color::ColorU;

        Self {
            // Standard ANSI colors
            colors: [
                ColorU::new(0x00, 0x00, 0x00, 0xFF), // Black
                ColorU::new(0xCC, 0x00, 0x00, 0xFF), // Red
                ColorU::new(0x4E, 0x9A, 0x06, 0xFF), // Green
                ColorU::new(0xC4, 0xA0, 0x00, 0xFF), // Yellow
                ColorU::new(0x34, 0x65, 0xA4, 0xFF), // Blue
                ColorU::new(0x75, 0x50, 0x7B, 0xFF), // Magenta
                ColorU::new(0x06, 0x98, 0x9A, 0xFF), // Cyan
                ColorU::new(0xD3, 0xD7, 0xCF, 0xFF), // White
                // Bright variants
                ColorU::new(0x55, 0x57, 0x53, 0xFF), // Bright Black
                ColorU::new(0xEF, 0x29, 0x29, 0xFF), // Bright Red
                ColorU::new(0x8A, 0xE2, 0x34, 0xFF), // Bright Green
                ColorU::new(0xFC, 0xE9, 0x4F, 0xFF), // Bright Yellow
                ColorU::new(0x72, 0x9F, 0xCF, 0xFF), // Bright Blue
                ColorU::new(0xAD, 0x7F, 0xA8, 0xFF), // Bright Magenta
                ColorU::new(0x34, 0xE2, 0xE2, 0xFF), // Bright Cyan
                ColorU::new(0xEE, 0xEE, 0xEC, 0xFF), // Bright White
            ],
            foreground: ColorU::new(0xD3, 0xD7, 0xCF, 0xFF),
            background: ColorU::new(0x00, 0x00, 0x00, 0xFF),
            cursor: ColorU::new(0xFF, 0xFF, 0xFF, 0xFF),
        }
    }
}

/// Cursor style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Underline,
    Beam,
}

impl Default for CursorStyle {
    fn default() -> Self {
        CursorStyle::Block
    }
}

/// Terminal mode flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct TermMode {
    /// Application Cursor Keys (DECCKM)
    pub app_cursor: bool,
    /// Application Keypad (DECKPAM)
    pub app_keypad: bool,
    /// Insert mode
    pub insert: bool,
    /// Bracketed paste mode
    pub bracketed_paste: bool,
    /// Origin mode (DECOM)
    pub origin: bool,
    /// Auto-wrap mode (DECAWM)
    pub auto_wrap: bool,
    /// Show cursor
    pub show_cursor: bool,
    /// Alternate screen
    pub alt_screen: bool,
}

/// Tracks cursor position within the terminal grid.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    /// Saved position (for save/restore cursor ESC sequences)
    pub saved_row: usize,
    pub saved_col: usize,
    pub style: CursorStyle,
    pub visible: bool,
}

/// The complete terminal state: grid, cursor, modes, and color palette.
pub struct TerminalState {
    /// Visible rows of the terminal grid. Index 0 = top of visible area.
    visible_rows: Vec<Row>,
    /// Scrollback rows (lines scrolled off the top).
    scrollback: Vec<Row>,
    /// Number of columns in the grid.
    pub(crate) cols: usize,
    /// Number of visible rows.
    pub(crate) rows: usize,
    /// Maximum scrollback lines to keep.
    max_scrollback: usize,
    /// Current cursor position.
    pub cursor: Cursor,
    /// Terminal modes.
    pub mode: TermMode,
    /// Color palette.
    pub palette: ColorPalette,
    /// Current SGR attributes (built up from CSI SGR sequences).
    pub current_fg: Option<usize>, // index into palette or None for default
    pub current_bg: Option<usize>,
    pub current_flags: Flags,
    /// Scroll region (top, bottom inclusive). None means full screen.
    pub scroll_region: Option<(usize, usize)>,
    /// Whether the terminal has been updated since the last render.
    pub dirty: bool,
}

impl Dimensions for TerminalState {
    fn total_rows(&self) -> usize {
        self.scrollback.len() + self.visible_rows.len()
    }

    fn visible_rows(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl TerminalState {
    /// Create a new terminal state with the given dimensions.
    pub fn new(cols: usize, rows: usize, max_scrollback: usize) -> Self {
        let visible_rows: Vec<Row> = (0..rows).map(|_| Row::new(cols)).collect();

        Self {
            visible_rows,
            scrollback: Vec::new(),
            cols,
            rows,
            max_scrollback,
            cursor: Cursor {
                visible: true,
                ..Default::default()
            },
            mode: TermMode {
                show_cursor: true,
                auto_wrap: true,
                ..Default::default()
            },
            palette: ColorPalette::default(),
            current_fg: None,
            current_bg: None,
            current_flags: Flags::empty(),
            scroll_region: None,
            dirty: true,
        }
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if self.cols == cols && self.rows == rows {
            return;
        }

        // Resize visible rows
        while self.visible_rows.len() < rows {
            self.visible_rows.push(Row::new(cols));
        }
        while self.visible_rows.len() > rows {
            let removed = self.visible_rows.remove(0);
            if !removed.is_clear() {
                self.scrollback.push(removed);
            }
        }

        // Resize each row's column count and collect overflows
        let mut overflows: Vec<(usize, Vec<warp_terminal::model::grid::cell::Cell>)> = Vec::new();
        for (row_idx, row) in self.visible_rows.iter_mut().enumerate() {
            if cols > self.cols {
                row.grow(cols);
            } else if let Some(overflow) = row.shrink(cols) {
                overflows.push((row_idx, overflow));
            }
        }

        // Insert overflow rows after the loop (avoids double borrow)
        for &(idx, ref overflow) in overflows.iter().rev() {
            let mut new_row = Row::new(cols);
            for (i, cell) in overflow.iter().enumerate() {
                if i < cols {
                    new_row[i] = cell.clone();
                }
            }
            if idx + 1 < self.visible_rows.len() {
                self.visible_rows.insert(idx + 1, new_row);
                self.visible_rows.pop();
            }
        }

        // Resize scrollback rows
        for row in &mut self.scrollback {
            if cols > self.cols {
                row.grow(cols);
            } else {
                row.shrink(cols);
            }
        }

        // Clamp cursor
        if self.cursor.col >= cols {
            self.cursor.col = cols.saturating_sub(1);
        }
        if self.cursor.row >= rows {
            self.cursor.row = rows.saturating_sub(1);
        }

        self.cols = cols;
        self.rows = rows;
        self.dirty = true;
    }

    /// Get a mutable reference to the cell at the current cursor position in visible space.
    pub fn cursor_cell_mut(&mut self) -> &mut Cell {
        &mut self.visible_rows[self.cursor.row][self.cursor.col]
    }

    /// Get a reference to a cell in visible space.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.visible_rows.get(row)?.get(col)
    }

    /// Move cursor to the next column, handling line wrap.
    pub fn advance_cursor(&mut self) {
        self.cursor.col += 1;
        if self.cursor.col >= self.cols {
            if self.mode.auto_wrap {
                self.cursor.col = 0;
                self.cursor.row += 1;
                if self.cursor.row >= self.rows {
                    self.scroll_up(1);
                    self.cursor.row = self.rows - 1;
                }
            } else {
                self.cursor.col = self.cols - 1;
            }
        }
    }

    /// Scroll the visible area up by n lines, pushing old lines into scrollback.
    pub fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(row) = self.visible_rows.first() {
                if !row.is_clear() {
                    self.scrollback.push(row.clone());
                    if self.scrollback.len() > self.max_scrollback {
                        self.scrollback.remove(0);
                    }
                }
            }
            self.visible_rows.remove(0);
            self.visible_rows.push(Row::new(self.cols));
        }
        self.dirty = true;
    }

    /// Scroll the visible area down by n lines.
    pub fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            self.visible_rows.pop();
            self.visible_rows.insert(0, Row::new(self.cols));
        }
        self.dirty = true;
    }

    /// Move cursor to a new position (1-based, as VTE sends it).
    pub fn set_cursor_pos(&mut self, row: u32, col: u32) {
        let row = (row.saturating_sub(1) as usize).min(self.rows.saturating_sub(1));
        let col = (col.saturating_sub(1) as usize).min(self.cols.saturating_sub(1));
        self.cursor.row = row;
        self.cursor.col = col;
    }

    /// Clear from cursor to end of line.
    pub fn clear_to_end_of_line(&mut self) {
        let row = self.cursor.row;
        let start = self.cursor.col;
        for col in start..self.cols {
            self.visible_rows[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Clear from beginning of line to cursor.
    pub fn clear_to_beginning_of_line(&mut self) {
        let row = self.cursor.row;
        let end = self.cursor.col;
        for col in 0..=end {
            self.visible_rows[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Clear entire line.
    pub fn clear_line(&mut self) {
        let row = self.cursor.row;
        for col in 0..self.cols {
            self.visible_rows[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Clear from cursor to end of screen.
    pub fn clear_to_end_of_screen(&mut self) {
        self.clear_to_end_of_line();
        for row in (self.cursor.row + 1)..self.rows {
            for col in 0..self.cols {
                self.visible_rows[row][col] = Cell::default();
            }
        }
        self.dirty = true;
    }

    /// Clear from beginning of screen to cursor.
    pub fn clear_to_beginning_of_screen(&mut self) {
        for row in 0..self.cursor.row {
            for col in 0..self.cols {
                self.visible_rows[row][col] = Cell::default();
            }
        }
        self.clear_to_beginning_of_line();
        self.dirty = true;
    }

    /// Clear entire screen.
    pub fn clear_screen(&mut self) {
        for row in 0..self.rows {
            for col in 0..self.cols {
                self.visible_rows[row][col] = Cell::default();
            }
        }
        self.dirty = true;
    }

    /// Get a reference to all visible rows.
    pub fn visible_rows_ref(&self) -> &[Row] {
        &self.visible_rows
    }

    /// Get a reference to all scrollback rows.
    pub fn scrollback_ref(&self) -> &[Row] {
        &self.scrollback
    }

    /// Number of lines in the scrollback buffer.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Get a mutable reference to all visible rows.
    pub fn visible_rows_mut(&mut self) -> &mut [Row] {
        &mut self.visible_rows
    }

    /// Write a character at the current cursor position and advance.
    pub fn write_char(&mut self, c: char) {
        if c == '\n' {
            self.cursor.col = 0;
            self.cursor.row += 1;
            if self.cursor.row >= self.rows {
                self.scroll_up(1);
                self.cursor.row = self.rows - 1;
            }
            return;
        }

        if c == '\r' {
            self.cursor.col = 0;
            return;
        }

        if c == '\t' {
            let tab_stop = 8;
            let next = ((self.cursor.col / tab_stop) + 1) * tab_stop;
            self.cursor.col = next.min(self.cols.saturating_sub(1));
            return;
        }

        // For backspace
        if c as u8 == 0x08 {
            if self.cursor.col > 0 {
                self.cursor.col -= 1;
            }
            return;
        }

        // Regular character — read current state before borrowing cell mutably
        let cur_fg = self.current_fg;
        let cur_bg = self.current_bg;
        let cur_flags = self.current_flags;
        {
            let cell = self.cursor_cell_mut();
            cell.c = c;
            if let Some(fg_idx) = cur_fg {
                cell.fg = warp_terminal::model::ansi::Color::Indexed(fg_idx as u8);
            }
            if let Some(bg_idx) = cur_bg {
                cell.bg = warp_terminal::model::ansi::Color::Indexed(bg_idx as u8);
            }
            cell.flags = cur_flags;
        }
        self.advance_cursor();
        self.dirty = true;
    }

    /// Set current SGR foreground color by ANSI index.
    pub fn set_fg_color(&mut self, idx: usize) {
        self.current_fg = Some(idx);
    }

    /// Set current SGR background color by ANSI index.
    pub fn set_bg_color(&mut self, idx: usize) {
        self.current_bg = Some(idx);
    }

    /// Reset foreground to default.
    pub fn reset_fg(&mut self) {
        self.current_fg = None;
    }

    /// Reset background to default.
    pub fn reset_bg(&mut self) {
        self.current_bg = None;
    }

    /// Reset all SGR attributes.
    pub fn reset_sgr(&mut self) {
        self.current_fg = None;
        self.current_bg = None;
        self.current_flags = Flags::empty();
    }

    /// Delete N characters at the cursor position, shifting remaining characters left.
    pub fn delete_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let start = self.cursor.col;
        let count = n.min(self.cols - start);
        for col in start..(self.cols - count) {
            self.visible_rows[row][col] =
                self.visible_rows[row][col + count].clone();
        }
        for col in (self.cols - count)..self.cols {
            self.visible_rows[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Erase N characters at the cursor position.
    pub fn erase_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let start = self.cursor.col;
        let end = (start + n).min(self.cols);
        for col in start..end {
            self.visible_rows[row][col] = Cell::default();
        }
        self.dirty = true;
    }

    /// Insert N blank lines at the cursor position.
    pub fn insert_lines(&mut self, n: usize) {
        let scroll_bottom = self
            .scroll_region
            .map(|(_, b)| b + 1)
            .unwrap_or(self.rows);
        for _ in 0..n {
            self.visible_rows.insert(self.cursor.row, Row::new(self.cols));
            self.visible_rows.remove(scroll_bottom);
        }
        self.dirty = true;
    }

    /// Delete N lines at the cursor position.
    pub fn delete_lines(&mut self, n: usize) {
        let scroll_bottom = self
            .scroll_region
            .map(|(_, b)| b + 1)
            .unwrap_or(self.rows);
        for _ in 0..n {
            self.visible_rows.remove(self.cursor.row);
            self.visible_rows.insert(
                scroll_bottom.saturating_sub(1).min(self.visible_rows.len()),
                Row::new(self.cols),
            );
        }
        self.dirty = true;
    }
}
