#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::test_helpers::setup_shell_session;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::time::Instant;

    const ITERATIONS: usize = 100;

    fn run_iteration() -> Result<(), String> {
        let shell_session = setup_shell_session().map_err(|e| e.to_string())?;
        let mut child = shell_session.child;

        let mut killer = child.clone_killer();

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let (tx_exit_status, rx_exit_status) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            barrier_clone.wait();

            let wait_result = child.wait();

            let _ = tx_exit_status.send(wait_result);
        });

        barrier.wait();

        let _kill_result = killer.kill();

        let wait_result = rx_exit_status.recv().map_err(|e| e.to_string())?;

        if wait_result.is_err() {
            return Err(format!("wait error: {:?}", wait_result.err()));
        }

        let status = wait_result.unwrap();

        if status.success() {
            return Err(format!(
                "Process should have been killed, but exited with: {}",
                status
            ));
        }

        Ok(())
    }

    #[test]
    #[timeout(300000)]
    fn test_wait_before_kill_stress() {
        let start = Instant::now();
        let mut completed = 0;
        let mut timeouts = 0;

        for i in 0..ITERATIONS {
            let iter_start = Instant::now();
            let result = run_iteration();
            let elapsed = iter_start.elapsed().as_millis();

            match result {
                Ok(()) => {
                    completed += 1;
                    if elapsed > 500 {
                        eprintln!("Iteration {} took {}ms", i, elapsed);
                    }
                }
                Err(e) => {
                    timeouts += 1;
                    eprintln!("Iteration {} failed: {}", i, e);
                    if timeouts > 10 {
                        panic!("Too many failures, stopping");
                    }
                }
            }
        }

        let total_elapsed = start.elapsed();
        eprintln!(
            "Completed {} iterations ({} timeouts) in {}ms (avg {:.2}ms/iter)",
            completed,
            timeouts,
            total_elapsed.as_millis(),
            total_elapsed.as_millis() as f64 / ITERATIONS as f64
        );

        assert_eq!(completed, ITERATIONS, "Some iterations failed");
    }
}
