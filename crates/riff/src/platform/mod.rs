//! The platform of `riff`: the parts that `riff` gives to
//! [`riff_client`] (01M54216S51X5VBXN3YAERW6J6), and each read of the
//! OS that differs between Linux and macOS (01M6YQDWT387EV7Y7XMT2G6YVM).
//! `main` gives the parts with [`install`] at its start.
//!
//! | Part | What `riff` gives |
//! |---|---|
//! | The store of the tokens and of the device key | The OS keyring: on Linux and BSD the store of riff, [`keyring_store`](crate::keyring_store), with plain sessions of the Secret Service (01M556QPPRSQ91HDED128G2X7G); the store of the `keyring` crate on each other OS |
//! | The secret files | `$RIFF_HOME/secrets` (see [`home`](crate::home)) |
//! | The local folder | [`local::dir`] |
//! | The folder of the leave marks | [`local::marks`] |
//! | The name of the machine | [`identity::this_host`] |
//! | The riff of the process | [`binary::this_riff`] |
//! | A wait line | Dim, on stderr; above the prompt in `riff chat` ([`chat::notice`]) |
//! | A reply of another build | [`auto_update::begin`]; one note on stderr, or above the prompt in `riff chat`, none in the update in the background ([`auto_update::in_background`]) |
//!
//! ```standalone_crate
//! riff::platform::install();
//! let a = riff_client::login::lock_path("http://a")?;
//! assert_ne!(a, riff_client::login::lock_path("http://b")?);
//! assert_eq!(a.parent(), riff::local::dir().as_deref());
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # The reads of the OS
//!
//! riff runs on Linux, and on macOS on Apple silicon
//! (01M6YPB5B42FZP7GCDAVTN4PWY). Each read of the OS that differs
//! between them is in this module: [`linux`] on Linux, `macos` on
//! macOS. No other module of riff reads `/proc` or `/sys`, or runs
//! `systemctl` (01M6YQDWT387EV7Y7XMT2G6YVM).
//!
//! | Read | Linux | macOS |
//! |---|---|---|
//! | [`process`]: the parent, the user, the start, the name, the arguments and the binary of a process | `/proc/PID/stat`, `cmdline` and `exe` | `proc_pidinfo` (`PROC_PIDTBSDINFO`), `proc_pidpath`, and `sysctl` `KERN_PROCARGS2` |
//! | [`Process::replaced`], [`own_replaced`]: a binary that a new file replaced | the link `exe` ends with ` (deleted)` | the file at the path of the binary is newer than the process, or gone |
//! | [`processes`], [`children`] | the folders of `/proc` | `proc_listpids` |
//! | [`environ`], [`cwd`], [`open_files`] | `/proc/PID/environ`, `cwd` and `fd` | `KERN_PROCARGS2`, `proc_pidinfo` (`PROC_PIDVNODEPATHINFO`) and `proc_pidfdinfo` |
//! | [`memory`] | `/proc/meminfo` | `sysctl` `hw.memsize`, `hw.pagesize` and `vm.page_*_count` |
//! | [`machine`]: the CPU speed, the memory and the load | cpufreq in `/sys`, `/proc/cpuinfo`, `/proc/meminfo` and `/proc/loadavg` | `sysctl` `hw.cpufrequency_max`, [`memory`] and `vm.loadavg` |
//! | [`cgroups`]: the cgroup controllers that podman needs | `/sys/fs/cgroup` | none |
//! | [`Socket`]: the path of a socket in a long folder | `/proc/self/fd/N/NAME` | the plain path, of at most [`SOCKET_MOST`] bytes (01M6Z49RD4F6EX4K9FF2Z0CET9) |
//! | [`service_manager`]: the manager of the daemon | systemd, `systemctl --user` | launchd, `launchctl` ([`launchd`], 01M6Z49H6CHSPV0FJZWY2MYHEM) |
//! | The CPU weight of the slice of the sessions | `systemctl --user set-property --runtime` ([`systemd::set_cpu_weight`], 01M778VSP6Z29FCPGX5S76ME1T) | none: no session runs on macOS |
//!
//! ```mermaid
//! flowchart LR
//!     C["riff chat, riff top, the TUI, the daemon"] --> P["platform"]
//!     P -- Linux --> L["/proc, /sys, systemctl"]
//!     P -- macOS --> M["libproc, sysctl, launchctl"]
//! ```
//!
//! ```
//! use riff::platform;
//!
//! let me = platform::process(std::process::id()).expect("this process");
//! assert_eq!(me.ppid, std::os::unix::process::parent_id());
//! assert!(!me.replaced && !platform::own_replaced());
//! let memory = platform::memory().expect("the memory of this machine");
//! assert!(memory.available <= memory.total);
//! ```

