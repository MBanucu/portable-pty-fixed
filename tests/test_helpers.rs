#[cfg(test)]
pub mod test_helpers {
    use anyhow::Result;
    use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
    use std::io::Write;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[cfg(windows)]
    const SHELL_COMMAND: &str = "cmd.exe"; // Use cmd.exe on Windows for testing
    #[cfg(windows)]
    const SHELL_ARGS: &[&str] = &[];

    #[cfg(target_os = "macos")]
    const SHELL_COMMAND: &str = "zsh";
    #[cfg(target_os = "macos")]
    const SHELL_ARGS: &[&str] = &["-f"];

    #[cfg(all(not(windows), not(target_os = "macos")))]
    const SHELL_COMMAND: &str = "bash";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    const SHELL_ARGS: &[&str] = &[];

    #[cfg(windows)]
    const NEWLINE: &[u8] = b"\r\n";
    #[cfg(not(windows))]
    const NEWLINE: &[u8] = b"\n";

    #[cfg(windows)]
    const PROMPT_SIGN: &str = ">";
    #[cfg(target_os = "macos")]
    const PROMPT_SIGN: &str = " > ";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    const PROMPT_SIGN: &str = "$";

    pub struct ShellSession {
        pub child: Box<dyn portable_pty::Child + Send + Sync>,
        pub child_pipe_tx: mpsc::Sender<String>,
        pub child_pipe_rx: mpsc::Receiver<String>,
        pub master: Box<dyn portable_pty::MasterPty + Send>,
    }

    pub fn setup_shell_session() -> Result<ShellSession> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize::default())?;
        let PtyPair { master, slave } = pair;
        let mut cmd = CommandBuilder::new(SHELL_COMMAND);

        for arg in SHELL_ARGS {
            let _ = cmd.arg(arg);
        }

        cmd.env("PROMPT", PROMPT_SIGN);

        let child = slave.spawn_command(cmd)?;
        drop(slave);

        let (mut tx, rx) = mpsc::channel();
        let (tx_reader, rx_reader) = mpsc::channel();
        let mut reader = master.try_clone_reader()?;

        thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            let mut collected_output = String::new();

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => panic!("Unexpected EOF"), // EOF
                    Ok(n) => {
                        // add a for loop that printlns every character as ascii code
                        // for debugging purposes
                        for (i, byte) in buffer[..n].iter().enumerate() {
                            println!("{}\t{}\t{}", i, byte, *byte as char);
                        }
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        if !output.is_empty() {
                            tx.send(output.clone()).unwrap();
                            collected_output.push_str(&output);
                            println!("collected_output: {}", collected_output);
                        }
                    }
                    Err(e) => {
                        panic!("Error reading from PTY: {}", e);
                    }
                }
                let find_str = PROMPT_SIGN;
                if collected_output.contains(find_str) {
                    println!("found {}", find_str);
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
}
