//! The static binaries of a release, and how `riff update` installs
//! them.
//!
//! # Design
//!
//! riff ships as one static binary (01M4Z60B7H67YSP22NMGT3DM7X). The
//! binary has musl in it, so it needs no shared library: its ELF has no
//! interpreter. So it runs on each Linux of its CPU, also in a session
//! image with an older libc, where the daemon runs it as `riff relay`
//! (01M4Z60B7KEV0H161RBZ1SDRY2).
//!
//! `just static` builds the static `riff` and `riff-server` of a tree in
//! the image `rust:CHANNEL-alpine` (01M4ZPJ04D9MJHNPAV0WN1KCES). The CI
//! job "Build" builds them for each pull request and each push, one time
//! for each tree (01M6QQ1NXY0CXMZEEEP413W4PK). The job "Static riff"
//! checks that each ELF has no interpreter and that `riff --version`
//! runs in the session image that `riff.toml` pins
//! (01M4ZPJ06QYVA23N9SQ45T8WQC). For a release tag, the release binaries
//! of the same tree go to the GitHub release of the tag, with [`SUMS`]
//! (01M4ZPJ091W3NYV1V0FJS9K6MS, 01M6QQ1P8H32E2TQ0CYA87R02V). The names
//! of the files are [`asset`].
//!
//! `riff update` installs these files on a machine with a [`TARGET`]
//! ([`install`], 01M4ZPJ0BAQZ6AN06FGXS5D4FP). It checks the SHA-256 of
//! each file against [`SUMS`], and that each ELF is for this CPU and has
//! no interpreter ([`check`]). Only when each check passes does it
//! rename the new files over the old ones in the bin folder of cargo
//! ([`bin_dir`]). A process that runs keeps its old file. When the
//! release has no [`SUMS`], or no line for this machine, `riff update`
//! builds the release with cargo, as before (01M4ZPJ0DJ9FTYNPTBBVJCTB37):
//! an older release, or a machine of another OS or CPU.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant U as riff update
//!     participant G as the GitHub release vX.Y.Z
//!     participant B as the bin folder of cargo
//!     participant C as cargo
//!     U->>G: SHA256SUMS
//!     alt no file, or no line for this machine
//!         U->>C: install --locked --git REPOSITORY --tag vX.Y.Z riff riff-server
//!     else
//!         U->>G: riff-TARGET, riff-server-TARGET
//!         U->>U: check the SHA-256, the CPU and the interpreter
//!         U->>B: a new file for each, then a rename over the old one
//!     end
//! ```
//!
//! With no tag from the person or the riff, `riff update` installs the
//! release that GitHub marks "Latest" ([`latest`],
//! 01M510MPR1S0EQNRK2195XTXG5): the release that the shared riff runs
//! (01M4YTW8788K4XYTEMRSXDH7B9). It is not always the highest tag: GitHub
//! never marks a prerelease "Latest".
//! riff asks the API of GitHub with no sign-in, and reads the tag of the
//! reply ([`tag_of`]).
//!
//! The variable [`RELEASES`] names another place of the release files:
//! a URL, or a folder, for example of a mirror. There the file
//! [`LATEST`] holds the reply for the Latest release. The test
//! environment gives each test an empty folder, so no test reaches
//! GitHub ([`Source`]).
//!
//! A riff of `cargo install` or `just install` links the libc of the
//! machine. So `riff init` fails its check of the riff binary, and the
//! daemon starts no session with it ([`check_file`],
//! 01M510NW68CTZBHHW7ZYFV7Q3X, 01M510NW8YEVMQKD6RWJPW38N3). The fix is
//! `riff update`. The update by itself also puts the static files of
//! the release in place of a build of that release that is not static
//! (01M510NWBNC20GXFRMHPW12TW9). In a test, a fake `podman` runs
//! `riff relay` on the host, so the test environment sets [`DYNAMIC`]
//! (01M510NWEFSN0NWQH8KQ6FA7DC).
//!
//! ```
//! use riff::release::{asset, check, sum_of};
//!
//! assert_eq!(asset("riff", "x86_64-unknown-linux-musl"), "riff-x86_64-unknown-linux-musl");
//! let sums = format!("{}  riff-x86_64-unknown-linux-musl\n", "ab".repeat(32));
//! assert_eq!(sum_of(&sums, "riff-x86_64-unknown-linux-musl"), Some("ab".repeat(32)));
//! assert!(check(b"#!/bin/sh\n", 62).is_err());
//! ```
//!
//! # A Mac
//!
//! A release also has the files of a Mac with Apple silicon, [`MAC`]
//! (01M6Z8F7B3HVVNGQMX8GP1Z7Q1): `riff` and `riff-server` for macOS, and
//! the static Linux riff of [`MAC_SESSION`]. The daemon on a Mac binds
//! that Linux riff into each session, in place of its own binary
//! ([`session_riff`]). Their sums are in a file of their own,
//! [`sums`] of [`MAC`], and other jobs of the CI make them. So the files
//! of Linux, and the jobs that make them, stay as they were, also when
//! the build for the Mac fails. `riff update` on a Mac checks that
//! `riff` and `riff-server` are Mach-O executables for Apple silicon
//! ([`check_mac`]), and that the Linux riff is static ([`files`]).
//! riff has no Developer ID of Apple and no notarization
//! (01M6Z8FM8B8D9NZT6ZYPPWD6TC): the linker signs each binary ad hoc,
//! and a file of `riff update` or of `curl` has no quarantine, so macOS
//! runs it.
//!
//! ```mermaid
//! flowchart LR
//!     L["CI jobs of Linux"] --> S["SHA256SUMS<br/>riff-x86_64-unknown-linux-musl<br/>riff-server-x86_64-unknown-linux-musl"]
//!     M["CI jobs of the Mac"] --> T["SHA256SUMS-aarch64-apple-darwin<br/>riff-aarch64-apple-darwin<br/>riff-server-aarch64-apple-darwin<br/>riff-aarch64-unknown-linux-musl"]
//!     S --> U["riff update on Linux"]
//!     T --> V["riff update on a Mac"]
//! ```

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// The file of the SHA-256 sums of the static binaries of a release, in
/// the form of `sha256sum`.
pub const SUMS: &str = "SHA256SUMS";

/// The binaries that a release ships, in the order of their install:
/// `riff` last, so that a failed rename of `riff-server` keeps the old
/// `riff`.
pub const BINARIES: [&str; 2] = ["riff-server", "riff"];

/// The variable with another place of the release files: a URL, or a
/// folder. A file is at `RELEASES/TAG/NAME`.
pub const RELEASES: &str = "RIFF_RELEASES";

/// The file of [`RELEASES`] with the reply for the release that GitHub
/// marks "Latest", in the form of the API of GitHub: `RELEASES/latest`.
pub const LATEST: &str = "latest";

/// The target of the release files of Linux on x86-64.
pub const LINUX: &str = "x86_64-unknown-linux-musl";