use std::io;
use std::path::{Path, PathBuf};

use riff_client::machines::Machine;
use riff_client::platform::Platform;
use riff_core::build::Build;
use riff_core::wire::RiffBinary;

use crate::chat;
use crate::style::{DIM, styled};
use crate::{auto_update, binary, identity, local};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "linux")]
pub mod systemd;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
use macos as os;

pub mod launchd;

/// The service manager of the daemon on this OS: [`systemd::Systemd`]
/// on Linux, [`launchd::Launchd`] on macOS.
pub use os::Manager;

/// The platform of `riff`.
struct Terminal;

impl Platform for Terminal {
    fn open_store(&self) -> keyring_core::Result<()> {
        open_store()
    }

    fn secret_files(&self) -> Option<PathBuf> {
        crate::home::dir().map(|home| home.join("secrets"))
    }

    fn local_dir(&self) -> Option<PathBuf> {
        local::dir()
    }

    fn marks(&self) -> Option<PathBuf> {
        local::marks()
    }

    fn host(&self) -> String {
        identity::this_host()
    }

    fn binary(&self) -> RiffBinary {
        binary::this_riff()
    }

    fn dim(&self, text: &str) -> String {
        styled(DIM, text)
    }

    fn show_wait(&self, line: &str) {
        chat::notice(line);
    }

    fn server_build(&self, base: &str, server: &Build) {
        auto_update::begin(base, server);
    }

    fn other_build(&self, note: &str) {
        if !auto_update::in_background() {
            chat::notice(&format!("riff: {note}"));
        }
    }
}

/// Gives the platform of `riff` to [`riff_client`], one time for the
/// process.
pub fn install() {
    riff_client::platform::set(Box::new(Terminal));
}

/// Sets the store of riff with plain sessions of the Secret Service
/// (01M556QPPRSQ91HDED128G2X7G).
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn open_store() -> keyring_core::Result<()> {
    let store = crate::keyring_store::Store::new()?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// Sets the store of the `keyring` crate: the Keychain of macOS, or the
/// Credential Manager of Windows.
#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
fn open_store() -> keyring_core::Result<()> {
    keyring::Entry::store_status()
        .as_ref()
        .map(|_| ())
        .map_err(|e| keyring_core::Error::PlatformFailure(e.to_string().into()))
}

/// One process of this host, as the OS shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Process {
    /// The process ID.
    pub pid: u32,
    /// The ID of the parent.
    pub ppid: u32,
    /// The ID of its user.
    pub uid: u32,
    /// The start of the process: the same for each read of one process,
    /// and another for a new process with the same ID. On Linux, the
    /// ticks of the clock after the boot. On macOS, the microseconds
    /// after 1970.
    pub start: u64,
    /// How long the process has run, in seconds.
    pub secs: u64,
    /// The name of its program, as the OS keeps it.
    pub name: String,
    /// Its command line. Empty when the OS gives none, for example for a
    /// thread of the kernel.
    pub args: Vec<String>,
    /// The path of its binary on disk, also after a new file replaced it.
    /// `None` when riff cannot read it.
    pub exe: Option<PathBuf>,
    /// True when a new file replaced the binary of the process after its
    /// start, or the binary is gone.
    pub replaced: bool,
    /// True when the process runs in a container of podman: Linux only. On
    /// macOS, no process of a container shows.
    pub in_container: bool,
}

/// The process `pid`, or `None` when it ended or riff cannot read it.
///
/// ```
/// use riff::platform::process;
///
/// let me = process(std::process::id()).expect("this process");
/// assert_eq!(me.pid, std::process::id());
/// assert_eq!(me.uid, nix::unistd::getuid().as_raw());
/// assert!(!me.args.is_empty() && !me.name.is_empty());
/// assert!(riff::binary::same_file(&me.exe.unwrap(), &std::env::current_exe()?));
/// assert_eq!(process(std::process::id()).unwrap().start, me.start);
/// assert_eq!(process(u32::MAX), None);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn process(pid: u32) -> Option<Process> {
    os::process(pid)
}

/// The processes of the user of this process, in the order of their IDs.
/// A process in a container of podman has [`Process::in_container`].
pub fn processes() -> Vec<Process> {
    os::processes()
}

/// The IDs of the child processes of `pid`. Empty when riff cannot read
/// them.
pub fn children(pid: u32) -> Vec<u32> {
    os::children(pid)
}

/// True when a new file replaced the binary of this process after its
/// start, or the binary is gone.
pub fn own_replaced() -> bool {
    os::own_replaced()
}

