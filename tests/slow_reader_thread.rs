#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
    use std::io::{Read, Write};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    /**
     * Conclusions of this test:
     * - [macOS] the shell process is not continuing without a reader thread continuously reading from the pipe.
     * - On every system this pattern can be used to handle creation, data collection and exit handling to gracefully shut down the PTY.
     * - This pattern ensures that no data is being lost.
     * - Waiting for the child exit before dropping master or writer garuantees that
     *   - there is an EOF written to the reader pipe and not somehow in the middle of the data.
     *   - the child exit code is not influenced by the observer that wants to gracefully handle the data.
     * - [Windows] You have to drop the master or the writer for the reader pipe to get EOF or abort written to the reader pipe.
     *   - Dropping the master or the writer when the child did not exit yet will
     *     - exit the child with signal Ctrl+C
     *     - close the reader pipe
     *     - delete the buffer of the reader pipe, data is lost
     *   - Dropping the master or the writer when child did already exit will just close the pipe with EOF but will not influence the buffer in the pipe.
     * - [macOS, Linux] You do not have to drop the master or the writer. The reader pipe is closing automatically after child exit.
     * - [Windows] Dropping master or writer after child exit garuantees that there is an EOF written at the end of the pipe, even if master or writer is dropped before the data is read from the pipe.
     *   In short: the dropping of master or writer after child exit does not influence the buffer of the reader pipe.
     * - The reader thread is exiting gracefully when EOF is being read (0-bytes-received-signal).
     * - Waiting for the reader thread to exit ensures that all data from the pipe is being drained to the storage of choice.
     *
     * Conclusion: The provided pattern in this test works for all systems.
     */
    #[test]
    #[timeout(5000)]
    fn slow_reader_thread() {
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
        let reader_for_first_thread = Arc::new(Mutex::new(master.try_clone_reader().unwrap()));
        let master_writer = Arc::new(Mutex::new(master.take_writer().unwrap()));
        let master_writer_for_reader = master_writer.clone();

        // Thread to read from the PTY and send data to the channel.
        let reader_handle = thread::spawn(move || {
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
                    let writer = master_writer_for_reader.clone();
                    // Send exit
                    thread::spawn(move || {
                        println!("sending exit");
                        writer.lock().unwrap().write_all(b"exit").unwrap();
                        writer.lock().unwrap().write_all(NEWLINE).unwrap();
                        println!(
                            "{}    [exit writer thread] time of exit written",
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        );
                    });
                    println!("stopping first reader thread");
                    println!(
                        "{}    [reader thread] time of start being slow",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                    );
                    // drop the ADDITIONAL lock reference to writer to be able to receive EOF (on Windows)
                    // Do not drop the moved/main writer in the reader loop (on Windows), it will kill the child (on Windows),
                    // when you still want to read data from the pipe.
                    drop(master_writer_for_reader);
                    thread::sleep(Duration::from_millis(200));
                    println!(
                        "{}    [reader thread] time of continuing reading",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                    );
                    // Continue the reader loop in this inner scope to make the code memory management safe,
                    // or you could say: to make rust happy.
                    // Because of the dropped writer, it has to be made sure that this is the last run of the loop
                    // so that the dropped writer can't be used by potentially following loops and cause panic.
                    // So a break statement has to follow after a drop statement in a loop.
                    // Sophisticated other patterns would challenge the rust compiler and the small human brain.
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
                    }
                    break;
                }
            }
        });

        println!("Waiting for shell to exit...");
        let status = child.lock().unwrap().wait().unwrap();
        println!("child exit status received");

        let child_exit_time = SystemTime::now();
        println!(
            "{}    [main thread] time of child exited",
            child_exit_time
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        println!("dropping writer and master");
        drop(master_writer); // Close the writer to signal EOF to the reader thread
        drop(master); // Close the master to signal another EOF to the reader thread, double is better than single

        // Collect all output from the channel
        println!("Collecting output from the channel...");
        let mut collected_output = String::new();
        while let Ok(chunk) = rx.recv() {
            collected_output.push_str(&chunk);
            if collected_output.contains("exit") {
                break;
            }
        }
        let echo_exit_received_time = SystemTime::now();
        println!(
            "{}    [main thread] time of exit signal received from reader thread",
            echo_exit_received_time
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        // Wait for reader to finish
        println!("Wait for reader to finish...");
        reader_handle.join().unwrap();

        // Collect all output from the channel
        println!("Collecting more output from the channel...");
        while let Ok(chunk) = rx.try_recv() {
            collected_output.push_str(&chunk);
        }

        let time_elapsed = echo_exit_received_time
            .duration_since(child_exit_time)
            .unwrap()
            .as_millis();
        #[cfg(not(target_os = "macos"))]
        assert!(time_elapsed > 100, "time elapsed: {}", time_elapsed);

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