/// The target of the release files of a Mac with Apple silicon
/// (01M6Z8F7B3HVVNGQMX8GP1Z7Q1).
pub const MAC: &str = "aarch64-apple-darwin";

/// The target of the Linux riff that the daemon on a Mac binds into each
/// session: the Linux of a VM on Apple silicon
/// (01M6Z8FM5F0Y7BQYNF8XS1B0T2).
pub const MAC_SESSION: &str = "aarch64-unknown-linux-musl";

/// The target of the release files for this machine. `None` when the
/// releases have none for it.
pub const TARGET: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some(LINUX)
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some(MAC)
} else {
    None
};

/// The file of the SHA-256 sums of the release files of `target`: [`SUMS`]
/// for [`LINUX`], else `SHA256SUMS-TARGET`. Each CI job that names the
/// files of a target writes its own.
///
/// ```
/// use riff::release::{LINUX, MAC, sums};
///
/// assert_eq!(sums(LINUX), "SHA256SUMS");
/// assert_eq!(sums(MAC), "SHA256SUMS-aarch64-apple-darwin");
/// ```
pub fn sums(target: &str) -> String {
    if target == LINUX {
        SUMS.to_owned()
    } else {
        format!("{SUMS}-{target}")
    }
}

/// What a release file must be, for [`check_kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A static ELF of this ELF machine ([`check`]).
    Static(u16),
    /// A Mach-O executable for Apple silicon ([`check_mac`]).
    Mac,
}

/// A release file that `riff update` installs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Its name in the release ([`asset`]).
    pub asset: String,
    /// Its name in the bin folder of cargo.
    pub name: String,
    /// What it must be.
    pub kind: Kind,
}

/// The release files that `riff update` installs on a machine of
/// `target`, in the order of their install: `riff` last, so that a
/// failed rename keeps the old `riff`. On a Mac, the Linux riff of the
/// sessions keeps its name of the release ([`session_riff`]).
///
/// ```
/// use riff::release::{files, Kind, LINUX, MAC};
///
/// let names = |target| files(target).into_iter().map(|f| (f.asset, f.name)).collect::<Vec<_>>();
/// assert_eq!(names(LINUX), [
///     ("riff-server-x86_64-unknown-linux-musl".to_owned(), "riff-server".to_owned()),
///     ("riff-x86_64-unknown-linux-musl".to_owned(), "riff".to_owned()),
/// ]);
/// assert!(files(LINUX).iter().all(|f| f.kind == Kind::Static(62)));
/// assert_eq!(names(MAC), [
///     ("riff-server-aarch64-apple-darwin".to_owned(), "riff-server".to_owned()),
///     ("riff-aarch64-unknown-linux-musl".to_owned(), "riff-aarch64-unknown-linux-musl".to_owned()),
///     ("riff-aarch64-apple-darwin".to_owned(), "riff".to_owned()),
/// ]);
/// let kinds: Vec<Kind> = files(MAC).into_iter().map(|f| f.kind).collect();
/// assert_eq!(kinds, [Kind::Mac, Kind::Static(183), Kind::Mac]);
/// ```
pub fn files(target: &str) -> Vec<File> {
    let kind = if target == MAC {
        Kind::Mac
    } else {
        Kind::Static(elf_machine(target))
    };
    let mut files: Vec<File> = BINARIES
        .iter()
        .map(|binary| File {
            asset: asset(binary, target),
            name: (*binary).to_owned(),
            kind,
        })
        .collect();
    if target == MAC {
        let linux = asset("riff", MAC_SESSION);
        let at = files.len() - 1;
        files.insert(
            at,
            File {
                asset: linux.clone(),
                name: linux,
                kind: Kind::Static(elf_machine(MAC_SESSION)),
            },
        );
    }
    files
}

/// The ELF machine of the code of `target`.
fn elf_machine(target: &str) -> u16 {
    if target.starts_with("aarch64") {
        EM_AARCH64
    } else {
        EM_X86_64
    }
}

/// The riff that the daemon of the riff `me` binds into each session
/// (01M6Z8FM5F0Y7BQYNF8XS1B0T2): `me` on Linux. On a Mac, `me` is a
/// Mach-O file, so it is the Linux riff of [`MAC_SESSION`] beside `me`.
///
/// ```
/// use std::path::Path;
/// use riff::release::session_riff;
///
/// let me = Path::new("/h/.cargo/bin/riff");
/// if cfg!(target_os = "macos") {
///     assert_eq!(session_riff(me), Path::new("/h/.cargo/bin/riff-aarch64-unknown-linux-musl"));
/// } else {
///     assert_eq!(session_riff(me), me);
/// }
/// ```
pub fn session_riff(me: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        me.with_file_name(asset("riff", MAC_SESSION))
    } else {
        me.to_owned()
    }
}

/// The variable that lets `riff init` and the daemon take a riff that
/// is not static, with the value `1` (01M510NWEFSN0NWQH8KQ6FA7DC). Only
/// for tests: there a fake `podman` runs `riff relay` on the host,
/// which has the libc of the riff under test.
pub const DYNAMIC: &str = "RIFF_DYNAMIC";

/// True when [`DYNAMIC`] is `1`.
pub fn takes_dynamic() -> bool {
    std::env::var_os(DYNAMIC).is_some_and(|value| value == "1")
}

/// The ELF machine of x86-64.
pub const EM_X86_64: u16 = 62;

/// The ELF machine of arm64.
pub const EM_AARCH64: u16 = 183;

/// The ELF machine of this CPU, for [`check`].
pub const MACHINE: u16 = if cfg!(target_arch = "aarch64") {
    EM_AARCH64
} else {
    EM_X86_64
};

/// The Mach-O CPU type of Apple silicon, `CPU_TYPE_ARM64`.
pub const CPU_ARM64: u32 = 0x0100_000c;

/// The longest wait for the first byte of a server, and between two
/// reads of a file.
const WAIT: Duration = Duration::from_secs(30);

/// The bytes at the start of a binary that [`check_file`] reads first.
const HEAD: u64 = 64 * 1024;

/// The name of the release file of `binary` for `target`.
///
/// ```
/// assert_eq!(
///     riff::release::asset("riff-server", "x86_64-unknown-linux-musl"),
///     "riff-server-x86_64-unknown-linux-musl"
/// );
/// ```
pub fn asset(binary: &str, target: &str) -> String {
    format!("{binary}-{target}")
}

/// The SHA-256 of `name` in `sums`, the text of [`SUMS`], in lowercase
/// hex. `None` when no line names it. A line is `HEX  NAME`, or
/// `HEX *NAME` in the binary mode of `sha256sum`.
///
/// ```
/// use riff::release::sum_of;
///
/// let a = "0a".repeat(32);
/// let b = "B1".repeat(32);
/// let sums = format!("{a}  riff-x\n{b} *riff-server-x\nnot a line\n");
/// assert_eq!(sum_of(&sums, "riff-x"), Some(a));
/// assert_eq!(sum_of(&sums, "riff-server-x"), Some("b1".repeat(32)));
/// assert_eq!(sum_of(&sums, "riff"), None);
/// assert_eq!(sum_of("abc  riff-x\n", "riff-x"), None);
/// ```
pub fn sum_of(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hex, rest) = line.split_once(' ')?;
        let file = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('*'))?;
        let hex_ok = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
        (file == name && hex_ok).then(|| hex.to_ascii_lowercase())
    })
}

