//! The invites of a riff of peers (01M519Z3SGEHVMS85HZD169CAP,
//! 01M519Z3VXY4R3WDSSBQCVYTS4).
//!
//! # Design
//!
//! `riff invite USER` on the daemon of the owner makes one [`Invite`]:
//! the riff ID, the user, a random secret of 32 bytes, and the end time
//! [`LIFE`] after now. [`Invite::text`] gives the invite string:
//! [`PREFIX`] and the base32 (RFC 4648, lower case, no padding) of these
//! bytes (01M521J6CEJB24QFN17077KG9S):
//!
//! | Bytes | Holds |
//! |---|---|
//! | 32 | the riff ID: the key of the owner |
//! | 32 | the secret |
//! | 8 | the end time, in milliseconds since the Unix epoch, big-endian |
//! | the rest | the user, in UTF-8 |
//!
//! The daemon of the owner keeps each invite in the file [`FILE`] in its
//! folder, with the mode 0600: the BLAKE3 hash of the secret, the user,
//! the end time, and whether a join used it ([`Invites`],
//! 01M521J6F5QH9MW4GVY0H2JD8N). The file holds no secret, and no
//! record holds an invite. A join takes the invite
//! ([`Invites::take`]) before the daemon writes the `member` record:
//! a second join with the same secret finds it used. When the write of
//! the record fails, the daemon gives the invite back
//! ([`Invites::give_back`]), so a failed join changes nothing.
//!
//! ```mermaid
//! flowchart LR
//!     I["riff invite brett"] --> M["Invites::make: the hash, the user, the end time"]
//!     M --> S["riff1-...: the riff, the secret, the end time, the user"]
//!     S -->|"by hand"| J["riff join riff1-..."]
//!     J --> T{"Invites::take"}
//!     T -->|"unknown, used or ended"| R["refused: nothing changes"]
//!     T -->|"the user"| W["the member record"]
//! ```
//!
//! # Example
//!
//! ```
//! use riff::daemon_key::Key;
//! use riff::peer::invite::{Invite, Invites, Refused};
//!
//! let dir = tempfile::tempdir()?;
//! let invites = Invites::in_dir(dir.path());
//! let riff = Key::from_bytes(&[1; 32]).public();
//! let invite = invites.make(riff, "brett", 1_000)?;
//! let text = invite.text();
//! assert!(text.starts_with("riff1-"));
//!
//! let read = Invite::parse(&text)?;
//! assert_eq!((read.riff, read.user.as_str()), (riff, "brett"));
//! assert_eq!(invites.take(read.secret(), 2_000)?, Ok("brett".to_owned()));
//! assert_eq!(invites.take(read.secret(), 2_000)?, Err(Refused::Used));
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fmt;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::daemon_key::Public;
use crate::text;

/// The start of each invite string.
pub const PREFIX: &str = "riff1-";

/// An invite works for this time after `riff invite`.
pub const LIFE: Duration = Duration::from_secs(24 * 60 * 60);

/// The file of the invites in the folder of the daemon of the owner.
pub const FILE: &str = "invites.json";

/// The bytes of the riff ID, the secret and the end time of a string.
const FIXED: usize = 32 + 32 + 8;

/// The secret of an invite: 32 random bytes. It has no `Display`, and its
/// `Debug` shows no byte.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<[u8; 32]>);

impl Secret {
    /// A new random secret.
    pub fn random() -> Result<Secret> {
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(bytes.as_mut())
            .map_err(|e| anyhow::anyhow!("cannot make a secret: {e}"))?;
        Ok(Secret(bytes))
    }

    /// The secret of the bytes `bytes`.
    pub fn from_bytes(bytes: [u8; 32]) -> Secret {
        Secret(Zeroizing::new(bytes))
    }

    /// The secret of 64 hex digits `hex`, or `None`.
    pub fn from_hex(hex: &str) -> Option<Secret> {
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = Zeroizing::new([0; 32]);
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).ok()?;
        }
        Some(Secret(bytes))
    }

    /// The 64 hex digits of the secret, for the join request.
    pub fn hex(&self) -> Zeroizing<String> {
        let mut hex = Zeroizing::new(String::with_capacity(64));
        for byte in self.0.iter() {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    /// The bytes of the secret.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The BLAKE3 hash of the secret, in hex: what the daemon of the owner
    /// keeps.
    pub fn hash(&self) -> String {
        blake3::hash(self.0.as_ref()).to_hex().to_string()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// An invite string on its way: on the control socket of the daemon. Its
/// `Debug` shows no part of it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Text(pub String);

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Text(..)")
    }
}

/// One invite: what the invite string holds.
#[derive(Clone, PartialEq, Eq)]
pub struct Invite {
    /// The riff: the key of the owner.
    pub riff: Public,
    /// The user of the new key.
    pub user: String,
    /// The end time, in milliseconds since the Unix epoch.
    pub ends_ms: u64,
    secret: Secret,
}

impl fmt::Debug for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Invite")
            .field("riff", &self.riff)
            .field("user", &self.user)
            .field("ends_ms", &self.ends_ms)
            .finish_non_exhaustive()
    }
}

