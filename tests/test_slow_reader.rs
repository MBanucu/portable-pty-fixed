#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
    use std::io::{Read, Write};
    use std::sync::mpsc::{channel};
    use std::sync::{Arc, Mutex};

    use std::thread;
    use std::time::Duration;
    #[cfg(target_os = "macos")]
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
    const SHELL_COMMAND: &str = "zsh";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    const SHELL_ARGS: &[&str] = &["-f"];

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

    #[test]
    #[timeout(5000)]
    fn slow_reader_read_pipe_split() {
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
        let mut cmd = CommandBuilder::new(SHELL_COMMAND);
        for arg in SHELL_ARGS {
            let _ = cmd.arg(arg);
        }

        cmd.env("PROMPT", PROMPT_SIGN);

        let child = Arc::new(Mutex::new(slave.spawn_command(cmd).unwrap()));

        drop(slave);

        // Set up channels for collecting output.
        let (tx, rx) = channel::<String>();
        let tx_arc1 = Arc::new(Mutex::new(tx));
        let tx_arc2 = tx_arc1.clone();
        let reader_for_first_thread = Arc::new(Mutex::new(master.try_clone_reader().unwrap()));
        let reader_for_second_thread = reader_for_first_thread.clone();
        let master_writer = Arc::new(Mutex::new(master.take_writer().unwrap()));
        let master_writer_for_reader = master_writer.clone();

        // Thread to read from the PTY and send data to the channel.
        thread::spawn(move || {
            let mut buffer = [0u8; 1024];
            let mut collected_output = String::new();

            let mut state = 1;
            let mut reader = reader_for_first_thread.lock().unwrap();
            let tx = tx_arc1.lock().unwrap();

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
                            tx.send(output.clone()).unwrap();
                            collected_output.push_str(&output);
                            println!("collected_output: {}", collected_output)
                        }
                    }
                    Err(e) => {
                        tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                        break;
                    }
                }
                let mut find_str = PROMPT_SIGN;
                let mut find_state = 1;
                if state == 1 && collected_output.contains(find_str) {
                    println!("found {}", find_str);
                    // Send a test command
                    println!("sending test command");
                    master_writer_for_reader
                        .lock()
                        .unwrap()
                        .write_all(b"echo hello")
                        .unwrap();
                    master_writer_for_reader
                        .lock()
                        .unwrap()
                        .write_all(NEWLINE)
                        .unwrap();
                    state = find_state + 1;
                    let at = collected_output.find(find_str).unwrap();
                    collected_output = collected_output.split_off(at + find_str.len());
                }
                find_str = "echo hello";
                find_state += 1;
                if state == find_state && collected_output.contains(find_str) {
                    println!("found {}", find_str);
                    let at = collected_output.find(find_str).unwrap();
                    collected_output = collected_output.split_off(at + find_str.len());
                    state = find_state + 1;
                }
                find_str = "hello";
                find_state += 1;
                if state == find_state && collected_output.contains(find_str) {
                    println!("found {}", find_str);
                    let at = collected_output.find(find_str).unwrap();
                    collected_output = collected_output.split_off(at + find_str.len());
                    state = find_state + 1;
                }
                find_str = PROMPT_SIGN;
                find_state += 1;
                if state == find_state && collected_output.contains(find_str) {
                    println!("found {}", find_str);
                    // Send exit
                    println!("sending exit");
                    master_writer_for_reader
                        .lock()
                        .unwrap()
                        .write_all(b"exit")
                        .unwrap();
                    master_writer_for_reader
                        .lock()
                        .unwrap()
                        .write_all(NEWLINE)
                        .unwrap();
                    println!("stopping first reader thread");
                    break;
                }
            }
        });

        let (tx_waiter, rx_waiter) = channel();
        let waiter_handle = thread::spawn(move || {
            let status = child.lock().unwrap().wait().unwrap();
            let _ = tx_waiter.send(status).unwrap();
        });

        // Wait for shell to exit
        println!("Waiting for bash to exit...");
        match rx_waiter.recv_timeout(Duration::from_millis(100)) {
            Err(e) => {
                // macOS is expected to fail this test
                #[cfg(not(target_os = "macos"))]
                panic!("{}", e);
            }
            Ok(status) => {
                // macOS is expected to fail this test
                #[cfg(target_os = "macos")]
                panic!("Is macOS fixed or what? ExitStatus: {}" status);

                waiter_handle.join().unwrap();
                println!("child exit status received");

                println!("starting second reader thread");
                let reader_handle = thread::spawn(move || {
                    let mut buffer = [0u8; 1024];
                    let mut reader = reader_for_second_thread.lock().unwrap();
                    let tx = tx_arc2.lock().unwrap();
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
                                    tx.send(output.clone()).unwrap();
                                }
                            }
                            Err(e) => {
                                tx.send(format!("Error reading from PTY: {}", e)).unwrap();
                                break;
                            }
                        }
                    }
                });

                println!("dropping writer and master");
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

                assert!(
                    status.success(),
                    "{} exited with status: {:?}, output: {}",
                    SHELL_COMMAND,
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
        }
    }
}