/// The SHA-256 of `bytes`, in lowercase hex.
///
/// ```
/// assert_eq!(
///     riff::release::sha256(b"abc"),
///     "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// );
/// ```
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What riff reads in the ELF of a binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf {
    /// The machine of its code, for example [`EM_X86_64`].
    pub machine: u16,
    /// The program that loads its shared libraries, from its program
    /// header `PT_INTERP`. `None` for a static binary.
    pub interpreter: Option<String>,
}

/// Reads the ELF header and the program headers of `bytes`: a 64-bit
/// little-endian executable. An error when `bytes` is not one.
///
/// ```
/// let me = std::fs::read(std::env::current_exe()?)?;
/// // A binary is an ELF file on Linux only.
/// if cfg!(target_os = "linux") {
///     let elf = riff::release::elf(&me).unwrap();
///     assert_eq!(elf.machine, riff::release::MACHINE);
///     if cfg!(target_env = "gnu") {
///         assert!(elf.interpreter.unwrap().contains("ld-linux"));
///     }
/// } else {
///     assert_eq!(riff::release::elf(&me).unwrap_err(), "it is not an ELF file");
/// }
/// assert!(riff::release::elf(b"\x7fELF").is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn elf(bytes: &[u8]) -> Result<Elf, String> {
    const PT_INTERP: u32 = 3;
    const PHDR: usize = 56;
    let u16_at = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let u64_at = |at: usize| -> Option<usize> {
        let n = u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?);
        usize::try_from(n).ok()
    };
    if bytes.get(..4) != Some(b"\x7fELF".as_slice()) {
        return Err("it is not an ELF file".into());
    }
    if bytes.get(4..6) != Some([2, 1].as_slice()) {
        return Err("it is not a 64-bit little-endian ELF file".into());
    }
    let short = || "its ELF header is too short".to_owned();
    let kind = u16_at(16).ok_or_else(short)?;
    if kind != 2 && kind != 3 {
        return Err(format!("its ELF type {kind} is not an executable"));
    }
    let machine = u16_at(18).ok_or_else(short)?;
    let table = u64_at(32).ok_or_else(short)?;
    let size = usize::from(u16_at(54).ok_or_else(short)?);
    let count = usize::from(u16_at(56).ok_or_else(short)?);
    if count > 0 && size < PHDR {
        return Err(format!("its program headers have the size {size}"));
    }
    let mut interpreter = None;
    for i in 0..count {
        let at = i
            .checked_mul(size)
            .and_then(|o| o.checked_add(table))
            .filter(|at| at.checked_add(PHDR).is_some_and(|end| end <= bytes.len()))
            .ok_or_else(|| "its program headers are past its end".to_owned())?;
        if u32_at(at) != Some(PT_INTERP) {
            continue;
        }
        let (start, len) = u64_at(at + 8).zip(u64_at(at + 32)).ok_or_else(short)?;
        let text = start
            .checked_add(len)
            .and_then(|end| bytes.get(start..end))
            .ok_or_else(|| "its interpreter is past its end".to_owned())?;
        let text = String::from_utf8_lossy(text);
        interpreter = Some(text.trim_end_matches('\0').to_owned());
    }
    Ok(Elf {
        machine,
        interpreter,
    })
}

/// Checks that `bytes` is a static binary for the ELF machine
/// `machine`: an ELF with no interpreter. The error says why not.
///
/// ```
/// use riff::release::{check, EM_X86_64};
///
/// let me = std::fs::read(std::env::current_exe()?)?;
/// if cfg!(target_env = "gnu") {
///     let error = check(&me, riff::release::MACHINE).unwrap_err();
///     assert!(error.starts_with("it needs the shared libraries of /"), "{error}");
/// }
/// assert_eq!(check(b"MZ", EM_X86_64).unwrap_err(), "it is not an ELF file");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn check(bytes: &[u8], machine: u16) -> Result<(), String> {
    static_elf(elf(bytes)?, machine)
}

/// Checks that `elf` is a static binary for the ELF machine `machine`.
fn static_elf(elf: Elf, machine: u16) -> Result<(), String> {
    if elf.machine != machine {
        return Err(format!(
            "its code is for the ELF machine {}, not {machine}",
            elf.machine
        ));
    }
    match elf.interpreter {
        Some(path) => Err(format!(
            "it needs the shared libraries of {path}: it is not static"
        )),
        None => Ok(()),
    }
}

/// Checks that `bytes` is a 64-bit Mach-O executable for Apple silicon
/// (01M6Z8FM2NRD0BSPW63603NN4Y). The error says why not.
///
/// ```
/// use riff::release::{CPU_ARM64, check_mac};
///
/// let mut bytes = vec![0u8; 32];
/// bytes[..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
/// bytes[4..8].copy_from_slice(&CPU_ARM64.to_le_bytes());
/// bytes[12..16].copy_from_slice(&2u32.to_le_bytes());
/// assert_eq!(check_mac(&bytes), Ok(()));
/// assert_eq!(check_mac(b"\x7fELF").unwrap_err(), "it is not a 64-bit Mach-O file");
/// bytes[4..8].copy_from_slice(&0x0100_0007_u32.to_le_bytes());
/// assert_eq!(
///     check_mac(&bytes).unwrap_err(),
///     "its code is for the Mach-O CPU 0x1000007, not 0x100000c"
/// );
/// ```
pub fn check_mac(bytes: &[u8]) -> Result<(), String> {
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    const MH_EXECUTE: u32 = 2;
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if u32_at(0) != Some(MH_MAGIC_64) {
        return Err("it is not a 64-bit Mach-O file".into());
    }
    let short = || "its Mach-O header is too short".to_owned();
    let cpu = u32_at(4).ok_or_else(short)?;
    if cpu != CPU_ARM64 {
        return Err(format!(
            "its code is for the Mach-O CPU {cpu:#x}, not {CPU_ARM64:#x}"
        ));
    }
    let kind = u32_at(12).ok_or_else(short)?;
    if kind != MH_EXECUTE {
        return Err(format!("its Mach-O type {kind} is not an executable"));
    }
    Ok(())
}

/// Checks that `bytes` is a file of the [`Kind`] `kind`: [`check`] or
/// [`check_mac`].
pub fn check_kind(bytes: &[u8], kind: Kind) -> Result<(), String> {
    match kind {
        Kind::Static(machine) => check(bytes, machine),
        Kind::Mac => check_mac(bytes),
    }
}

