#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::test_helpers::setup_shell_session;

    #[test]
    #[timeout(5000)]
    fn test_kill() {
        let shell_session = setup_shell_session().unwrap();
        let mut child = shell_session.child;

        child.kill().unwrap();

        let status = child.wait().unwrap();

        assert!(
            !status.success(),
            "Process should have been killed, but exited with: {}",
            status
        );
    }

    #[test]
    #[timeout(5000)]
    fn test_wait_before_kill() {
        let shell_session = setup_shell_session().unwrap();
        let mut child = shell_session.child;

        let mut killer = child.clone_killer();

        let (tx_waiting, rx_waiting) = std::sync::mpsc::channel();
        let (tx_exit_status, rx_exit_status) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            // Notify the main thread that we're about to wait
            tx_waiting.send(()).unwrap();

            // Wait for the child process to exit
            let wait_result = child.wait();

            // Send the result back to the main thread
            tx_exit_status.send(wait_result).unwrap();
        });

        // Wait for the child process to start waiting
        rx_waiting.recv().unwrap();

        // Kill the child process
        killer.kill().unwrap();

        // Wait for the exit status
        let wait_result = rx_exit_status.recv().unwrap();

        assert!(
            wait_result.is_ok(),
            "Waiting on the child process should succeed, but got error: {:?}",
            wait_result.err()
        );

        let status = wait_result.unwrap();

        assert!(
            !status.success(),
            "Process should have been killed, but exited with: {}",
            status
        );
    }
}
