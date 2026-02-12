#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
    use std::io::{Read, Write};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use std::thread;
    use std::time::Duration;

    #[cfg(windows)]
    const INTERACTIVE_PTY_COMMAND: &str = "cmd.exe"; // Use cmd.exe on Windows for testing

    #[cfg(target_os = "macos")]
    const INTERACTIVE_PTY_COMMAND: &str = "zsh";

    #[cfg(all(not(windows), not(target_os = "macos")))]
    const INTERACTIVE_PTY_COMMAND: &str = "bash";

    #[cfg(windows)]
    const NEWLINE: &[u8] = b"\r\n";

    #[cfg(not(windows))]
    const NEWLINE: &[u8] = b"\n";

    #[test]
    #[timeout(5000)]
    fn slow_reader_no_read_pipe() {
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

        let PtyPair { master, slave } = pair;

        // Set up the command to launch interactive terminal
        let cmd = CommandBuilder::new(INTERACTIVE_PTY_COMMAND);
        let child = Arc::new(Mutex::new(slave.spawn_command(cmd).unwrap()));

        drop(slave);

        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let mut reader = master.try_clone_reader().unwrap();
        let mut master_writer = master.take_writer().unwrap();

        println!("reading initial chunk");
        let mut buffer = [0u8; 1024];
        match reader.read(&mut buffer) {
            Ok(n) => {
                // add a for loop that printlns every character as ascii code
                // for debugging purposes
                for (i, byte) in buffer[..n].iter().enumerate() {
                    println!("{}\t{}\t{}", i, byte, *byte as char);
                }
                let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                if !output.is_empty() {
                    tx.send(output).unwrap();
                }
            }
            Err(e) => {
                tx.send(format!("Error reading from PTY: {}", e)).unwrap();
            }
        }

        // Send a test command
        println!("send echo hello command");
        master_writer.write_all(b"echo hello").unwrap();
        master_writer.write_all(NEWLINE).unwrap();

        // Send exit
        println!("send exit command");
        master_writer.write_all(b"exit").unwrap();
        master_writer.write_all(NEWLINE).unwrap();

        // Wait for Bash to exit
        println!("Waiting for bash to exit...");
        let status = child.lock().unwrap().wait().unwrap();

        println!("dropping ressources");
        drop(master_writer); // Close the writer to signal EOF to the reader thread
        drop(master); // Close the master to ensure the reader thread can exit

        println!("sleeping");
        std::thread::sleep(Duration::from_millis(500));

        println!("starting reader thread");
        // Thread to read from the PTY and send data to the channel.
        let reader_handle = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        // add a for loop that printlns every character as ascii code
                        // for debugging purposes
                        for (i, byte) in buffer[..n].iter().enumerate() {
                            println!("{}\t{}\t{}", i, byte, *byte as char);
                        }
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        if !output.is_empty() {
                            tx.send(output).unwrap();
                        }
                    }
                    Err(e) => {
                        tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                        break;
                    }
                }
            }
        });

        // Wait for reader to finish
        println!("Wait for reader to finish...");
        reader_handle.join().unwrap();

        // Collect all output from the channel
        println!("Collecting output from the channel...");
        let mut collected_output = String::new();
        while let Ok(chunk) = rx.try_recv() {
            collected_output.push_str(&chunk);
        }

        // const STATUS_CONTROL_C_EXIT: u32 = 0xC000013A;

        assert!(
            status.success(),
            "{} exited with status: {:?}, output: {}",
            INTERACTIVE_PTY_COMMAND,
            status,
            collected_output
        );

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
        assert!(
            collected_output.contains("exit"),
            "Output was: {:?}, expected to contain 'exit'",
            collected_output
        );
    }

    #[test]
    #[timeout(5000)]
    fn slow_reader_read_pipe_slow() {
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

        let PtyPair { master, slave } = pair;

        // Set up the command to launch Bash with no profile, no rc, and empty prompt.
        let cmd = CommandBuilder::new(INTERACTIVE_PTY_COMMAND);
        let child = Arc::new(Mutex::new(slave.spawn_command(cmd).unwrap()));

        drop(slave);

        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let mut reader = master.try_clone_reader().unwrap();
        let mut master_writer = master.take_writer().unwrap();

        // Send a test command
        master_writer.write_all(b"echo hello").unwrap();
        master_writer.write_all(NEWLINE).unwrap();

        // Send exit
        master_writer.write_all(b"exit").unwrap();
        master_writer.write_all(NEWLINE).unwrap();

        // Thread to read from the PTY and send data to the channel.
        let reader_handle = thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        // add a for loop that printlns every character as ascii code
                        // for debugging purposes
                        for (i, byte) in buffer[..n].iter().enumerate() {
                            println!("{}\t{}\t{}", i, byte, *byte as char);
                        }
                        let output = String::from_utf8_lossy(&buffer[..n]).to_string();
                        if !output.is_empty() {
                            tx.send(output).unwrap();
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    Err(e) => {
                        tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                        break;
                    }
                }
            }
        });

        // Wait for Bash to exit
        println!("Waiting for bash to exit...");
        let status = child.lock().unwrap().wait().unwrap();

        drop(master_writer); // Close the writer to signal EOF to the reader thread
        drop(master); // Close the master to ensure the reader thread can exit

        // Wait for reader to finish
        println!("Wait for reader to finish...");
        reader_handle.join().unwrap();

        // Collect all output from the channel
        println!("Collecting output from the channel...");
        let mut collected_output = String::new();
        while let Ok(chunk) = rx.try_recv() {
            collected_output.push_str(&chunk);
        }

        // const STATUS_CONTROL_C_EXIT: u32 = 0xC000013A;

        assert!(
            status.success(),
            "{} exited with status: {:?}, output: {}",
            INTERACTIVE_PTY_COMMAND,
            status,
            collected_output
        );

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
        assert!(
            collected_output.contains("exit"),
            "Output was: {:?}, expected to contain 'exit'",
            collected_output
        );
        assert!(false)
    }
}