/// [`check`] of the file at `path`, for the ELF machine of this CPU
/// ([`MACHINE`]). The error says why it is not a static binary.
///
/// ```
/// use riff::release::check_file;
///
/// let dir = tempfile::tempdir()?;
/// let script = isolated::write_program(dir.path().join("riff"), "#!/bin/sh\n");
/// assert_eq!(check_file(&script).unwrap_err(), "it is not an ELF file");
/// let none = check_file(&dir.path().join("none")).unwrap_err();
/// assert!(none.starts_with("riff cannot read "), "{none}");
/// let me = std::env::current_exe()?;
/// if cfg!(target_env = "gnu") {
///     assert!(check_file(&me).unwrap_err().ends_with("it is not static"));
/// }
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn check_file(path: &Path) -> Result<(), String> {
    check_file_of(path, MACHINE)
}

/// [`check`] of the file at `path`, for the ELF machine `machine`.
fn check_file_of(path: &Path, machine: u16) -> Result<(), String> {
    use std::io::Read;
    let cannot = |e: std::io::Error| format!("riff cannot read {}: {e}", path.display());
    // The headers are at the start of the file: riff reads the whole
    // file only when they go past the head.
    let mut head = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(HEAD).read_to_end(&mut head))
        .map_err(cannot)?;
    let elf = match elf(&head) {
        Err(_) if head.len() as u64 == HEAD => elf(&std::fs::read(path).map_err(cannot)?)?,
        read => read?,
    };
    static_elf(elf, machine)
}

/// True when the riff of this process must be static, and is not
/// (01M510NWBNC20GXFRMHPW12TW9): `me` is the `riff` of the bin folder
/// `bin` of cargo, and the `riff` or the `riff-server` there is not
/// static. A binary that is not there needs nothing. A riff that runs
/// from another folder, for example the build of a clone, needs
/// nothing. On a Mac, `riff` and `riff-server` are Mach-O files: only
/// the Linux riff of the sessions beside them must be static, and be
/// there ([`session_riff`], 01M6Z8FM5F0Y7BQYNF8XS1B0T2).
///
/// ```
/// use riff::release::needs_static;
///
/// let bin = tempfile::tempdir()?;
/// let riff = isolated::write_program(bin.path().join("riff"), "#!/bin/sh\n");
/// assert!(needs_static(&riff, bin.path()));
/// let other = tempfile::tempdir()?;
/// assert!(!needs_static(&other.path().join("riff"), bin.path()));
/// assert!(!needs_static(&riff, other.path()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn needs_static(me: &Path, bin: &Path) -> bool {
    needs_static_on(me, bin, cfg!(target_os = "macos"))
}

/// [`needs_static`] on a Mac when `mac`, else on Linux.
fn needs_static_on(me: &Path, bin: &Path, mac: bool) -> bool {
    let riff = std::fs::canonicalize(bin.join("riff"));
    let me = std::fs::canonicalize(me);
    if !matches!((riff, me), (Ok(riff), Ok(me)) if riff == me) {
        return false;
    }
    if mac {
        return check_file_of(&bin.join(asset("riff", MAC_SESSION)), EM_AARCH64).is_err();
    }
    BINARIES
        .iter()
        .map(|binary| bin.join(binary))
        .any(|path| path.exists() && check_file(&path).is_err())
}

/// The bin folder of `cargo install`: `CARGO_INSTALL_ROOT/bin`, else
/// `CARGO_HOME/bin`, else `~/.cargo/bin`. So `riff update` puts riff
/// where the person installed it with cargo.
///
/// ```
/// use std::ffi::OsString;
/// use std::path::Path;
/// use riff::release::bin_dir;
///
/// fn env(vars: &'static [(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
///     |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.into())
/// }
/// let all = env(&[("CARGO_INSTALL_ROOT", "/opt/c"), ("CARGO_HOME", "/c"), ("HOME", "/h")]);
/// assert_eq!(bin_dir(all).unwrap(), Path::new("/opt/c/bin"));
/// let cargo_home = env(&[("CARGO_HOME", "/c"), ("HOME", "/h")]);
/// assert_eq!(bin_dir(cargo_home).unwrap(), Path::new("/c/bin"));
/// assert_eq!(bin_dir(env(&[("HOME", "/h")])).unwrap(), Path::new("/h/.cargo/bin"));
/// assert_eq!(bin_dir(env(&[])), None);
/// ```
pub fn bin_dir(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let root = set("CARGO_INSTALL_ROOT")
        .or_else(|| set("CARGO_HOME"))
        .or_else(|| set("HOME").map(|home| home.join(".cargo")))?;
    Some(root.join("bin"))
}

/// Where the release files are: the GitHub releases of the repository,
/// else the value of [`RELEASES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The GitHub releases of the repository.
    GitHub,
    /// A web server, with no `/` at the end.
    Web(String),
    /// A folder.
    Folder(PathBuf),
}

impl Source {
    /// The source of `releases`, the value of [`RELEASES`]: a URL with
    /// `http://` or `https://`, else a folder. With no value, the GitHub
    /// releases of the repository.
    ///
    /// ```
    /// use riff::release::Source;
    ///
    /// assert_eq!(Source::of(None), Source::GitHub);
    /// assert_eq!(Source::of(Some("http://127.0.0.1:9/")), Source::Web("http://127.0.0.1:9".into()));
    /// assert_eq!(Source::of(Some("/srv/riff")), Source::Folder("/srv/riff".into()));
    /// ```
    pub fn of(releases: Option<&str>) -> Source {
        match releases {
            None => Source::GitHub,
            Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
                Source::Web(url.trim_end_matches('/').to_owned())
            }
            Some(folder) => Source::Folder(folder.into()),
        }
    }

    /// Where the release file `name` of `tag` is.
    ///
    /// ```
    /// use riff::release::Source;
    ///
    /// let place = Source::GitHub.place("v1.4.0", "SHA256SUMS");
    /// assert_eq!(place, "https://github.com/como-technologies/riff/releases/download/v1.4.0/SHA256SUMS");
    /// assert_eq!(Source::of(Some("/srv/riff")).place("v1.4.0", "riff-x"), "/srv/riff/v1.4.0/riff-x");
    /// ```
    pub fn place(&self, tag: &str, name: &str) -> String {
        match self {
            Source::GitHub => format!(
                "{}/releases/download/{tag}/{name}",
                env!("CARGO_PKG_REPOSITORY")
            ),
            Source::Web(base) => format!("{base}/{tag}/{name}"),
            Source::Folder(dir) => dir.join(tag).join(name).display().to_string(),
        }
    }

    /// Where the reply for the release that GitHub marks "Latest" is
    /// (01M510MPR1S0EQNRK2195XTXG5): the API of GitHub, else the file
    /// [`LATEST`].
    ///
    /// ```
    /// use riff::release::Source;
    ///
    /// assert_eq!(
    ///     Source::GitHub.latest_place(),
    ///     "https://api.github.com/repos/como-technologies/riff/releases/latest"
    /// );
    /// assert_eq!(Source::of(Some("http://m:9/")).latest_place(), "http://m:9/latest");
    /// assert_eq!(Source::of(Some("/srv/riff")).latest_place(), "/srv/riff/latest");
    /// ```
    pub fn latest_place(&self) -> String {
        match self {
            Source::GitHub => format!(
                "https://api.github.com/repos/{}/releases/latest",
                env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/")
            ),
            Source::Web(base) => format!("{base}/{LATEST}"),
            Source::Folder(dir) => dir.join(LATEST).display().to_string(),
        }
    }

    /// The bytes of the release file `name` of `tag`. `None` when the
    /// release has no such file: a 404, or no file in the folder.
    async fn get(
        &self,
        client: &reqwest::Client,
        tag: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>> {
        self.read(client, &self.place(tag, name)).await
    }

    /// The bytes at `place`, a place of this source. `None` when there is
    /// no such file: a 404, or no file in the folder.
    async fn read(&self, client: &reqwest::Client, place: &str) -> Result<Option<Vec<u8>>> {
        match self {
            Source::GitHub | Source::Web(_) => fetch(client, place).await,
            Source::Folder(_) => match std::fs::read(place) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).with_context(|| format!("cannot read {place}")),
            },
        }
    }
}

