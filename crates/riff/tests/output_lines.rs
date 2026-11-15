//! The daemon reads the output of a container in lines of at most 64 KiB
//! (01M7G7HJH5MHGRNYP2J9QG8SCQ). A command writes 100 MiB with no line
//! break into a pipe, as a container writes into the pipe of podman.

use std::process::Stdio;

use riff::output_lines::{BUF, LINE_MAX, Lines};
use riff::run::Scrub;
use riff::text::LINE_CUT;

/// The pipe of a command that writes 100 MiB of `a` with no line break,
/// then the line `next`: the read holds at most its buffer and the limit,
/// and the kept line ends with the mark.
#[tokio::test]
async fn an_output_of_100_mib_with_no_line_break_keeps_the_memory_of_the_read_under_the_limit() {
    let mut child = tokio::process::Command::new("sh")
        .args([
            "-c",
            "head -c 104857600 /dev/zero | tr '\\000' a; printf '\\nnext\\n'",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = Lines::new(child.stdout.take().unwrap());
    let mut shown = Vec::new();
    while let Some(line) = lines.next_line().await.unwrap() {
        assert!(lines.held() <= BUF + LINE_MAX, "{}", lines.held());
        shown.push(line.clean(&Scrub::default()));
    }
    assert!(child.wait().await.unwrap().success());
    assert!(lines.held() <= BUF + LINE_MAX, "{}", lines.held());
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0], "a".repeat(LINE_MAX) + LINE_CUT);
    assert_eq!(shown[1], "next");
}