/// A path that reads the binary that this process runs. On Linux,
/// `/proc/self/exe`: it reads the binary also after a new file replaced
/// it. On macOS, the path of the binary.
pub fn running_binary() -> PathBuf {
    os::running_binary()
}

/// The environment of the process `pid`: each `NAME=VALUE` with a NUL
/// after it. `None` when riff cannot read it.
pub fn environ(pid: u32) -> Option<Vec<u8>> {
    os::environ(pid)
}

/// The working folder of the process `pid`, or `None` when riff cannot
/// read it.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    os::cwd(pid)
}

/// The files and folders that the process `pid` holds open. Empty when
/// riff cannot read them.
pub fn open_files(pid: u32) -> Vec<PathBuf> {
    os::open_files(pid)
}

/// The memory of this machine, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// All of the memory.
    pub total: u64,
    /// The memory that a new process can take now, without a swap.
    pub available: u64,
}

/// The memory of this machine, or `None` when riff cannot read it.
pub fn memory() -> Option<Memory> {
    os::memory()
}

/// The numbers of this machine for the placement of workers, with
/// `threads` hardware threads ([`crate::machine::here`]). A number that
/// riff cannot read gets the safe value of [`crate::machine::here`].
pub fn machine(threads: u16) -> Machine {
    os::machine(threads)
}

/// The cgroup controllers that podman needs, for the checks of
/// `riff init` ([`crate::first_run`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cgroups {
    /// The text of `cgroup.controllers` of the root of cgroup v2.
    pub root: Option<String>,
    /// The text of `cgroup.controllers` of the user unit `user@UID.service`.
    pub user: Option<String>,
}

/// The cgroup controllers of the user `uid`. On Linux, the variable
/// [`crate::first_run::CGROUP_VAR`] names a folder that riff reads in
/// place of `/sys/fs/cgroup`. macOS has no cgroups: none.
pub fn cgroups(uid: u32) -> Cgroups {
    os::cgroups(uid)
}

/// The most bytes of the path of a Unix socket on macOS: its address has
/// room for 104 bytes, with a NUL at the end (01M6Z49RD4F6EX4K9FF2Z0CET9).
/// On Linux, a path through a descriptor of the folder is short.
pub const SOCKET_MOST: usize = 103;

/// `path` when it has at most `most` bytes. Else an error that names the
/// path, its bytes and the limit (01M6Z49RD4F6EX4K9FF2Z0CET9).
///
/// ```
/// use riff::platform::{SOCKET_MOST, socket_fits};
/// use std::path::PathBuf;
///
/// let home = "/Users/mike";
/// let socket = PathBuf::from(format!("{home}/.local/share/riff/daemon/riff.sock"));
/// assert_eq!(socket_fits(socket.clone(), SOCKET_MOST)?, socket);
/// let long = PathBuf::from(format!("/{}/riff.sock", "d".repeat(93)));
/// assert_eq!(long.as_os_str().len(), 104);
/// let e = socket_fits(long, SOCKET_MOST).unwrap_err();
/// assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
/// assert!(e.to_string().contains(" has 104 bytes, and macOS takes at most 103."), "{e}");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn socket_fits(path: PathBuf, most: usize) -> io::Result<PathBuf> {
    let bytes = path.as_os_str().len();
    if bytes > most {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            crate::text::socket_too_long(&path, bytes, most),
        ));
    }
    Ok(path)
}

/// A path of the socket `name` in the folder `dir` that fits in the
/// address of a Unix socket. On Linux, it is a path through a descriptor
/// of the folder, `/proc/self/fd/N/NAME`, and the folder stays open as
/// long as this value lives. On macOS, it is `dir/name`, of at most
/// [`SOCKET_MOST`] bytes ([`socket_fits`]).
///
/// ```
/// use std::os::unix::net::{UnixListener, UnixStream};
/// use riff::platform::Socket;
///
/// let dir = tempfile::tempdir()?;
/// let listener = UnixListener::bind(Socket::in_folder(dir.path(), "a.sock")?.path())?;
/// let _conn = UnixStream::connect(Socket::in_folder(dir.path(), "a.sock")?.path())?;
/// assert!(listener.accept().is_ok());
/// assert!(dir.path().join("a.sock").exists());
/// assert!(Socket::in_folder(&dir.path().join("gone"), "a.sock").is_err());
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
pub struct Socket {
    path: PathBuf,
    /// The folder, open while the path names it.
    _folder: std::fs::File,
}