/// What [`install`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// It put the release files of this machine in this folder.
    Static(PathBuf),
    /// The release has no files for this machine. It changed nothing.
    None,
}

/// Installs the release files of `tag` for this machine in the bin
/// folder of cargo (01M4ZPJ0BAQZ6AN06FGXS5D4FP,
/// 01M6Z8FM2NRD0BSPW63603NN4Y): [`install_from`] the place of
/// [`RELEASES`] for [`TARGET`]. It gives [`Installed::None`] when the
/// machine has no [`TARGET`].
pub async fn install(tag: &str) -> Result<Installed> {
    let Some(target) = TARGET else {
        return Ok(Installed::None);
    };
    let source = Source::of(std::env::var(RELEASES).ok().as_deref());
    install_from(&source, tag, target, bin_dir(|name| std::env::var_os(name))).await
}

/// Installs the [`files`] of `target` of the release `tag` at `source` in
/// `dir`, the bin folder of cargo. It gives [`Installed::None`] when the
/// release has no [`sums`] of `target`, or no line for a file of it
/// (01M4ZPJ0DJ9FTYNPTBBVJCTB37). An error when `dir` is `None`, or when a
/// file does not come, or fails a check: then it changes nothing.
///
/// ```
/// use riff::release::{Installed, LINUX, MAC, Source, install_from};
///
/// let releases = tempfile::tempdir()?;
/// std::fs::create_dir(releases.path().join("v1.5.0"))?;
/// // The release has the files of Linux only.
/// std::fs::write(releases.path().join("v1.5.0/SHA256SUMS"), "")?;
/// let source = Source::Folder(releases.path().into());
/// let bin = tempfile::tempdir()?;
/// let run = |target| {
///     let install = install_from(&source, "v1.5.0", target, Some(bin.path().into()));
///     tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(install)
/// };
/// assert_eq!(run(MAC).unwrap(), Installed::None);
/// // SHA256SUMS has no line for riff.
/// assert_eq!(run(LINUX).unwrap(), Installed::None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub async fn install_from(
    source: &Source,
    tag: &str,
    target: &str,
    dir: Option<PathBuf>,
) -> Result<Installed> {
    let client = client()?;
    let sums_name = sums(target);
    let Some(sums) = source.get(&client, tag, &sums_name).await? else {
        return Ok(Installed::None);
    };
    let sums = String::from_utf8_lossy(&sums).into_owned();
    let mut wanted = Vec::new();
    for file in files(target) {
        let Some(sum) = sum_of(&sums, &file.asset) else {
            return Ok(Installed::None);
        };
        wanted.push((file, sum));
    }
    let dir = dir.context("cannot find the bin folder of cargo: set HOME or CARGO_HOME")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
    println!("{}", crate::text::downloads(tag, target, &dir));
    // Each file passes each check before riff writes one.
    let mut good = Vec::new();
    for (file, sum) in wanted {
        let name = &file.asset;
        let bytes = source
            .get(&client, tag, name)
            .await?
            .with_context(|| format!("the release {tag} has no {name}"))?;
        let got = sha256(&bytes);
        if got != sum {
            bail!("the SHA-256 of {name} of {tag} is {got}, not {sum} of {sums_name}");
        }
        check_kind(&bytes, file.kind).map_err(|why| anyhow::anyhow!("{name} of {tag}: {why}"))?;
        good.push((file.name, bytes));
    }
    let mut news = Vec::new();
    for (name, bytes) in good {
        match write_new(&dir, &name, &bytes) {
            Ok(new) => news.push((new, dir.join(name))),
            Err(e) => {
                for (new, _) in &news {
                    let _ = std::fs::remove_file(new);
                }
                return Err(e);
            }
        }
    }
    for (new, old) in &news {
        if let Err(e) = std::fs::rename(new, old) {
            // A new file that moved is not there any more.
            for (new, _) in &news {
                let _ = std::fs::remove_file(new);
            }
            return Err(e).with_context(|| format!("cannot replace {}", old.display()));
        }
    }
    Ok(Installed::Static(dir))
}

/// Checks that `bytes` are the static `riff` of the release `tag` for
/// `target` (01M51M3ZKWQYBBPD97R0G9QAX0): their SHA-256 is the line of
/// [`asset`] `riff` in [`sums`] of `target` of the release at `source`.
/// A check build or a dev build has the version of a release, but
/// another SHA-256. `Err` says why the bytes are not the release: no
/// `target`, no file of sums, no line, another SHA-256, or a read that
/// failed.
///
/// ```
/// use riff::release::{MAC, Source, asset, is_release, sha256};
///
/// let dir = tempfile::tempdir()?;
/// let target = "x86_64-unknown-linux-musl";
/// std::fs::create_dir(dir.path().join("v1.5.0"))?;
/// let line = format!("{}  {}\n", sha256(b"the riff"), asset("riff", target));
/// std::fs::write(dir.path().join("v1.5.0/SHA256SUMS"), line)?;
/// let source = Source::Folder(dir.path().into());
/// let check = |bytes: &'static [u8], tag: &'static str, target| {
///     let source = source.clone();
///     tokio::runtime::Builder::new_current_thread()
///         .build()
///         .unwrap()
///         .block_on(async move { is_release(bytes, tag, &source, target).await })
/// };
/// assert_eq!(check(b"the riff", "v1.5.0", Some(target)), Ok(()));
/// let dev = check(b"a dev build", "v1.5.0", Some(target)).unwrap_err();
/// assert!(dev.ends_with("it is a check build or a dev build"), "{dev}");
/// let none = check(b"the riff", "v1.4.0", Some(target)).unwrap_err();
/// assert_eq!(none, "the release v1.4.0 has no SHA256SUMS");
/// assert!(check(b"the riff", "v1.5.0", None).is_err());
/// // A Mac reads the sums of a Mac.
/// let mac = check(b"the riff", "v1.5.0", Some(MAC)).unwrap_err();
/// assert_eq!(mac, "the release v1.5.0 has no SHA256SUMS-aarch64-apple-darwin");
/// let line = format!("{}  {}\n", sha256(b"the riff"), asset("riff", MAC));
/// std::fs::write(dir.path().join("v1.5.0/SHA256SUMS-aarch64-apple-darwin"), line)?;
/// assert_eq!(check(b"the riff", "v1.5.0", Some(MAC)), Ok(()));
/// # Ok::<(), std::io::Error>(())
/// ```
pub async fn is_release(
    bytes: &[u8],
    tag: &str,
    source: &Source,
    target: Option<&str>,
) -> Result<(), String> {
    let target = target.ok_or(crate::text::NOT_RELEASE_TARGET)?;
    let sums_name = sums(target);
    let read = |e: anyhow::Error| crate::text::not_release_read(tag, &sums_name, &format!("{e:#}"));
    let sums = source
        .get(&client().map_err(read)?, tag, &sums_name)
        .await
        .map_err(read)?
        .ok_or_else(|| crate::text::not_release_no_sums(tag, &sums_name))?;
    let name = asset("riff", target);
    let sum = sum_of(&String::from_utf8_lossy(&sums), &name)
        .ok_or_else(|| crate::text::not_release_no_line(tag, &sums_name, &name))?;
    let got = sha256(bytes);
    if got != sum {
        return Err(crate::text::not_release_sum(&got, tag, &sums_name, &name));
    }
    Ok(())
}

