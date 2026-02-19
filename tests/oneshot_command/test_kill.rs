#[cfg(test)]
mod tests {
    use ntest::timeout;
    use portable_pty::test_helpers::setup_shell_session;

    #[test]
    #[timeout(10000)]
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
}