impl Invite {
    /// The invite of the riff `riff` for the user `user`, with the secret
    /// `secret` and the end time `ends_ms`.
    pub fn new(riff: Public, user: &str, secret: Secret, ends_ms: u64) -> Invite {
        Invite {
            riff,
            user: user.to_owned(),
            ends_ms,
            secret,
        }
    }

    /// The secret of the invite.
    pub fn secret(&self) -> &Secret {
        &self.secret
    }

    /// The invite string (01M521J6CEJB24QFN17077KG9S).
    pub fn text(&self) -> Zeroizing<String> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(FIXED + self.user.len()));
        bytes.extend_from_slice(self.riff.as_bytes());
        bytes.extend_from_slice(self.secret.bytes());
        bytes.extend_from_slice(&self.ends_ms.to_be_bytes());
        bytes.extend_from_slice(self.user.as_bytes());
        let mut text = Zeroizing::new(data_encoding::BASE32_NOPAD.encode(&bytes));
        text.make_ascii_lowercase();
        text.insert_str(0, PREFIX);
        text
    }

    /// Reads the invite string `text`. Spaces and line ends around it do
    /// not count.
    pub fn parse(text: &str) -> Result<Invite> {
        let text = text.trim();
        let Some(base32) = text.strip_prefix(PREFIX) else {
            bail!("{}", text::INVITE_NOT_A_STRING);
        };
        let upper = Zeroizing::new(base32.to_ascii_uppercase());
        let bytes = Zeroizing::new(
            data_encoding::BASE32_NOPAD
                .decode(upper.as_bytes())
                .ok()
                .filter(|bytes| bytes.len() > FIXED)
                .context(text::INVITE_NOT_A_STRING)?,
        );
        let riff = Public::from_bytes(bytes[..32].try_into().expect("32 bytes"));
        let secret = Secret::from_bytes(bytes[32..64].try_into().expect("32 bytes"));
        let ends_ms = u64::from_be_bytes(bytes[64..FIXED].try_into().expect("8 bytes"));
        let user = std::str::from_utf8(&bytes[FIXED..])
            .ok()
            .filter(|user| riff_core::name::check("user", user).is_ok())
            .context(text::INVITE_NOT_A_STRING)?;
        Ok(Invite::new(riff, user, secret, ends_ms))
    }
}

/// Why the daemon of the owner refuses the secret of a join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The daemon has no invite with that secret.
    Unknown,
    /// A join used the invite.
    Used,
    /// The invite ended.
    Ended,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::Unknown => text::INVITE_UNKNOWN,
            Refused::Used => text::INVITE_USED,
            Refused::Ended => text::INVITE_ENDED,
        })
    }
}

/// One invite in [`FILE`]: no secret, only its hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    /// The BLAKE3 hash of the secret, in hex.
    hash: String,
    user: String,
    ends_ms: u64,
    #[serde(default)]
    used: bool,
}

/// The invites of the daemon of the owner, in the file [`FILE`] of its
/// folder. See the design of the module.
#[derive(Debug)]
pub struct Invites {
    path: PathBuf,
    /// One change of the file at a time.
    lock: Mutex<()>,
}

impl Invites {
    /// The invites in [`FILE`] of the folder `dir`.
    pub fn in_dir(dir: &Path) -> Invites {
        Invites {
            path: dir.join(FILE),
            lock: Mutex::new(()),
        }
    }

    /// The path of the file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Makes an invite of the riff `riff` for the user `user` at the time
    /// `now_ms`: it ends [`LIFE`] later. It keeps the hash of its secret.
    pub fn make(&self, riff: Public, user: &str, now_ms: u64) -> Result<Invite> {
        let life = u64::try_from(LIFE.as_millis()).unwrap_or(u64::MAX);
        let invite = Invite::new(riff, user, Secret::random()?, now_ms.saturating_add(life));
        let _one = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut kept = self.read()?;
        kept.push(Kept {
            hash: invite.secret().hash(),
            user: user.to_owned(),
            ends_ms: invite.ends_ms,
            used: false,
        });
        self.write(&kept)?;
        Ok(invite)
    }

    /// Takes the invite of `secret` at the time `now_ms`: it gives the user
    /// of the invite, and marks it used. Else it gives why not, and
    /// changes nothing.
    pub fn take(&self, secret: &Secret, now_ms: u64) -> Result<Result<String, Refused>> {
        let hash = secret.hash();
        let _one = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut kept = self.read()?;
        let Some(invite) = kept.iter_mut().find(|invite| invite.hash == hash) else {
            return Ok(Err(Refused::Unknown));
        };
        if invite.used {
            return Ok(Err(Refused::Used));
        }
        if now_ms >= invite.ends_ms {
            return Ok(Err(Refused::Ended));
        }
        invite.used = true;
        let user = invite.user.clone();
        self.write(&kept)?;
        Ok(Ok(user))
    }