/// The tag of the release in `reply`, a reply of the API of GitHub for
/// one release: its `tag_name`. An error when the reply has none, or
/// when the tag is not a release tag `vX.Y.Z`.
///
/// ```
/// use riff::release::tag_of;
///
/// let reply = br#"{"tag_name": "v1.4.0", "prerelease": false, "draft": false}"#;
/// assert_eq!(tag_of(reply).unwrap(), "v1.4.0");
/// assert!(tag_of(br#"{"tag_name": "main"}"#).is_err());
/// assert!(tag_of(br#"{"message": "Not Found"}"#).is_err());
/// assert!(tag_of(b"<html></html>").is_err());
/// ```
pub fn tag_of(reply: &[u8]) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }
    let release: Release = serde_json::from_slice(reply)
        .map_err(|e| format!("it is not a release of the API of GitHub: {e}"))?;
    crate::lifecycle::parse_tag(&release.tag_name)
}

/// The tag of the release that GitHub marks "Latest"
/// (01M510MPR1S0EQNRK2195XTXG5), from [`Source::latest_place`] of
/// [`RELEASES`]. It is never a prerelease, also when its tag is higher.
/// An error names `riff update --tag`.
pub async fn latest() -> Result<String> {
    let source = Source::of(std::env::var(RELEASES).ok().as_deref());
    let place = source.latest_place();
    let name_one =
        || format!("cannot find the Latest release at {place}. Name one: riff update --tag vX.Y.Z");
    let reply = source
        .read(&client()?, &place)
        .await
        .with_context(name_one)?
        .with_context(|| {
            format!("{place} names no Latest release. Name one: riff update --tag vX.Y.Z")
        })?;
    tag_of(&reply)
        .map_err(anyhow::Error::msg)
        .with_context(name_one)
}

/// The HTTP client of the release files. It names riff in its
/// `User-Agent`: the API of GitHub refuses a request with none.
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("riff/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(WAIT)
        .read_timeout(WAIT)
        .build()
        .context("cannot make the HTTP client of the releases")
}

/// The body of `url`. `None` for a 404: the release has no such file.
async fn fetch(client: &reqwest::Client, url: &str) -> Result<Option<Vec<u8>>> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("cannot get {url}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response
        .error_for_status()
        .with_context(|| format!("cannot get {url}"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("cannot read {url}"))?;
    Ok(Some(bytes.to_vec()))
}

/// Writes `bytes` to a new file beside `dir/binary`, with the mode
/// 0755, and gives its path. The file has the ID of this process in its
/// name, so two updates do not share it.
fn write_new(dir: &Path, binary: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = dir.join(format!(".{binary}.new-{}", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&path);
        anyhow::Error::new(e).context(format!("cannot write {}", path.display()))
    })?;
    Ok(path)
}

/// A 64-bit little-endian ELF of `machine` with one program header of
/// each type in `types`, for tests. A `PT_INTERP` (3) names `interp`.
#[cfg(test)]
pub(crate) fn fake_elf(machine: u16, types: &[u32], interp: &str) -> Vec<u8> {
    let table = 64usize;
    let strings = table + 56 * types.len();
    let mut bytes = vec![0u8; strings];
    bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
    bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&machine.to_le_bytes());
    bytes[32..40].copy_from_slice(&(table as u64).to_le_bytes());
    bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&(types.len() as u16).to_le_bytes());
    for (i, kind) in types.iter().enumerate() {
        let at = table + 56 * i;
        bytes[at..at + 4].copy_from_slice(&kind.to_le_bytes());
        bytes[at + 8..at + 16].copy_from_slice(&(strings as u64).to_le_bytes());
        bytes[at + 32..at + 40].copy_from_slice(&(interp.len() as u64 + 1).to_le_bytes());
    }
    bytes.extend_from_slice(interp.as_bytes());
    bytes.push(0);
    bytes
}

