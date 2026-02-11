#[cfg(test)]
mod tests {
    use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
    use std::io::{Read, Write};
    use std::sync::mpsc::channel;
    use std::thread;
    use std::time::Duration;

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

        #[cfg(windows)]
        const NEWLINE: &[u8] = b"\r\n";

        #[cfg(not(windows))]
        const NEWLINE: &[u8] = b"\n";

        // Set up the command to launch Bash with no profile, no rc, and empty prompt.
        let cmd = CommandBuilder::new(BASH_COMMAND);
        let mut child = pair.slave.spawn_command(cmd).unwrap();

        drop(pair.slave);

        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let mut reader = pair.master.try_clone_reader().unwrap();

        // Thread to read from the PTY and send data to the channel.
        let reader_handle = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        tx.send(output).unwrap();
                    }
                    Err(e) => {
                        tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                        break;
                    }
                }
            }
        });

        thread::sleep(Duration::from_millis(500));

        // Collect any initial output (e.g., startup sequences on Windows).
        let mut initial_output = String::new();
        while let Ok(chunk) = rx.try_recv() {
            initial_output.push_str(&chunk);
        }

        // On Windows, handle the cursor position query if present.
        #[cfg(windows)]
        let cleaned_initial = if initial_output.contains("\x1b[6n") {
            let mut writer = pair.master.take_writer().unwrap();
            writer.write_all(b"\x1b[1;1R").unwrap();
            initial_output.replace("\x1b[6n", "")
        } else {
            initial_output
        };

        #[cfg(not(windows))]
        let cleaned_initial = initial_output;

        // Now take the writer (on Unix, it's here; on Windows, if no query, take it now).
        #[cfg(not(windows))]
        let master_writer = pair.master.take_writer().unwrap();

        #[cfg(windows)]
        let master_writer = if initial_output.contains("\x1b[6n") {
            // Writer already taken above.
            unreachable!() // Adjust if needed; assumes query is always present on Windows startup.
        } else {
            pair.master.take_writer().unwrap()
        };

        // Thread to write input into the PTY.
        let writer_handle = thread::spawn(move || {
            let mut writer = master_writer;
            // Send a test command
            writer.write_all(b"echo hello").unwrap();
            writer.write_all(NEWLINE).unwrap();
            // Send exit
            writer.write_all(b"exit").unwrap();
            writer.write_all(NEWLINE).unwrap();
        });

        // Wait for writer to finish
        writer_handle.join().unwrap();

        // Wait for Bash to exit
        let status = child.wait().unwrap();

        // Collect all remaining output, prepending the cleaned initial.
        let mut collected_output = cleaned_initial;
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
        // Count occurrences to be more robust across platforms (should appear at least twice).
        let hello_count = collected_output.matches("hello").count();
        assert!(
            hello_count >= 2,
            "Output was: {:?}, 'hello' appeared {} times",
            collected_output,
            hello_count
        );
    }
}