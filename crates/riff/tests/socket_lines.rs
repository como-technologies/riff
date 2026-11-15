//! The daemon reads the socket of a session in lines of at most 1 MiB
//! (01M7GEMCQQQT6TN8ABFGWRQBWR). A process writes 100 MiB with no line
//! break on a Unix socket, as a process in a container writes on the
//! socket of its session.

use riff::socket_lines::{LINE_MAX, Limited};
use riff::text::socket_line_too_long;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The read fails with the error that names the session, holds at most
/// the limit of the line, and takes no more than it needs: the writer
/// cannot write the whole line.
#[tokio::test]
async fn a_socket_line_of_100_mib_with_no_line_break_fails_with_the_name_of_the_session() {
    let name = "riff-issue-12-1a2b3c4d";
    let (daemon, mut session) = UnixStream::pair().unwrap();
    let sent = tokio::spawn(async move {
        let part = vec![b'a'; 1024 * 1024];
        for _ in 0..100 {
            if session.write_all(&part).await.is_err() {
                return false;
            }
        }
        true
    });
    let mut read = BufReader::new(Limited::new(daemon, name));
    let mut line = String::new();
    let error = read.read_line(&mut line).await.unwrap_err();
    assert_eq!(error.to_string(), socket_line_too_long(name, LINE_MAX));
    assert_eq!(line.len(), LINE_MAX);
    assert!(line.capacity() <= 2 * LINE_MAX, "{}", line.capacity());
    drop(read);
    assert!(!sent.await.unwrap(), "the read took the whole line");
}
