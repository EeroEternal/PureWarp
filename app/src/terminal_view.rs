//! Terminal keyboard input handling.
//!
//! Converts keystrokes to raw bytes for PTY forwarding.

use warp_terminal::model::escape_sequences::{KeystrokeWithDetails, ModeProvider, ToEscapeSequence};
use warp_terminal::model::TermMode;

/// Simple ModeProvider implementation for the terminal keyboard handler.
/// We use default terminal modes since we don't have access to the
/// live terminal state from the keydown callback context.
struct SimpleModeProvider {
    app_cursor: bool,
}

impl ModeProvider for SimpleModeProvider {
    fn is_term_mode_set(&self, mode: TermMode) -> bool {
        mode.contains(TermMode::APP_CURSOR) && self.app_cursor
    }
}


/// Convert a Keystroke to raw bytes to send to the PTY.
///
/// Handles special keys (enter, escape, tab, backspace, arrows, etc.)
/// and regular characters. Uses the existing escape sequence machinery
/// from warp_terminal for special keys.
pub fn terminal_keystroke_to_bytes(keystroke: &warpui::keymap::Keystroke) -> Vec<u8> {
    // Handle simple printable characters (no modifiers, single char key)
    if !keystroke.has_any_modifier() {
        // Check for special keys first
        match keystroke.key.as_str() {
            "enter" | "numpadenter" => return b"\r".to_vec(),
            "tab" => return b"\t".to_vec(),
            "escape" => return b"\x1b".to_vec(),
            "backspace" => return b"\x7f".to_vec(),
            "space" => return b" ".to_vec(),
            "delete" => return b"\x1b[3~".to_vec(),
            "insert" => return b"\x1b[2~".to_vec(),
            "pageup" => return b"\x1b[5~".to_vec(),
            "pagedown" => return b"\x1b[6~".to_vec(),
            "home" => return b"\x1b[H".to_vec(),
            "end" => return b"\x1b[F".to_vec(),
            "up" => return b"\x1b[A".to_vec(),
            "down" => return b"\x1b[B".to_vec(),
            "right" => return b"\x1b[C".to_vec(),
            "left" => return b"\x1b[D".to_vec(),
            // F-keys
            "f1" => return b"\x1bOP".to_vec(),
            "f2" => return b"\x1bOQ".to_vec(),
            "f3" => return b"\x1bOR".to_vec(),
            "f4" => return b"\x1bOS".to_vec(),
            "f5" => return b"\x1b[15~".to_vec(),
            "f6" => return b"\x1b[17~".to_vec(),
            "f7" => return b"\x1b[18~".to_vec(),
            "f8" => return b"\x1b[19~".to_vec(),
            "f9" => return b"\x1b[20~".to_vec(),
            "f10" => return b"\x1b[21~".to_vec(),
            "f11" => return b"\x1b[23~".to_vec(),
            "f12" => return b"\x1b[24~".to_vec(),
            _ => {}
        }

        // Single printable character
        if keystroke.key.chars().count() == 1 {
            return keystroke.key.as_bytes().to_vec();
        }
    }

    // Handle Ctrl-modified keys (C0 control codes)
    if keystroke.ctrl && !keystroke.alt && !keystroke.shift && !keystroke.meta {
        // Handle single-char Ctrl-modified keys (a-z, A-Z, space)
        if let Some(c) = keystroke.key.chars().next() {
            if keystroke.key.chars().count() == 1 {
                match c {
                    'a'..='z' | 'A'..='Z' => {
                        let code = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                        return vec![code];
                    }
                    ' ' => return vec![0x00],  // Ctrl+Space → NUL
                    _ => {}
                }
            }
        }
        // Handle named special keys with Ctrl
        match keystroke.key.as_str() {
            "[" => return vec![0x1b],  // Ctrl+[ → ESC
            "\\" => return vec![0x1c], // Ctrl+\ → FS
            "]" => return vec![0x1d],  // Ctrl+] → GS
            "^" => return vec![0x1e],  // Ctrl+^ → RS
            "_" => return vec![0x1f],  // Ctrl+_ → US
            "/" => return vec![0x1f],  // Ctrl+/ → US
            "2" => return vec![0x00],  // Ctrl+2 → NUL
            "6" => return vec![0x1e],  // Ctrl+6 → RS
            _ => {}
        }
    }

    // Use the escape sequence machinery for more complex keystrokes
    let mode_provider = SimpleModeProvider {
        app_cursor: false,
    };
    let details = KeystrokeWithDetails {
        keystroke,
        key_without_modifiers: None,
        chars: None,
    };
    if let Some(seq) = details.to_escape_sequence(&mode_provider) {
        return seq;
    }

    // Fallback: send the key as-is
    keystroke.key.as_bytes().to_vec()
}
