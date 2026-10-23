//! The end of a command at a closed pipe (01M5MRG35BYRGZYD4VQVV8ZTYX).
//!
//! Rust ignores SIGPIPE. A write to a pipe that its reader closed fails
//! with EPIPE, and `println!` panics on that error. So `riff daemon status
//! | head -1` showed a panic after `head` read its line.
//!
//! The usual fix sets SIGPIPE back to its default. riff does not do
//! that: the signal would also come from a write to a socket or to the
//! pipe of a child process. `riff run` reconnects when the daemon closes
//! its socket, and the daemon and the relay serve sockets. A SIGPIPE
//! would kill them.
//!
//! [`end_quietly`] adds a panic hook in place of the signal. The panic of
//! `println!` or `eprintln!` (also of `anstream`) on a closed pipe ends
//! the process with the exit 0 and prints nothing. Each other panic goes
//! to the hook that was there before.
//!
//! ```mermaid
//! flowchart LR
//!     P["println! at a closed pipe"] --> E[EPIPE] --> H{panic hook}
//!     H -- "failed printing to stdout or stderr" --> X[exit 0]
//!     H -- "another panic" --> D[the hook before]
//!     S["write to a socket"] --> Err[an error, no signal]
//! ```
//!
//! ```
//! use riff_core::pipe::closed_pipe;
//!
//! assert!(closed_pipe("failed printing to stdout: Broken pipe (os error 32)"));
//! assert!(!closed_pipe("failed printing to stdout: No space left on device (os error 28)"));
//! ```

/// EPIPE: the same number on Linux, macOS and the BSDs.
const EPIPE: i32 = 32;

/// True when `message` is the panic of a print to stdout or stderr that
/// failed because the reader closed the pipe. `println!`, `eprintln!`
/// and the macros of `anstream` panic with this message.
///
/// ```
/// use riff_core::pipe::closed_pipe;
///
/// assert!(closed_pipe("failed printing to stderr: Broken pipe (os error 32)"));
/// assert!(!closed_pipe("Broken pipe (os error 32)"));
/// ```
pub fn closed_pipe(message: &str) -> bool {
    let error = std::io::Error::from_raw_os_error(EPIPE).to_string();
    ["failed printing to stdout: ", "failed printing to stderr: "]
        .iter()
        .any(|start| message.strip_prefix(start) == Some(error.as_str()))
}

/// Adds the panic hook: a print to a closed pipe ends the process with
/// the exit 0, and prints nothing. Call it at the start of `main` of a
/// command that ends by itself, never of a server: the hook ends the whole
/// process from each thread.
///
/// ```no_run
/// riff_core::pipe::end_quietly();
/// for n in 0..1000 {
///     println!("{n}"); // `| head -1` ends the process quietly here.
/// }
/// ```
pub fn end_quietly() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload_as_str().is_some_and(closed_pipe) {
            std::process::exit(0);
        }
        before(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::closed_pipe;

    /// The message of `println!` and of `anstream::println!` on EPIPE.
    #[test]
    fn a_print_to_a_closed_pipe_is_a_closed_pipe() {
        let error = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        let os = std::io::Error::from_raw_os_error(32);
        assert_eq!(os.kind(), error.kind());
        for label in ["stdout", "stderr"] {
            assert!(closed_pipe(&format!("failed printing to {label}: {os}")));
        }
    }

    /// Another error of a print, or another panic, is not a closed pipe.
    #[test]
    fn another_panic_is_not_a_closed_pipe() {
        let full = std::io::Error::from_raw_os_error(28);
        assert!(!closed_pipe(&format!("failed printing to stdout: {full}")));
        assert!(!closed_pipe(
            "failed printing to a file: Broken pipe (os error 32)"
        ));
        assert!(!closed_pipe("called `Option::unwrap()` on a `None` value"));
        assert!(!closed_pipe(""));
    }
}
