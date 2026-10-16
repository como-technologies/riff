//! The records that a daemon keeps (01M519Z2JMAKN0T6JZFBPSTKC8): the
//! file [`FILE`] in the folder `state` of the daemon.
//!
//! # Design
//!
//! The file holds each record that the daemon kept, in the order in
//! which it kept them: the length of the record (4 bytes, big-endian),
//! then its bytes as its author signed them. The daemon appends a record
//! and syncs the file before it applies the record to its state. So a
//! record in the state is on the disk.
//!
//! At its start, the daemon reads each record again in that order, and
//! checks and applies each one ([`crate::peer::state`]). A crash in an
//! append can leave a last record that is cut short: [`Book::open`]
//! drops it, and cuts the file back to the last whole record.
//!
//! # Example
//!
//! ```
//! use riff::peer::book::Book;
//!
//! let dir = tempfile::tempdir()?;
//! let (mut book, kept) = Book::open(dir.path())?;
//! assert!(kept.is_empty());
//! book.append(b"one")?;
//! book.append(b"two")?;
//! drop(book);
//! let (_, kept) = Book::open(dir.path())?;
//! assert_eq!(kept, [b"one".to_vec(), b"two".to_vec()]);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The name of the file of the records in the folder `state`.
pub const FILE: &str = "records";

/// The file of the records of a daemon, open to append.
#[derive(Debug)]
pub struct Book {
    path: PathBuf,
    file: File,
}

impl Book {
    /// Opens the file of the records in the folder `dir`, and reads each
    /// whole record in it. It makes the folder and the file when they are
    /// not there. It drops a last record that is cut short.
    pub fn open(dir: &Path) -> Result<(Book, Vec<Vec<u8>>)> {
        Book::open_file(dir, FILE)
    }

    /// [`Book::open`] of the file `name` in the folder `dir`, for example
    /// the proofs of the forks ([`crate::peer::keep::FORKS`]).
    pub fn open_file(dir: &Path, name: &str) -> Result<(Book, Vec<Vec<u8>>)> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot make the folder {}", dir.display()))?;
        let path = dir.join(name);
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        let mut all = Vec::new();
        file.read_to_end(&mut all)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let (kept, whole) = records(&all);
        if whole < all.len() {
            file.set_len(whole as u64)
                .with_context(|| format!("cannot cut {}", path.display()))?;
        }
        Ok((Book { path, file }, kept))
    }

    /// Writes the file again with only `records`, in this order: a new
    /// file, then a rename over the old one. A crash keeps the old file
    /// or the new one, not a part of each.
    pub fn rewrite(&mut self, records: &[Vec<u8>]) -> Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        let fail = |what: &str| format!("cannot {what} {}", self.path.display());
        let mut new = tempfile::NamedTempFile::new_in(dir).with_context(|| fail("write"))?;
        for bytes in records {
            let length = u32::try_from(bytes.len()).context("a record of 4 GiB or more")?;
            new.write_all(&length.to_be_bytes())
                .and_then(|()| new.write_all(bytes))
                .with_context(|| fail("write"))?;
        }
        new.as_file().sync_data().with_context(|| fail("sync"))?;
        new.persist(&self.path)
            .map_err(|e| e.error)
            .with_context(|| fail("replace"))?;
        self.file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .with_context(|| fail("open"))?;
        Ok(())
    }

    /// Reads each whole record of the file again.
    pub fn records(&self) -> Result<Vec<Vec<u8>>> {
        let all = std::fs::read(&self.path)
            .with_context(|| format!("cannot read {}", self.path.display()))?;
        Ok(records(&all).0)
    }

    /// Appends the record `bytes`, and syncs the file.
    pub fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let length = u32::try_from(bytes.len()).context("a record of 4 GiB or more")?;
        let mut frame = Vec::with_capacity(4 + bytes.len());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(bytes);
        self.file
            .write_all(&frame)
            .and_then(|()| self.file.sync_data())
            .with_context(|| format!("cannot write a record to {}", self.path.display()))
    }
}

/// Each whole record in `all`, and the length of the part that holds
/// them.
fn records(all: &[u8]) -> (Vec<Vec<u8>>, usize) {
    let mut kept = Vec::new();
    let mut at = 0;
    while let Some(head) = all.get(at..at + 4) {
        let length = u32::from_be_bytes(head.try_into().expect("4 bytes")) as usize;
        let Some(bytes) = all.get(at + 4..at + 4 + length) else {
            break;
        };
        kept.push(bytes.to_vec());
        at += 4 + length;
    }
    (kept, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crash in an append leaves a cut record: the next open drops it,
    /// and the next append comes after the last whole record.
    #[test]
    fn a_cut_last_record_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let (mut book, _) = Book::open(dir.path()).unwrap();
        book.append(b"whole").unwrap();
        drop(book);
        let path = dir.path().join(FILE);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&[0, 0, 0, 9, b'c', b'u']).unwrap();
        drop(file);

        let (mut book, kept) = Book::open(dir.path()).unwrap();
        assert_eq!(kept, [b"whole".to_vec()]);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 9);
        book.append(b"next").unwrap();
        drop(book);
        let (_, kept) = Book::open(dir.path()).unwrap();
        assert_eq!(kept, [b"whole".to_vec(), b"next".to_vec()]);
    }

    #[test]
    fn a_rewrite_keeps_only_the_records_that_it_gets() {
        let dir = tempfile::tempdir().unwrap();
        let (mut book, _) = Book::open(dir.path()).unwrap();
        for bytes in [&b"one"[..], b"two", b"three"] {
            book.append(bytes).unwrap();
        }
        book.rewrite(&[b"one".to_vec(), b"three".to_vec()]).unwrap();
        book.append(b"four").unwrap();
        assert_eq!(
            book.records().unwrap(),
            [b"one".to_vec(), b"three".to_vec(), b"four".to_vec()]
        );
        drop(book);
        let (_, kept) = Book::open(dir.path()).unwrap();
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn a_cut_length_is_dropped_too() {
        let (kept, whole) = records(&[0, 0, 0, 1, 7, 0, 0]);
        assert_eq!((kept, whole), (vec![vec![7]], 5));
        assert_eq!(records(&[]), (vec![], 0));
    }
}
