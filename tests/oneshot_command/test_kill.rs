#[cfg(test)]
mod tests {
    use ntest::timeout;

    include!("../test_helpers.rs");

    #[test]
    #[timeout(10000)]
    fn test_kill() {
        let shell_session = test_helpers::setup_shell_session().unwrap();
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