impl Socket {
    /// The path of the socket `name` in the folder `dir`. An error when
    /// riff cannot open the folder, or the path does not fit.
    pub fn in_folder(dir: &Path, name: &str) -> io::Result<Socket> {
        let folder = std::fs::File::open(dir)?;
        let path = os::socket_path(&folder, dir, name)?;
        Ok(Socket {
            path,
            _folder: folder,
        })
    }

    /// The path to bind or to connect to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The service manager of the daemon on this machine
/// (01M4Z60B7GTV4HHF9R3TMZRS7J): systemd on Linux, launchd on macOS
/// (01M6Z49H6CHSPV0FJZWY2MYHEM). An error with no home.
pub fn service_manager() -> anyhow::Result<Manager> {
    os::service_manager()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    /// A `sleep` child that ends when it drops.
    struct Sleep(Child);

    impl Sleep {
        fn start(program: &Path, dir: &Path) -> Sleep {
            let child = Command::new(program)
                .arg("30")
                .current_dir(dir)
                .env("RIFF_PLATFORM_TEST", "yes")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            Sleep(child)
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for Sleep {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// The path of `sleep` on `PATH`.
    fn sleep_program() -> PathBuf {
        crate::binary::on_path("sleep").expect("sleep on PATH")
    }

    /// Waits up to 10 seconds for `ready`.
    fn wait_for(mut ready: impl FnMut() -> bool) {
        let span = isolated::Span::start();
        while !ready() {
            assert!(span.within(Duration::from_secs(10)), "not in time");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM: this process, as the OS shows it.
    #[test]
    fn this_process_shows_its_parent_user_arguments_and_binary() {
        let me = process(std::process::id()).unwrap();
        assert_eq!(me.ppid, std::os::unix::process::parent_id());
        assert_eq!(me.uid, nix::unistd::getuid().as_raw());
        let args: Vec<String> = std::env::args().collect();
        assert_eq!(me.args, args);
        let exe = me.exe.clone().unwrap();
        assert!(crate::binary::same_file(
            &exe,
            &std::env::current_exe().unwrap()
        ));
        assert!(!me.replaced && !own_replaced());
        assert!(!me.in_container);
        assert!(running_binary().exists());
        assert!(processes().iter().any(|p| p.pid == me.pid));
        assert!(processes().iter().all(|p| p.uid == me.uid));
        assert_eq!(process(me.pid).unwrap().start, me.start);
        assert!(process(me.ppid).unwrap().start <= me.start);
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM: a child, its environment, its working
    /// folder and its open files.
    #[test]
    fn a_child_shows_its_environment_folder_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let sleep = Sleep::start(&sleep_program(), dir.path());
        wait_for(|| process(sleep.pid()).is_some_and(|p| p.args.get(1).is_some_and(|a| a == "30")));
        let child = process(sleep.pid()).unwrap();
        assert_eq!(child.ppid, std::process::id());
        assert_eq!(child.args[1], "30");
        assert!(children(std::process::id()).contains(&sleep.pid()));
        let env = environ(sleep.pid()).unwrap();
        assert!(
            env.split(|b| *b == 0)
                .any(|var| var == b"RIFF_PLATFORM_TEST=yes"),
            "{}",
            String::from_utf8_lossy(&env)
        );
        let real = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(cwd(sleep.pid()), Some(real.clone()));
        // The own process holds this file open.
        let file = real.join("open");
        let _open = std::fs::File::create(&file).unwrap();
        assert!(open_files(std::process::id()).contains(&file));
        assert_eq!(process(u32::MAX), None);
        assert_eq!(environ(u32::MAX), None);
        assert_eq!(cwd(u32::MAX), None);
        assert!(open_files(u32::MAX).is_empty());
        assert!(children(u32::MAX).is_empty());
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM: a process whose binary a new file
    /// replaced, as `riff update` does, runs a replaced binary. A child
    /// `cp` writes each program, so that no child of another test holds
    /// a write handle of it ([`isolated::copy_program`]).
    #[test]
    fn a_new_file_in_place_of_the_binary_of_a_process_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let program = isolated::copy_program(sleep_program(), dir.path().join("sleep"));
        let sleep = Sleep::start(&program, dir.path());
        wait_for(|| process(sleep.pid()).is_some_and(|p| p.args.get(1).is_some_and(|a| a == "30")));
        let old = process(sleep.pid()).unwrap();
        assert!(!old.replaced, "{old:?}");
        let real = std::fs::canonicalize(&program).unwrap();
        assert_eq!(old.exe.as_deref(), Some(real.as_path()));
        // A new file, as `riff update` writes it. A file time has a
        // resolution of up to one second.
        std::thread::sleep(Duration::from_millis(1100));
        let new = isolated::copy_program(sleep_program(), dir.path().join("new"));
        std::fs::rename(&new, &program).unwrap();
        let now = process(sleep.pid()).unwrap();
        assert!(now.replaced, "{now:?}");
        assert_eq!(now.exe.as_deref(), Some(real.as_path()));
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM: the numbers of this machine are sane.
    #[test]
    fn the_memory_and_the_machine_are_sane_numbers() {
        let memory = memory().unwrap();
        assert!(memory.total > 0 && memory.available > 0, "{memory:?}");
        assert!(memory.available <= memory.total, "{memory:?}");
        let m = machine(4);
        assert_eq!(m.threads, 4);
        assert!(m.mhz > 0 && m.now_mhz > 0, "{m:?}");
        assert!(m.mem_gb > 0 && m.avail_gb <= m.mem_gb, "{m:?}");
        assert!(m.load >= 0.0, "{m:?}");
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM, 01M6Z49H6CHSPV0FJZWY2MYHEM: macOS has
    /// no cgroups, and its service manager is launchd, with the agents of
    /// the person.
    #[test]
    fn the_cgroups_and_the_service_manager_are_of_the_os() {
        use crate::service::ServiceManager;
        let uid = nix::unistd::getuid().as_raw();
        let manager = service_manager().unwrap();
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        if cfg!(target_os = "macos") {
            assert_eq!(cgroups(uid), Cgroups::default());
            assert_eq!(manager.unit_name(), launchd::LABEL);
            assert_eq!(
                manager.unit_path(),
                home.join("Library/LaunchAgents")
                    .join(format!("{}.plist", launchd::LABEL))
            );
        } else {
            assert_eq!(manager.unit_name(), crate::service::UNIT);
            assert!(manager.unit_path().ends_with("systemd/user/riff.service"));
        }
    }

    /// 01M6Z49RD4F6EX4K9FF2Z0CET9: on macOS, the path of a socket in a long
    /// folder fails with the limit, and a short path binds. On Linux, a
    /// path through the descriptor of the folder binds in a long folder.
    #[test]
    fn a_socket_in_a_long_folder_binds_on_linux_and_fails_on_macos() {
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().unwrap();
        let long = root.path().join("d".repeat(120));
        std::fs::create_dir(&long).unwrap();
        let socket = Socket::in_folder(&long, "riff.sock");
        if cfg!(target_os = "macos") {
            let e = socket.unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
            assert!(
                e.to_string().contains(", and macOS takes at most 103."),
                "{e}"
            );
            assert!(!long.join("riff.sock").exists());
        } else {
            let _listener = UnixListener::bind(socket.unwrap().path()).unwrap();
            assert!(long.join("riff.sock").exists());
        }
        let short = Socket::in_folder(root.path(), "a.sock").unwrap();
        let _listener = UnixListener::bind(short.path()).unwrap();
        assert!(root.path().join("a.sock").exists());
    }

    /// The words that read `/proc` or `/sys`, or run `systemctl`.
    const READS: [&str; 3] = ["\"/proc", "\"/sys", "\"systemctl\""];

    /// The lines of `text` before its module `tests`, but the comments.
    fn code(text: &str) -> impl Iterator<Item = (usize, &str)> {
        let lines: Vec<&str> = text.lines().collect();
        let end = lines
            .windows(2)
            .position(|w| {
                w[0].trim() == "#[cfg(test)]" && w[1].trim_start().starts_with("mod tests")
            })
            .unwrap_or(lines.len());
        lines
            .into_iter()
            .enumerate()
            .take(end)
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
    }

    /// 01M6YQDWT387EV7Y7XMT2G6YVM: no module but this one reads `/proc` or
    /// `/sys`, or runs `systemctl`, up to its tests.
    #[test]
    fn only_the_platform_module_reads_proc_and_sys_and_runs_systemctl() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut dirs = vec![src.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path != src.join("platform") {
                        dirs.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in code(&text) {
                    if READS.iter().any(|read| line.contains(read)) {
                        found.push(format!("{}:{}: {line}", path.display(), n + 1));
                    }
                }
            }
        }
        assert!(found.is_empty(), "{}", found.join("\n"));
        // The check sees a read.
        let read = "fn a() {\n    // \"/proc\"\n    std::fs::read(\"/proc/meminfo\");\n}\n#[cfg(test)]\nmod tests {\nb(\"/sys\")\n";
        let lines: Vec<usize> = code(read)
            .filter(|(_, l)| READS.iter().any(|r| l.contains(r)))
            .map(|(n, _)| n)
            .collect();
        assert_eq!(lines, [2]);
    }
}
