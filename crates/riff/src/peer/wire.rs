//! The frames of a peer link: what two daemons send on their one stream
//! (01M519Z2YSXADT45A1R0N70WRQ).
//!
//! # Design
//!
//! Each frame is its kind (1 byte), the length of its payload (4 bytes,
//! big-endian), and the payload.
//!
//! | Kind | Byte | Payload |
//! |---|---|---|
//! | [`Frame::Heads`] | 1 | for each author: its key (32 bytes), its last sequence number (8 bytes, big-endian), and the ID of that record (32 bytes) |
//! | [`Frame::Record`] | 2 | the bytes of one record, as its author signed them |
//!
//! [`read`] reads no payload that is larger than its kind takes: a record
//! of [`LARGEST`] bytes, or the heads of [`MOST_AUTHORS`] authors. Each
//! other fault of a frame is a [`Bad`] frame. The daemon then closes the
//! link ([`crate::peer::sync`]).
//!
//! # Example
//!
//! ```
//! use std::collections::BTreeMap;
//!
//! use riff::daemon_key::Key;
//! use riff::peer::record::Id;
//! use riff::peer::wire::{self, Frame};
//!
//! let heads = BTreeMap::from([(Key::from_bytes(&[1; 32]).public(), (3, Id([7; 32])))]);
//! let mut bytes = wire::bytes(&Frame::Heads(heads.clone()));
//! bytes.extend(wire::bytes(&Frame::Record(vec![1, 2, 3])));
//! let mut stream = bytes.as_slice();
//! tokio::runtime::Runtime::new()?.block_on(async {
//!     assert_eq!(wire::read(&mut stream).await?, Some(Frame::Heads(heads)));
//!     assert_eq!(wire::read(&mut stream).await?, Some(Frame::Record(vec![1, 2, 3])));
//!     assert_eq!(wire::read(&mut stream).await?, None);
//!     Ok::<(), wire::Bad>(())
//! })?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::daemon_key::Public;
use crate::peer::body::LARGEST;
use crate::peer::keep::Heads;
use crate::peer::record::Id;

/// The byte of [`Frame::Heads`].
pub const HEADS: u8 = 1;

/// The byte of [`Frame::Record`].
pub const RECORD: u8 = 2;

/// The most authors in one [`Frame::Heads`].
pub const MOST_AUTHORS: usize = 65_536;

/// The bytes of one author in [`Frame::Heads`].
const HEAD: usize = 32 + 8 + 32;

/// One frame of a peer link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// The heads of the daemon that sends it.
    Heads(Heads),
    /// The bytes of one record.
    Record(Vec<u8>),
}

/// A frame that the daemon cannot read.
#[derive(Debug)]
pub enum Bad {
    /// The stream ended in a frame, or a read failed.
    Read(std::io::Error),
    /// A kind that is not [`HEADS`] or [`RECORD`].
    Kind(u8),
    /// A payload that is larger than its kind takes, or heads whose
    /// length is not a whole number of authors.
    Length { kind: u8, length: usize },
}

impl fmt::Display for Bad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bad::Read(e) => write!(f, "the read of a frame: {e}"),
            Bad::Kind(kind) => write!(f, "the kind of a frame: {kind}"),
            Bad::Length { kind, length } => {
                write!(
                    f,
                    "the length of a frame of the kind {kind}: {length} bytes"
                )
            }
        }
    }
}

impl std::error::Error for Bad {}

/// The bytes of `frame`.
pub fn bytes(frame: &Frame) -> Vec<u8> {
    let (kind, payload) = match frame {
        Frame::Heads(heads) => {
            let mut payload = Vec::with_capacity(heads.len() * HEAD);
            for (author, (seq, id)) in heads {
                payload.extend_from_slice(author.as_bytes());
                payload.extend_from_slice(&seq.to_be_bytes());
                payload.extend_from_slice(&id.0);
            }
            (HEADS, payload)
        }
        Frame::Record(bytes) => (RECORD, bytes.clone()),
    };
    let length = u32::try_from(payload.len()).expect("a frame of less than 4 GiB");
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(kind);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Writes `frame` to `stream`.
pub async fn write(stream: &mut (impl AsyncWrite + Unpin), frame: &Frame) -> std::io::Result<()> {
    stream.write_all(&bytes(frame)).await
}

/// Reads the next frame of `stream`. `None` when the stream ends before
/// a frame.
pub async fn read(stream: &mut (impl AsyncRead + Unpin)) -> Result<Option<Frame>, Bad> {
    let mut head = [0; 5];
    match stream.read(&mut head[..1]).await.map_err(Bad::Read)? {
        0 => return Ok(None),
        _ => stream.read_exact(&mut head[1..]).await.map_err(Bad::Read)?,
    };
    let kind = head[0];
    let length = u32::from_be_bytes(head[1..].try_into().expect("4 bytes")) as usize;
    let most = match kind {
        HEADS => MOST_AUTHORS * HEAD,
        RECORD => LARGEST,
        other => return Err(Bad::Kind(other)),
    };
    if length > most || (kind == HEADS && length % HEAD != 0) {
        return Err(Bad::Length { kind, length });
    }
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload).await.map_err(Bad::Read)?;
    if kind == RECORD {
        return Ok(Some(Frame::Record(payload)));
    }
    let heads = payload
        .chunks_exact(HEAD)
        .map(|head| {
            let author = Public::from_bytes(head[..32].try_into().expect("32 bytes"));
            let seq = u64::from_be_bytes(head[32..40].try_into().expect("8 bytes"));
            let id = Id(head[40..].try_into().expect("32 bytes"));
            (author, (seq, id))
        })
        .collect();
    Ok(Some(Frame::Heads(heads)))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_all(bytes: &[u8]) -> Result<Option<Frame>, Bad> {
        let mut stream = bytes;
        read(&mut stream).await
    }

    #[tokio::test]
    async fn a_frame_larger_than_its_kind_is_bad_and_not_read() {
        let mut big = vec![RECORD];
        big.extend_from_slice(&u32::try_from(LARGEST + 1).unwrap().to_be_bytes());
        assert!(matches!(
            read_all(&big).await,
            Err(Bad::Length { kind: RECORD, length }) if length == LARGEST + 1
        ));
        let mut odd = vec![HEADS, 0, 0, 0, 5];
        odd.extend_from_slice(&[0; 5]);
        assert!(matches!(
            read_all(&odd).await,
            Err(Bad::Length {
                kind: HEADS,
                length: 5
            })
        ));
        assert!(matches!(
            read_all(&[9, 0, 0, 0, 0]).await,
            Err(Bad::Kind(9))
        ));
    }

    #[tokio::test]
    async fn a_stream_that_ends_in_a_frame_is_bad() {
        let frame = bytes(&Frame::Record(vec![1, 2, 3]));
        assert!(matches!(read_all(&frame[..6]).await, Err(Bad::Read(_))));
        assert!(matches!(read_all(&frame[..3]).await, Err(Bad::Read(_))));
        assert!(matches!(read_all(&[]).await, Ok(None)));
    }

    #[tokio::test]
    async fn a_record_of_the_largest_size_and_empty_heads_pass() {
        let largest = Frame::Record(vec![7; LARGEST]);
        assert_eq!(read_all(&bytes(&largest)).await.unwrap(), Some(largest));
        let empty = Frame::Heads(Heads::new());
        assert_eq!(read_all(&bytes(&empty)).await.unwrap(), Some(empty));
    }
}
