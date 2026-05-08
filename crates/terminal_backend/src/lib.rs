//! PureWarp terminal backend.
//!
//! Provides PTY session management, VTE parsing, and terminal state
//! for the PureWarp terminal emulator.

pub mod pty_session;
pub mod terminal_state;
pub mod vte_parser;

pub use pty_session::PtySession;
pub use terminal_state::{ColorPalette, CursorStyle, TermMode, TerminalState};
pub use vte_parser::VteHandler;
