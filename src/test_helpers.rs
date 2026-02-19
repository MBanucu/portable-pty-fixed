use crate::{CommandBuilder, PtyPair, PtySize, native_pty_system};
use anyhow::Result;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
pub const SHELL_COMMAND: &str = "cmd.exe";
#[cfg(windows)]
pub const SHELL_ARGS: &[&str] = &[];

#[cfg(target_os = "macos")]
pub const SHELL_COMMAND: &str = "zsh";
#[cfg(target_os = "macos")]
pub const SHELL_ARGS: &[&str] = &["-f"];

#[cfg(all(not(windows), not(target_os = "macos")))]
pub const SHELL_COMMAND: &str = "bash";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const SHELL_ARGS: &[&str] = &[];

#[cfg(windows)]
pub const NEWLINE: &[u8] = b"\r\n";
#[cfg(not(windows))]
pub const NEWLINE: &[u8] = b"\n";

#[cfg(windows)]
pub const PROMPT_SIGN: &str = ">";
#[cfg(target_os = "macos")]
pub const PROMPT_SIGN: &str = " > ";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const PROMPT_SIGN: &str = "$";

pub struct ShellSession {
    pub child: Box<dyn crate::Child + Send + Sync>,
    pub child_pipe_tx: mpsc::Sender<String>,
    pub child_pipe_rx: mpsc::Receiver<String>,
    pub master: Box<dyn crate::MasterPty + Send>,
}

pub fn setup_shell_session() -> Result<ShellSession> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize::default())?;
    let PtyPair { master, slave } = pair;
    let mut reader = master.try_clone_reader()?;
    let mut cmd = CommandBuilder::new(SHELL_COMMAND);

    for arg in SHELL_ARGS {
        cmd.arg(arg);
    }

    cmd.env("PROMPT", PROMPT_SIGN);

    let child = slave.spawn_command(cmd)?;
    drop(slave);

    let (mut tx, rx) = mpsc::channel();
    let (tx_reader, rx_reader) = mpsc::channel();

    thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        let mut collected_output = String::new();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => panic!("Unexpected EOF"),
                Ok(n) => {
                    // add a for loop that printlns every character as ascii code
                    // for debugging purposes
                    // for (i, byte) in buffer[..n].iter().enumerate() {
                    //     println!("{}\t{}\t{}", i, byte, *byte as char);
                    // }

                    let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                    if !output.is_empty() {
                        tx.send(output.clone()).unwrap();
                        collected_output.push_str(&output);
                    }
                }
                Err(e) => {
                    panic!("Error reading from PTY: {}", e);
                }
            }
            if collected_output.contains(PROMPT_SIGN) {
                tx_reader.send(tx).unwrap();
                break;
            }
        }
    });

    tx = match rx_reader.recv_timeout(Duration::from_millis(1000)) {
        Ok(tx) => tx,
        Err(_) => panic!("Timeout waiting for shell prompt"),
    };

    Ok(ShellSession {
        child,
        child_pipe_tx: tx,
        child_pipe_rx: rx,
        master,
    })
}
