#[cfg(test)]
mod tests {
    use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
    use regex::Regex;
    use std::io::{Read, Write};
    use std::sync::mpsc::channel;
    use std::thread;

    #[test]
    fn test_bash_example() {
        let pty_system = NativePtySystem::default();

        // Open the PTY with a default size.
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        #[cfg(windows)]
        const BASH_COMMAND: &str = "cmd.exe"; // Use cmd.exe on Windows for testing

        #[cfg(not(windows))]
        const BASH_COMMAND: &str = "bash";

        // Set up the command to launch Bash with no profile, no rc, and empty prompt.
        let cmd = CommandBuilder::new(BASH_COMMAND);
        let mut child = pair.slave.spawn_command(cmd).unwrap();

        drop(pair.slave);

        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let master_writer = pair.master.take_writer().unwrap();

        // Thread to read from the PTY and send data to the channel.
        let reader_handle = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        print!("{}", output); // Print to stdout for visibility.
                        tx.send(output).unwrap();
                    }
                    Err(e) => {
                        tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                        break;
                    }
                }
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(500));

        // Thread to write input into the PTY.
        let writer_handle = thread::spawn(move || {
            let mut writer = master_writer;
            // Send a test command
            writer.write_all(b"echo hello\n").unwrap();
            // Send exit
            writer.write_all(b"exit\n").unwrap();
        });

        // Wait for writer to finish
        writer_handle.join().unwrap();

        // Wait for Bash to exit
        let status = child.wait().unwrap();

        // Collect all output
        let mut collected_output = String::new();
        while let Ok(chunk) = rx.try_recv() {
            collected_output.push_str(&chunk);
        }

        assert!(
            status.success(),
            "Bash exited with status: {:?}, output: {}",
            status,
            collected_output
        );

        // Wait for reader to finish
        reader_handle.join().unwrap();

        // Assert that the output contains the expected echo result
        // Expected: "echo hello" echoed back (due to terminal echo), then "hello"
        // But with PS1 empty and no rc, it should be minimal.
        // We check for "hello" appearing twice (command echo + output)
        assert!(
            Regex::new(r"hello.*\n.*hello")
                .unwrap()
                .is_match(&collected_output),
            "Output was: {:?}",
            collected_output
        );
    }
}
