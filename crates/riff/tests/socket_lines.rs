//! The daemon reads the socket of a session in lines of at most 1 MiB
//! (01M7GEMCQQQT6TN8ABFGWRQBWR). A process writes 100 MiB with no line
//! break on a Unix socket, as a process in a container writes on the
//! socket of its session.

use riff::socket_lines::{HELD, LINE_MAX, Lines};
use riff::text::socket_line_too_long;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

/// After a short line, the read fails with the error that names the
/// session, holds at most `HELD` bytes, and takes no more than it needs:
/// the writer cannot write the whole line.
#[tokio::test]
async fn a_socket_line_of_100_mib_with_no_line_break_fails_with_the_name_of_the_session() {
    let name = "riff-issue-12-1a2b3c4d";
    let (daemon, mut session) = UnixStream::pair().unwrap();
    let sent = tokio::spawn(async move {
        let part = vec![b'a'; 1024 * 1024];
        if session.write_all(b"{\"ask\":\"hello\"}\n").await.is_err() {
            return false;
        }
        for _ in 0..100 {
            if session.write_all(&part).await.is_err() {
                return false;
            }
        }
        true
    });
    let mut lines = Lines::new(daemon, name);
    let hello = lines.next_line().await.unwrap();
    assert_eq!(hello.as_deref(), Some("{\"ask\":\"hello\"}"));
    let error = lines.next_line().await.unwrap_err();
    assert_eq!(error.to_string(), socket_line_too_long(name, LINE_MAX));
    assert!(lines.held() <= HELD, "{}", lines.held());
    assert_eq!(lines.too_long().map(<[u8]>::len), Some(LINE_MAX));
    drop(lines);
    assert!(!sent.await.unwrap(), "the read took the whole line");
}
