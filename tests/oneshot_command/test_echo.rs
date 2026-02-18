#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
    use std::io::{Read};
    use std::sync::mpsc::channel;

    use std::thread;
    
    const COMMAND: &str = "echo";

    #[test]
    #[timeout(5000)]
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

        let PtyPair { master, slave } = pair;

        // Set up the command to launch Bash with no profile, no rc, and empty prompt.
        let mut cmd = CommandBuilder::new(COMMAND);
        cmd.arg("hello");
        let mut child = slave.spawn_command(cmd).unwrap();

        drop(slave);
        
        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let mut reader = master.try_clone_reader().unwrap();

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

        
        // Wait for Bash to exit
        println!("Waiting for bash to exit...");
        let status = child.wait().unwrap();
        
        drop(master); // Close the master to signal EOF to the reader pipe


        // Wait for reader to finish
        println!("Wait for reader to finish...");
        reader_handle.join().unwrap();

        // Collect all output from the channel
        println!("Collecting output from the channel...");
        let mut collected_output = String::new();
        while let Ok(chunk) = rx.try_recv() {
            collected_output.push_str(&chunk);
        }

        assert!(
            status.success(),
            "{} exited with status: {:?}, output: {}",
            COMMAND,
            status,
            collected_output
        );

        // Assert that the output contains the expected echo result
        // Expected: "echo hello" echoed back (due to terminal echo), then "hello"
        // Count occurrences to be more robust across platforms (should appear at least twice).
        let hello_count = collected_output.matches("hello").count();
        assert!(
            hello_count == 1,
            "Output was: {:?}, 'hello' appeared {} times",
            collected_output,
            hello_count
        );
    }
}
