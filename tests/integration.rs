// tests/integration.rs
mod test_helpers;

mod interactive_session {
    mod slow_reader_thread;
    mod test_bash;
    mod try_reading_pipe_after_child_exit;
}

mod oneshot_command {
    mod test_echo;
    mod test_kill;
}
