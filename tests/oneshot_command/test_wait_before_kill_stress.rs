#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::test_helpers::setup_shell_session;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::time::Instant;

    const ITERATIONS: usize = 50;
    const SLOW_THRESHOLD_MS: u128 = 5000;
    const SLOW_THRESHOLD_DURATION: std::time::Duration = std::time::Duration::from_millis(SLOW_THRESHOLD_MS as u64);

    fn run_iteration(tx_debug_info: std::sync::mpsc::Sender<String>) -> Result<(), String> {
        let shell_session = setup_shell_session().map_err(|e| e.to_string())?;
        let mut child = shell_session.child;

        let mut killer = child.clone_killer();

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        let (tx_exit_status, rx_exit_status) = std::sync::mpsc::channel();
        let tx_debug_info_clone = tx_debug_info.clone();

        std::thread::spawn(move || {
            barrier_clone.wait();

            std::thread::sleep(std::time::Duration::from_millis(100));
            tx_debug_info_clone.send(format!("waiting for child process to exit")).unwrap();
            let wait_result = child.wait();

            let _ = tx_exit_status.send(wait_result);
        });

        barrier.wait();

        tx_debug_info.send(format!("killing child process")).unwrap();
        let _kill_result = killer.kill();

        tx_debug_info.send(format!("waiting for exit status")).unwrap();
        let wait_result = rx_exit_status.recv().map_err(|e| e.to_string())?;
        tx_debug_info.send(format!("wait_result = {:?}", wait_result)).unwrap();

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
    #[timeout(15000)]
    fn test_wait_before_kill_stress() {
        use std::sync::mpsc;

        let mut completed = 0;
        let mut timeouts = 0;
        let mut slow = 0;

        let mut handles = Vec::with_capacity(ITERATIONS);

        for _i in 0..ITERATIONS {
            let (tx, rx) = mpsc::channel();
            let (tx_debug_info, rx_debug_info) = mpsc::channel();

            let handle = std::thread::spawn(move || {
                let iter_start = Instant::now();
                let result = run_iteration(tx_debug_info);
                let elapsed = iter_start.elapsed().as_millis();
                let _ = tx.send((result, elapsed));
            });

            handles.push((handle, rx, rx_debug_info));
        }

        let start_global = Instant::now();

        for (i, (_handle, rx, rx_debug_info)) in handles.into_iter().enumerate() {
            let (result, elapsed) = match rx.recv_timeout(SLOW_THRESHOLD_DURATION - start_global.elapsed().min(SLOW_THRESHOLD_DURATION)) {
                Ok(data) => data,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    timeouts += 1;
                    eprintln!("Iteration {} timed out (>{}ms)", i, SLOW_THRESHOLD_MS);
                    for debug_msg in rx_debug_info.try_iter() {
                        eprintln!("DEBUG: Iteration {} - {}", i, debug_msg);
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    timeouts += 1;
                    eprintln!("Iteration {} channel disconnected", i);
                    for debug_msg in rx_debug_info.try_iter() {
                        eprintln!("DEBUG: Iteration {} - {}", i, debug_msg);
                    }
                    continue;
                }
            };

            match result {
                Ok(()) => {
                    completed += 1;
                    if elapsed > SLOW_THRESHOLD_MS {
                        slow += 1;
                        eprintln!("Iteration {} took {}ms (slow >{}ms)", i, elapsed, SLOW_THRESHOLD_MS);
                    }
                }
                Err(e) => {
                    timeouts += 1;
                    eprintln!("Iteration {} failed: {}", i, e);
                    for debug_msg in rx_debug_info.try_iter() {
                        eprintln!("DEBUG: Iteration {} - {}", i, debug_msg);
                    }
                    if timeouts > 10 {
                        panic!("Too many failures, stopping");
                    }
                }
            }
        }

        let total_elapsed = start_global.elapsed();
        eprintln!(
            "Completed {} iterations ({} failures, {} slow >{}ms) in {}ms (avg {:.2}ms/iter)",
            completed,
            timeouts,
            slow,
            SLOW_THRESHOLD_MS,
            total_elapsed.as_millis(),
            total_elapsed.as_millis() as f64 / ITERATIONS as f64
        );

        assert_eq!(completed, ITERATIONS, "Some iterations failed");
    }
}
