#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
    use std::thread;
    use std::time::Duration;

    const COMMAND: &str = "sleep";

    #[test]
    #[timeout(10000)]
    fn test_kill() {
        let pty_system = NativePtySystem::default();

        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let PtyPair { master, slave } = pair;

        let mut cmd = CommandBuilder::new(COMMAND);
        cmd.arg("60");
        let mut child = slave.spawn_command(cmd).unwrap();

        drop(slave);
        
        thread::sleep(Duration::from_millis(100));
        
        child.kill().unwrap();
        
        let status = child.wait().unwrap();
        drop(master);

        assert!(
            !status.success(),
            "Process should have been killed, but exited with: {}",
            status
        );
    }
}
