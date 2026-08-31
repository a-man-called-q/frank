//! Native cross-platform PTY boundary used by Take Control.
//!
//! Provider orchestration remains structured/headless.  This PTY is a shell in
//! the selected task worktree, with output forwarded as bounded terminal
//! frames.  A caller must hold the server-side lease before constructing one.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use frank_protocol::TerminalFrame;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

const OUTPUT_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("could not open PTY: {0}")]
    Open(String),
    #[error("PTY IO failed: {0}")]
    Io(String),
    #[error("PTY session is closed")]
    Closed,
}

pub type Result<T> = std::result::Result<T, PtyError>;

pub struct PtySession {
    pub session_id: String,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send>>>,
    output: Receiver<TerminalFrame>,
    exit_sent: Arc<Mutex<bool>>,
}

impl PtySession {
    pub fn spawn(session_id: impl Into<String>, spec: &super::ShellSpec) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: spec.rows.max(5),
                cols: spec.cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let shell = spec.shell.clone().unwrap_or_else(default_shell);
        let mut command = CommandBuilder::new(shell);
        command.cwd(&spec.cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Io(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Io(error.to_string()))?;
        let (tx, output) = mpsc::sync_channel(64);
        spawn_reader(reader, tx);
        Ok(Self {
            session_id: session_id.into(),
            master: Arc::new(Mutex::new(pair.master)),
            writer: Arc::new(Mutex::new(writer)),
            child: Arc::new(Mutex::new(child)),
            output,
            exit_sent: Arc::new(Mutex::new(false)),
        })
    }

    pub fn send_input(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > OUTPUT_CHUNK_BYTES {
            return Err(PtyError::Io("terminal input frame is too large".into()));
        }
        let mut writer = self.writer.lock().map_err(|_| PtyError::Closed)?;
        writer
            .write_all(bytes)
            .map_err(|error| PtyError::Io(error.to_string()))?;
        writer
            .flush()
            .map_err(|error| PtyError::Io(error.to_string()))?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self.master.lock().map_err(|_| PtyError::Closed)?;
        master
            .resize(PtySize {
                rows: rows.max(5),
                cols: cols.max(20),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Io(error.to_string()))
    }

    pub fn try_recv(&self) -> Result<Option<TerminalFrame>> {
        match self.output.try_recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(TryRecvError::Empty) => {
                let mut child = self.child.lock().map_err(|_| PtyError::Closed)?;
                let mut exit_sent = self.exit_sent.lock().map_err(|_| PtyError::Closed)?;
                if !*exit_sent
                    && let Some(status) = child
                        .try_wait()
                        .map_err(|error| PtyError::Io(error.to_string()))?
                {
                    *exit_sent = true;
                    return Ok(Some(TerminalFrame::Exit {
                        code: status.exit_code().min(i32::MAX as u32) as i32,
                    }));
                }
                Ok(None)
            }
            Err(TryRecvError::Disconnected) => Err(PtyError::Closed),
        }
    }

    pub fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().map_err(|_| PtyError::Closed)?;
        child
            .kill()
            .map_err(|error| PtyError::Io(error.to_string()))
    }
}

fn spawn_reader(mut reader: Box<dyn Read + Send>, tx: SyncSender<TerminalFrame>) {
    thread::spawn(move || {
        let mut buffer = vec![0_u8; OUTPUT_CHUNK_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if tx
                        .send(TerminalFrame::Output {
                            bytes: buffer[..size].to_vec(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(TerminalFrame::Error {
                        message: error.to_string(),
                    });
                    break;
                }
            }
        }
    });
}

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}