/// Writes a fake riff to `path` for a test: a static ELF of this CPU,
/// or, with `interp`, one that needs the shared libraries of `interp`.
#[cfg(test)]
pub(crate) fn fake_riff(path: &Path, interp: Option<&str>) -> PathBuf {
    let bytes = match interp {
        Some(interp) => fake_elf(MACHINE, &[3, 1], interp),
        None => fake_elf(MACHINE, &[1], ""),
    };
    std::fs::write(path, bytes).unwrap();
    path.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_riff_needs_the_static_files_only_when_it_runs_from_the_bin_folder() {
        let bin = tempfile::tempdir().unwrap();
        let riff = fake_riff(&bin.path().join("riff"), None);
        assert!(!needs_static(&riff, bin.path()), "a static riff");
        let server = bin.path().join("riff-server");
        fake_riff(&server, Some("/lib64/ld-linux-x86-64.so.2"));
        assert!(needs_static(&riff, bin.path()), "riff-server is not static");
        fake_riff(&server, None);
        fake_riff(&riff, Some("/lib64/ld-linux-x86-64.so.2"));
        assert!(needs_static(&riff, bin.path()), "riff is not static");
        // A link to the riff of the folder is that riff.
        let other = tempfile::tempdir().unwrap();
        let link = other.path().join("riff");
        std::os::unix::fs::symlink(&riff, &link).unwrap();
        assert!(needs_static(&link, bin.path()));
        // A riff of another folder.
        let copy = isolated::copy_program(&riff, other.path().join("copy"));
        assert!(!needs_static(&copy, bin.path()));
    }

    #[test]
    fn a_static_binary_passes_and_one_with_an_interpreter_fails() {
        let load = 1;
        let interp = 3;
        let ok = fake_elf(EM_X86_64, &[load, load], "");
        assert_eq!(check(&ok, EM_X86_64), Ok(()));
        let dynamic = fake_elf(EM_X86_64, &[interp, load], "/lib64/ld-linux-x86-64.so.2");
        assert_eq!(
            elf(&dynamic).unwrap().interpreter.as_deref(),
            Some("/lib64/ld-linux-x86-64.so.2")
        );
        assert_eq!(
            check(&dynamic, EM_X86_64).unwrap_err(),
            "it needs the shared libraries of /lib64/ld-linux-x86-64.so.2: it is not static"
        );
    }

    #[test]
    fn a_binary_of_another_cpu_fails() {
        let arm = fake_elf(183, &[1], "");
        assert_eq!(
            check(&arm, EM_X86_64).unwrap_err(),
            "its code is for the ELF machine 183, not 62"
        );
    }

    #[test]
    fn a_broken_elf_is_an_error_and_no_panic() {
        let good = fake_elf(EM_X86_64, &[3], "/lib/ld.so");
        for cut in 0..good.len() {
            let _ = elf(&good[..cut]);
        }
        let mut far = good.clone();
        far[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(
            elf(&far).unwrap_err(),
            "its program headers are past its end"
        );
        let mut far = good.clone();
        far[64 + 8..64 + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(elf(&far).unwrap_err(), "its interpreter is past its end");
        let mut small = good;
        small[54..56].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(
            elf(&small).unwrap_err(),
            "its program headers have the size 8"
        );
        let mut object = fake_elf(EM_X86_64, &[], "");
        object[16..18].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            elf(&object).unwrap_err(),
            "its ELF type 1 is not an executable"
        );
        assert_eq!(
            elf(b"\x7fELF\x01\x01").unwrap_err(),
            "it is not a 64-bit little-endian ELF file"
        );
    }

    // A test binary is an ELF file on Linux only.
    #[cfg(target_os = "linux")]
    #[test]
    fn this_test_binary_is_read_as_the_loader_sees_it() {
        let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let elf = elf(&me).unwrap();
        assert_eq!(elf.machine, MACHINE);
        assert_eq!(
            elf.interpreter.is_some(),
            cfg!(target_env = "gnu"),
            "{elf:?}"
        );
    }

    #[test]
    fn the_new_file_has_the_mode_of_a_program_and_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_new(dir.path(), "riff", b"bytes").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"bytes");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".riff.new-")
        );
    }

    /// A Mach-O header of `cpu` and the type `kind`, then `text`.
    fn fake_mac(cpu: u32, kind: u32, text: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 32];
        bytes[..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&cpu.to_le_bytes());
        bytes[12..16].copy_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    /// A static ELF of `machine`, then `text`.
    fn fake_static(machine: u16, text: &str) -> Vec<u8> {
        let mut bytes = fake_elf(machine, &[1], "");
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    /// Puts the release `tag` in `dir`: each file of `files` by its name
    /// in the release, and the file of sums `sums` with a line for each.
    fn put(dir: &Path, tag: &str, sums: &str, files: &[(String, Vec<u8>)]) {
        let dir = dir.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        let mut lines = String::new();
        for (name, bytes) in files {
            std::fs::write(dir.join(name), bytes).unwrap();
            lines += &format!("{}  {name}\n", sha256(bytes));
        }
        std::fs::write(dir.join(sums), lines).unwrap();
    }

    /// The files of a good release of a Mac: `riff`, `riff-server` and
    /// the Linux riff of the sessions.
    fn mac_files() -> Vec<(String, Vec<u8>)> {
        vec![
            (asset("riff", MAC), fake_mac(CPU_ARM64, 2, "the Mac riff")),
            (
                asset("riff-server", MAC),
                fake_mac(CPU_ARM64, 2, "the Mac riff-server"),
            ),
            (
                asset("riff", MAC_SESSION),
                fake_static(EM_AARCH64, "the Linux riff of a Mac"),
            ),
        ]
    }

    /// The files of a good release of Linux.
    fn linux_files() -> Vec<(String, Vec<u8>)> {
        vec![
            (asset("riff", LINUX), fake_static(EM_X86_64, "the riff")),
            (
                asset("riff-server", LINUX),
                fake_static(EM_X86_64, "the riff-server"),
            ),
        ]
    }

    /// The names of the files in `dir`, in order.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// 01M6Z8F7B3HVVNGQMX8GP1Z7Q1, 01M6Z8FM2NRD0BSPW63603NN4Y: on a Mac,
    /// `riff update` takes the files of a Mac from their own file of
    /// sums, and installs the Linux riff of the sessions beside `riff`.
    #[tokio::test]
    async fn a_mac_installs_the_files_of_a_mac_and_the_linux_riff_of_its_sessions() {
        let releases = tempfile::tempdir().unwrap();
        put(releases.path(), "v1.5.0", SUMS, &linux_files());
        put(releases.path(), "v1.5.0", &sums(MAC), &mac_files());
        let bin = tempfile::tempdir().unwrap();
        std::fs::write(bin.path().join("riff"), "the old riff").unwrap();
        let source = Source::Folder(releases.path().into());

        let done = install_from(&source, "v1.5.0", MAC, Some(bin.path().into()))
            .await
            .unwrap();
        assert_eq!(done, Installed::Static(bin.path().into()));
        assert_eq!(
            names(bin.path()),
            ["riff", "riff-aarch64-unknown-linux-musl", "riff-server"],
            "no new file stays"
        );
        for (name, bytes) in [
            ("riff", fake_mac(CPU_ARM64, 2, "the Mac riff")),
            ("riff-server", fake_mac(CPU_ARM64, 2, "the Mac riff-server")),
            (
                "riff-aarch64-unknown-linux-musl",
                fake_static(EM_AARCH64, "the Linux riff of a Mac"),
            ),
        ] {
            assert_eq!(
                std::fs::read(bin.path().join(name)).unwrap(),
                bytes,
                "{name}"
            );
            let path = bin.path().join(name);
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755, "{name}");
        }
        // The Linux riff of the sessions is the riff that the daemon on a
        // Mac binds, and it passes the check of the daemon.
        let riff = bin.path().join("riff");
        assert!(!needs_static_on(&riff, bin.path(), true));
        assert_eq!(
            check_file_of(&bin.path().join(asset("riff", MAC_SESSION)), EM_AARCH64),
            Ok(())
        );
    }

    /// 01M6Z8F7B3HVVNGQMX8GP1Z7Q1: Linux stays the same. Also beside the
    /// files of a Mac, `riff update` on Linux reads `SHA256SUMS`, and
    /// takes `riff-x86_64-unknown-linux-musl` and
    /// `riff-server-x86_64-unknown-linux-musl` as before.
    #[tokio::test]
    async fn linux_takes_the_same_files_as_before_beside_the_files_of_a_mac() {
        assert_eq!(sums(LINUX), "SHA256SUMS");
        let releases = tempfile::tempdir().unwrap();
        put(releases.path(), "v1.5.0", SUMS, &linux_files());
        put(releases.path(), "v1.5.0", &sums(MAC), &mac_files());
        let bin = tempfile::tempdir().unwrap();
        let source = Source::Folder(releases.path().into());

        let done = install_from(&source, "v1.5.0", LINUX, Some(bin.path().into()))
            .await
            .unwrap();
        assert_eq!(done, Installed::Static(bin.path().into()));
        assert_eq!(names(bin.path()), ["riff", "riff-server"]);
        assert_eq!(
            std::fs::read(bin.path().join("riff")).unwrap(),
            fake_static(EM_X86_64, "the riff")
        );
        assert_eq!(
            std::fs::read(bin.path().join("riff-server")).unwrap(),
            fake_static(EM_X86_64, "the riff-server")
        );

        // With the files of a Mac only, Linux has no files.
        let mac_only = tempfile::tempdir().unwrap();
        put(mac_only.path(), "v1.5.0", &sums(MAC), &mac_files());
        let source = Source::Folder(mac_only.path().into());
        let other = tempfile::tempdir().unwrap();
        let done = install_from(&source, "v1.5.0", LINUX, Some(other.path().into()))
            .await
            .unwrap();
        assert_eq!(done, Installed::None);
        assert!(names(other.path()).is_empty());
    }

    /// 01M6Z8FM2NRD0BSPW63603NN4Y: a file of a Mac that fails a check:
    /// `riff update` installs nothing, and names the reason. With no line
    /// for a file of a Mac, it installs nothing, and cargo builds.
    #[tokio::test]
    async fn a_mac_installs_nothing_when_a_file_fails_a_check() {
        let linux_riff = asset("riff", MAC_SESSION);
        let mac_riff = asset("riff", MAC);
        let cases: [(&str, Vec<u8>, String); 4] = [
            (
                "riff-aarch64-apple-darwin",
                fake_static(EM_AARCH64, "an ELF"),
                format!("{mac_riff} of v1.5.0: it is not a 64-bit Mach-O file"),
            ),
            (
                "riff-aarch64-apple-darwin",
                fake_mac(0x0100_0007, 2, "a riff of a Mac with Intel"),
                format!(
                    "{mac_riff} of v1.5.0: its code is for the Mach-O CPU 0x1000007, \
                     not 0x100000c"
                ),
            ),
            (
                "riff-aarch64-unknown-linux-musl",
                fake_elf(EM_AARCH64, &[3, 1], "/lib/ld-linux-aarch64.so.1"),
                format!(
                    "{linux_riff} of v1.5.0: it needs the shared libraries of \
                     /lib/ld-linux-aarch64.so.1: it is not static"
                ),
            ),
            (
                "riff-aarch64-unknown-linux-musl",
                fake_static(EM_X86_64, "a Linux riff of x86-64"),
                format!("{linux_riff} of v1.5.0: its code is for the ELF machine 62, not 183"),
            ),
        ];
        for (bad, bytes, reason) in cases {
            let releases = tempfile::tempdir().unwrap();
            let mut files = mac_files();
            for (name, old) in &mut files {
                if name == bad {
                    *old = bytes.clone();
                }
            }
            put(releases.path(), "v1.5.0", &sums(MAC), &files);
            let bin = tempfile::tempdir().unwrap();
            std::fs::write(bin.path().join("riff"), "the old riff").unwrap();
            let source = Source::Folder(releases.path().into());
            let error = install_from(&source, "v1.5.0", MAC, Some(bin.path().into()))
                .await
                .unwrap_err();
            assert_eq!(format!("{error:#}"), reason);
            assert_eq!(names(bin.path()), ["riff"], "{reason}");
            assert_eq!(
                std::fs::read(bin.path().join("riff")).unwrap(),
                b"the old riff"
            );
        }

        // A file of sums of a Mac with no line for the Linux riff.
        let releases = tempfile::tempdir().unwrap();
        let mut files = mac_files();
        files.retain(|(name, _)| *name != linux_riff);
        put(releases.path(), "v1.5.0", &sums(MAC), &files);
        let source = Source::Folder(releases.path().into());
        let bin = tempfile::tempdir().unwrap();
        let done = install_from(&source, "v1.5.0", MAC, Some(bin.path().into()))
            .await
            .unwrap();
        assert_eq!(done, Installed::None);
        assert!(names(bin.path()).is_empty());
    }

    /// 01M6Z8FM5F0Y7BQYNF8XS1B0T2: on a Mac, the riff of the bin folder
    /// needs the fix when the Linux riff of the sessions is missing or is
    /// not static. Its `riff` and `riff-server` are Mach-O files.
    #[test]
    fn a_mac_needs_the_static_linux_riff_of_its_sessions() {
        let bin = tempfile::tempdir().unwrap();
        let riff = bin.path().join("riff");
        std::fs::write(&riff, fake_mac(CPU_ARM64, 2, "riff")).unwrap();
        let server = bin.path().join("riff-server");
        std::fs::write(&server, fake_mac(CPU_ARM64, 2, "riff-server")).unwrap();
        assert!(needs_static_on(&riff, bin.path(), true), "no Linux riff");
        let linux = bin.path().join(asset("riff", MAC_SESSION));
        std::fs::write(&linux, fake_elf(EM_AARCH64, &[3, 1], "/lib/ld.so")).unwrap();
        assert!(needs_static_on(&riff, bin.path(), true), "not static");
        std::fs::write(&linux, fake_static(EM_AARCH64, "")).unwrap();
        assert!(!needs_static_on(&riff, bin.path(), true));
        // On Linux, a Mach-O riff is not static.
        assert!(needs_static_on(&riff, bin.path(), false));
        // A riff of another folder needs nothing.
        let other = tempfile::tempdir().unwrap();
        std::fs::remove_file(&linux).unwrap();
        assert!(!needs_static_on(
            &other.path().join("riff"),
            bin.path(),
            true
        ));
    }

    #[test]
    fn a_mach_o_file_of_another_type_or_a_short_one_fails() {
        assert_eq!(
            check_mac(&fake_mac(CPU_ARM64, 6, "")).unwrap_err(),
            "its Mach-O type 6 is not an executable"
        );
        let good = fake_mac(CPU_ARM64, 2, "");
        assert_eq!(
            check_mac(&good[..10]).unwrap_err(),
            "its Mach-O header is too short"
        );
        for cut in 0..good.len() {
            let _ = check_mac(&good[..cut]);
        }
        assert_eq!(check_kind(&good, Kind::Mac), Ok(()));
        assert!(check_kind(&good, Kind::Static(EM_AARCH64)).is_err());
    }

    /// The test binary on a Mac is a Mach-O file for Apple silicon, as
    /// the riff of a release of a Mac is.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn this_test_binary_on_a_mac_passes_the_check_of_a_mac() {
        assert_eq!(TARGET, Some(MAC));
        let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(check_mac(&me), Ok(()));
    }
}
