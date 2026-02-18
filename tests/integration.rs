// tests/integration.rs
mod interactive_session {
    #[path = "interactive_session/slow_reader_thread.rs"]
    mod slow_reader_thread;
    #[path = "interactive_session/test_bash.rs"]
    mod test_bash;
    #[path = "interactive_session/try_reading_pipe_after_child_exit.rs"]
    mod try_reading_pipe_after_child_exit;
}

mod oneshot_command {
    #[path = "oneshot_command/test_echo.rs"]
    mod test_echo;
}