    /// Gives back the invite of `secret` that [`Invites::take`] took: a
    /// join that failed after the take.
    pub fn give_back(&self, secret: &Secret) -> Result<()> {
        let hash = secret.hash();
        let _one = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut kept = self.read()?;
        for invite in kept.iter_mut().filter(|invite| invite.hash == hash) {
            invite.used = false;
        }
        self.write(&kept)
    }

    /// The invites in the file: none when there is no file.
    fn read(&self) -> Result<Vec<Kept>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("cannot read {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", self.path.display())),
        }
    }

    /// Writes `kept` to the file, with the mode 0600: a temp file, then a
    /// rename.
    fn write(&self, kept: &[Kept]) -> Result<()> {
        let temp = self.path.with_extension("json.new");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .with_context(|| format!("cannot write {}", temp.display()))?;
        file.write_all(&serde_json::to_vec_pretty(kept)?)?;
        file.sync_all()?;
        std::fs::rename(&temp, &self.path)
            .with_context(|| format!("cannot write {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::daemon_key::Key;

    fn riff() -> Public {
        Key::from_bytes(&[1; 32]).public()
    }

    /// 01M521J6CEJB24QFN17077KG9S: the string holds the riff, the secret,
    /// the end time and the user, and reads back the same.
    #[test]
    fn an_invite_string_reads_back_the_same() {
        let secret = Secret::from_bytes([9; 32]);
        let invite = Invite::new(riff(), "brett", secret.clone(), 86_400_000);
        let text = invite.text();
        assert!(text.starts_with(PREFIX), "{}", *text);
        let base32 = &text[PREFIX.len()..];
        assert!(
            base32
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)),
            "{base32}"
        );
        let read = Invite::parse(&format!(" {}\n", *text)).unwrap();
        assert_eq!(read, invite);
        assert_eq!(read.secret(), &secret);
        // The Debug of an invite and of its secret shows no byte of it.
        let shown = format!("{read:?} {:?}", read.secret());
        assert!(!shown.contains("9, 9"), "{shown}");
        assert!(!shown.contains(&*secret.hex()), "{shown}");
    }

    #[test]
    fn a_string_that_is_not_an_invite_is_refused() {
        let good = Invite::new(riff(), "brett", Secret::from_bytes([9; 32]), 1).text();
        let cut = &good[..good.len() - 10];
        let no_user = Invite::new(riff(), "x", Secret::from_bytes([9; 32]), 1).text();
        let no_user = &no_user[..no_user.len() - 2];
        for text in ["", "riff1-", "riff2-abc", "riff1-!!!", cut, no_user] {
            let error = Invite::parse(text).unwrap_err().to_string();
            assert_eq!(error, text::INVITE_NOT_A_STRING, "{text}");
        }
        let bad_user = Invite::new(riff(), "a b", Secret::from_bytes([9; 32]), 1).text();
        assert!(Invite::parse(&bad_user).is_err());
    }

    #[test]
    fn a_secret_reads_from_its_hex() {
        let secret = Secret::from_bytes([0xab; 32]);
        assert_eq!(Secret::from_hex(&secret.hex()), Some(secret));
        assert_eq!(Secret::from_hex("ab"), None);
        assert_eq!(Secret::from_hex(&"+a".repeat(32)), None);
    }

    /// 01M519Z3SGEHVMS85HZD169CAP, 01M521J6F5QH9MW4GVY0H2JD8N: an invite
    /// works one time, until its end. A wrong secret changes nothing. The
    /// file holds no secret, and has the mode 0600.
    #[test]
    fn an_invite_works_one_time_until_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let invites = Invites::in_dir(dir.path());
        let now = 1_000;
        let first = invites.make(riff(), "brett", now).unwrap();
        let second = invites.make(riff(), "ann", now).unwrap();
        let life = u64::try_from(LIFE.as_millis()).unwrap();
        assert_eq!(first.ends_ms, now + life);

        let file = std::fs::read(invites.path()).unwrap();
        let mode = std::fs::metadata(invites.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        for invite in [&first, &second] {
            let hex = invite.secret().hex();
            assert!(!file.windows(64).any(|w| w == hex.as_bytes()));
            assert!(
                !file
                    .windows(32)
                    .any(|w| w == invite.secret().bytes().as_slice())
            );
            assert!(!String::from_utf8_lossy(&file).contains(&invite.text()[PREFIX.len()..]));
        }

        let wrong = Secret::from_bytes([7; 32]);
        assert_eq!(invites.take(&wrong, now).unwrap(), Err(Refused::Unknown));
        assert_eq!(std::fs::read(invites.path()).unwrap(), file);

        let ends = now + life;
        assert_eq!(
            invites.take(second.secret(), ends).unwrap(),
            Err(Refused::Ended)
        );
        assert_eq!(
            invites.take(first.secret(), ends - 1).unwrap(),
            Ok("brett".into())
        );
        assert_eq!(
            invites.take(first.secret(), now).unwrap(),
            Err(Refused::Used)
        );
        invites.give_back(first.secret()).unwrap();
        assert_eq!(
            invites.take(first.secret(), now).unwrap(),
            Ok("brett".into())
        );
    }
}
