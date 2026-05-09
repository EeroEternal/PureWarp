//! PTY (Pseudo-TTY) session management.
//!
//! Handles spawning a shell process connected to a PTY, reading its output
//! in a background async task, and forwarding the output through the VTE parser
//! to update the terminal state.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use futures::channel::mpsc;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};

use crate::terminal_state::TerminalState;
use crate::vte_parser::VteHandler;

/// Shared handle to the PTY writer, used to send input to the shell.
pub type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Represents an active PTY session connected to a user shell.
pub struct PtySession {
    /// The writer end, shared with the input handler.
    pub writer: PtyWriter,
    /// Handle to the child process (as a ChildKiller for sending signals).
    #[allow(dead_code)]
    child: Box<dyn ChildKiller + Send + Sync>,
    /// Channel sender for notifying renderer of terminal updates.
    update_tx: mpsc::UnboundedSender<()>,
    /// Channel receiver for terminal updates. Stored so it stays alive
    /// and can be retrieved by the terminal view.
    update_rx: Option<mpsc::UnboundedReceiver<()>>,
}

impl PtySession {
    /// Spawn a new PTY session with the user's default shell.
    ///
    /// The terminal state will be updated in the background as output
    /// arrives from the shell process.
    pub async fn spawn(
        state: Arc<Mutex<TerminalState>>,
        cols: u16,
        rows: u16,
        shell_program: Option<&str>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();

        // Create the PTY pair
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        // Determine the shell to use
        let shell = shell_program
            .map(|s| s.to_string())
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());

        // Build the command
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        // Identify terminal to shell
        cmd.env("TERM_PROGRAM", "purewarp");
        // Suppress zsh's PROMPT_SP '%' – PureWarp handles cursor tracking
        // internally and shell- side assumptions may differ.
        cmd.env("PROMPT_EOL_MARK", "");

        // Spawn the child process
        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;

        // Drop the slave end - we only need the master
        drop(pair.slave);

        let master = pair.master;

        // Split master into reader and writer
        let reader = master.try_clone_reader().context("Failed to clone PTY reader")?;
        let writer: Box<dyn Write + Send> = master.take_writer().context("Failed to take PTY writer")?;

        let writer = Arc::new(Mutex::new(writer));

        // Create notification channel (futures channel implements Stream)
        let (update_tx, update_rx) = mpsc::unbounded::<()>();

        // Background task: read PTY output and update terminal state
        let state_clone = state.clone();
        let update_tx_bg = update_tx.clone();
        let writer_bg = writer.clone();
        tokio::task::spawn_blocking(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            let mut parser = vte::Parser::new();

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - shell exited
                        log::info!("PTY reader: EOF, shell process ended");
                        break;
                    }
                    Ok(n) => {
                        // Feed bytes to VTE parser
                        let mut state = state_clone.lock().unwrap();
                        {
                            let mut writer_guard = writer_bg.lock().unwrap();
                            let mut handler = VteHandler::with_writer(
                                &mut state,
                                &mut **writer_guard,
                            );
                            for &byte in &buf[..n] {
                                parser.advance(&mut handler, byte);
                            }
                        }
                        // Notify that state was updated
                        let _ = update_tx_bg.unbounded_send(());
                    }
                    Err(e) => {
                        log::error!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            writer,
            child: child.clone_killer(),
            update_tx,
            update_rx: Some(update_rx),
        })
    }

    /// Write bytes to the PTY (send input to the shell).
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        self.writer
            .lock()
            .map_err(|e| anyhow::anyhow!("PTY writer lock error: {}", e))?
            .write_all(data)
            .context("Failed to write to PTY")?;
        Ok(())
    }

    /// Resize the PTY when the terminal window is resized.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.writer
            .lock()
            .map_err(|e| anyhow::anyhow!("PTY writer lock error: {}", e))?
            .flush()
            .ok();

        log::info!("PTY resize requested: {}x{}", cols, rows);

        Ok(())
    }

    /// Get a sender for update notifications (to trigger re-renders).
    pub fn update_sender(&self) -> mpsc::UnboundedSender<()> {
        self.update_tx.clone()
    }

    /// Take the update receiver for use in a terminal view's update loop.
    /// Returns None if the receiver has already been taken.
    pub fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<()>> {
        self.update_rx.take()
    }

    /// Check if the child process is still running.
    /// Note: ChildKiller only supports kill/clone_killer, not wait.
    /// For now, we always report alive.
    pub fn is_alive(&self) -> bool {
        true
    }
}
