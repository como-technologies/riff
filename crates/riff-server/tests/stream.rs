//! Each event stream sends its first bytes when it opens
//! (01M3QA6TDF6FB5PH8E5V7HCYDQ), behind a front end that holds a reply
//! until its first body byte, as Cloud Run does.

mod common;

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A connect must finish in this time. The keep-alive of the server
/// comes after 15 seconds.
const AT_ONCE: Duration = Duration::from_secs(3);

/// Starts a proxy to `upstream`. It holds each reply until the first
/// byte after the headers, then passes all bytes on. Returns its URL.
async fn holding_proxy(upstream: &str) -> String {
    let upstream = upstream.trim_start_matches("http://").to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (client, _) = listener.accept().await.unwrap();
            let server = TcpStream::connect(&upstream).await.unwrap();
            tokio::spawn(hold(client, server));
        }
    });
    url
}

async fn hold(client: TcpStream, server: TcpStream) {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    tokio::spawn(async move { tokio::io::copy(&mut client_read, &mut server_write).await });
    let mut held = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let n = server_read.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        held.extend_from_slice(&chunk[..n]);
        let body = held.windows(4).position(|w| w == b"\r\n\r\n");
        if body.is_some_and(|end| held.len() > end + 4) {
            break;
        }
    }
    if client_write.write_all(&held).await.is_ok() {
        let _ = tokio::io::copy(&mut server_read, &mut client_write).await;
    }
}

/// Opens `path` through the proxy. Returns the first body chunk.
async fn first_bytes(path: &str) -> String {
    let (_service, base) = common::start(false, &[]).await;
    let proxy = holding_proxy(&base).await;
    let connect = async {
        let mut reply = common::client()
            .get(format!("{proxy}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        reply.chunk().await.unwrap().unwrap()
    };
    let chunk = isolated::in_time(AT_ONCE, connect)
        .await
        .expect("the stream sends no byte when it opens");
    String::from_utf8_lossy(&chunk).into_owned()
}

#[tokio::test]
async fn the_tail_connect_finishes_at_once() {
    let uri = "riff%3A%2F%2Fmike%40pangolin";
    let first = first_bytes(&format!("/v1/tail?uri={uri}&thread=chat")).await;
    assert!(first.starts_with(": ready"), "{first:?}");
}

#[tokio::test]
async fn the_watch_connect_finishes_at_once() {
    let uri = "riff%3A%2F%2Fmike%40pangolin%2Fcomo-technologies%2Friff%3Fsession%3Da";
    let first = first_bytes(&format!("/v1/watch?uri={uri}")).await;
    assert!(first.starts_with(": ready"), "{first:?}");
}
