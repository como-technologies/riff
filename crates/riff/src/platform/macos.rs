//! The reads of the OS on macOS (01M6YQDWT387EV7Y7XMT2G6YVM,
//! 01M6YS350HYR87VEVRDVA0SARX): `libproc` for the processes and `sysctl`
//! for the machine. macOS has no service manager of the daemon yet: the
//! daemon needs Linux (01M6YPB5B42FZP7GCDAVTN4PWY). See
//! [`crate::platform`].

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use libproc::bsd_info::BSDInfo;
use libproc::file_info::{ListFDs, PIDFDInfo, PIDFDInfoFlavor, ProcFDType, pidfdinfo};
use libproc::proc_pid::{PIDInfo, PidInfoFlavor, listpidinfo, pidinfo, pidpath};
use libproc::processes::{ProcFilter, pids_by_type};
use riff_client::machines::{BASE_MHZ, Machine};
use sysctl::{Ctl, CtlValue, Sysctl};

use super::{Cgroups, Memory, Process};
use crate::service::{ServiceManager, Unit};

/// The service manager of the daemon on macOS: none yet. A value of it
/// cannot exist, so no step of it runs.
#[derive(Debug)]
pub enum Manager {}

impl Manager {
    /// The path of the unit file.
    pub fn unit_path(&self) -> PathBuf {
        match *self {}
    }
}

impl ServiceManager for Manager {
    fn install(&self, _unit: &Unit) -> Result<PathBuf> {
        match *self {}
    }

    fn uninstall(&self) -> Result<Option<PathBuf>> {
        match *self {}
    }

    fn state(&self) -> Result<String> {
        match *self {}
    }

    fn installed(&self) -> Result<Option<Unit>> {
        match *self {}
    }

    fn rewrite(&self, _unit: &Unit) -> Result<PathBuf> {
        match *self {}
    }

    fn try_restart(&self) -> Result<()> {
        match *self {}
    }

    fn main_pid(&self) -> Result<Option<u32>> {
        match *self {}
    }
}

pub(super) fn service_manager() -> Result<Manager> {
    bail!("{}", crate::text::DAEMON_LINUX_ONLY)
}

/// The ID of the process in the form of `libproc`.
fn raw(pid: u32) -> Option<i32> {
    i32::try_from(pid).ok()
}

/// The text of a C string in `bytes`, up to its NUL.
fn c_text(bytes: &[std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = bytes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The time `micros` microseconds after 1970.
fn at(micros: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_micros(micros)
}

/// True when the file `exe` changed after `start`, or is gone: a new
/// file in its place, as `riff update` writes it, has a later time.
fn newer(exe: &Path, start: SystemTime) -> bool {
    std::fs::metadata(exe)
        .and_then(|meta| meta.modified())
        .map_or(true, |modified| modified > start)
}

pub(super) fn process(pid: u32) -> Option<Process> {
    let info: BSDInfo = pidinfo(raw(pid)?, 0).ok()?;
    let start = info
        .pbi_start_tvsec
        .saturating_mul(1_000_000)
        .saturating_add(info.pbi_start_tvusec);
    let secs = SystemTime::now()
        .duration_since(at(start))
        .map_or(0, |run| run.as_secs());
    let args = procargs(pid).map(|raw| parse_procargs(&raw));
    let exe = pidpath(raw(pid)?).ok().map(PathBuf::from).or_else(|| {
        let path = PathBuf::from(&args.as_ref()?.exec);
        Some(std::fs::canonicalize(&path).unwrap_or(path))
    });
    let name = match c_text(&info.pbi_name) {
        name if name.is_empty() => c_text(&info.pbi_comm),
        name => name,
    };
    Some(Process {
        pid,
        ppid: info.pbi_ppid,
        uid: info.pbi_uid,
        start,
        secs,
        name,
        args: args.map(|args| args.args).unwrap_or_default(),
        replaced: exe.as_deref().is_some_and(|exe| newer(exe, at(start))),
        exe,
        in_container: false,
    })
}

pub(super) fn processes() -> Vec<Process> {
    let uid = nix::unistd::getuid().as_raw();
    let mut all: Vec<Process> = pids_by_type(ProcFilter::ByUID { uid })
        .unwrap_or_default()
        .into_iter()
        .filter_map(process)
        .filter(|process| process.uid == uid)
        .collect();
    all.sort_by_key(|process| process.pid);
    all
}

pub(super) fn children(pid: u32) -> Vec<u32> {
    pids_by_type(ProcFilter::ByParentProcess { ppid: pid })
        .unwrap_or_default()
        .into_iter()
        .filter(|&child| child != 0)
        .collect()
}

pub(super) fn own_replaced() -> bool {
    let me = std::process::id();
    let Some(process) = process(me) else {
        return false;
    };
    let exe = process.exe.or_else(|| std::env::current_exe().ok());
    exe.is_some_and(|exe| newer(&exe, at(process.start)))
}

pub(super) fn running_binary() -> PathBuf {
    process(std::process::id())
        .and_then(|process| process.exe)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_default()
}

/// `KERN_PROCARGS2` of `<sys/sysctl.h>`: the count of the arguments, the
/// path of the binary, the arguments and the environment of a process.
const KERN_PROCARGS2: i32 = 49;

/// `CTL_KERN` of `<sys/sysctl.h>`.
const CTL_KERN: i32 = 1;

/// The raw `KERN_PROCARGS2` of the process `pid`, or `None` when riff
/// cannot read it, for example for a process of another user.
fn procargs(pid: u32) -> Option<Vec<u8>> {
    let ctl = Ctl::Oid(vec![CTL_KERN, KERN_PROCARGS2, raw(pid)?]);
    match ctl.value().ok()? {
        CtlValue::Node(bytes) | CtlValue::Struct(bytes) => Some(bytes),
        _ => None,
    }
}

/// The parts of `KERN_PROCARGS2`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcArgs {
    /// The path of the binary, as the process gave it to `exec`.
    pub exec: String,
    /// The arguments.
    pub args: Vec<String>,
    /// The environment: each `NAME=VALUE` with a NUL after it.
    pub env: Vec<u8>,
}

/// The parts of `raw`, the bytes of `KERN_PROCARGS2`: the count of the
/// arguments as a 32-bit number, the path of the binary, NULs, each
/// argument with a NUL after it, and then each variable of the
/// environment with a NUL after it, up to an empty one.
///
/// ```
/// use riff::platform::macos::{ProcArgs, parse_procargs};
///
/// let mut raw = 2i32.to_ne_bytes().to_vec();
/// raw.extend(b"/bin/sleep\0\0\0\0sleep\x0030\0HOME=/h\0A=1\0\0ptr_munge=\0");
/// let parts = ProcArgs {
///     exec: "/bin/sleep".into(),
///     args: vec!["sleep".into(), "30".into()],
///     env: b"HOME=/h\0A=1\0".to_vec(),
/// };
/// assert_eq!(parse_procargs(&raw), parts);
/// assert_eq!(parse_procargs(b"\x01"), ProcArgs::default());
/// ```
pub fn parse_procargs(raw: &[u8]) -> ProcArgs {
    let Some((count, rest)) = raw.split_first_chunk::<4>() else {
        return ProcArgs::default();
    };
    let count = usize::try_from(i32::from_ne_bytes(*count)).unwrap_or(0);
    let mut parts = rest.split(|&b| b == 0);
    let exec = String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
    let mut parts = parts.skip_while(|part| part.is_empty());
    let args = parts
        .by_ref()
        .take(count)
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    let mut env = Vec::new();
    for var in parts.take_while(|part| !part.is_empty()) {
        env.extend_from_slice(var);
        env.push(0);
    }
    ProcArgs { exec, args, env }
}

pub(super) fn environ(pid: u32) -> Option<Vec<u8>> {
    procargs(pid).map(|raw| parse_procargs(&raw).env)
}

/// The size of `struct vnode_info` of `<sys/proc_info.h>`.
const VNODE_INFO: usize = 152;

/// The size of `MAXPATHLEN`, the room of a path.
const PATH: usize = 1024;

/// `struct proc_vnodepathinfo` of `<sys/proc_info.h>`: the working
/// folder of a process, then its root folder. Each is a
/// `struct vnode_info_path`: a `struct vnode_info`, then the path.
#[repr(C, align(8))]
struct VnodePathInfo {
    _cwd_info: [u8; VNODE_INFO],
    cwd: [std::os::raw::c_char; PATH],
    _root: [u8; VNODE_INFO + PATH],
}

impl PIDInfo for VnodePathInfo {
    fn flavor() -> PidInfoFlavor {
        PidInfoFlavor::VNodePathInfo
    }
}

pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    let info: VnodePathInfo = pidinfo(raw(pid)?, 0).ok()?;
    Some(PathBuf::from(c_text(&info.cwd))).filter(|path| !path.as_os_str().is_empty())
}

/// The size of `struct proc_fileinfo` of `<sys/proc_info.h>`.
const FILE_INFO: usize = 24;

/// `struct vnode_fdinfowithpath` of `<sys/proc_info.h>`: a
/// `struct proc_fileinfo`, a `struct vnode_info`, then the path of the
/// file.
#[repr(C, align(8))]
struct FdPath {
    _info: [u8; FILE_INFO + VNODE_INFO],
    path: [std::os::raw::c_char; PATH],
}

impl Default for FdPath {
    fn default() -> FdPath {
        FdPath {
            _info: [0; FILE_INFO + VNODE_INFO],
            path: [0; PATH],
        }
    }
}

impl PIDFDInfo for FdPath {
    fn flavor() -> PIDFDInfoFlavor {
        PIDFDInfoFlavor::VNodePathInfo
    }
}

pub(super) fn open_files(pid: u32) -> Vec<PathBuf> {
    let Some(id) = raw(pid) else {
        return Vec::new();
    };
    let Ok(info) = pidinfo::<BSDInfo>(id, 0) else {
        return Vec::new();
    };
    let room = usize::try_from(info.pbi_nfiles).unwrap_or(0).max(1);
    let Ok(fds) = listpidinfo::<ListFDs>(id, room) else {
        return Vec::new();
    };
    fds.iter()
        .filter(|fd| matches!(ProcFDType::from(fd.proc_fdtype), ProcFDType::VNode))
        .filter_map(|fd| pidfdinfo::<FdPath>(id, fd.proc_fd).ok())
        .map(|file| PathBuf::from(c_text(&file.path)))
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

pub(super) fn socket_path(_folder: &File, dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// The number in the `sysctl` `name`, or `None` when this Mac has none.
fn number(name: &str) -> Option<u64> {
    match Ctl::new(name).ok()?.value().ok()? {
        CtlValue::Int(n) | CtlValue::S32(n) => u64::try_from(n).ok(),
        CtlValue::Long(n) | CtlValue::S64(n) => u64::try_from(n).ok(),
        CtlValue::Uint(n) | CtlValue::U32(n) => Some(u64::from(n)),
        CtlValue::Ulong(n) | CtlValue::U64(n) => Some(n),
        _ => None,
    }
}

pub(super) fn memory() -> Option<Memory> {
    let total = number("hw.memsize")?;
    let page = number("hw.pagesize")?;
    // The free pages, and the pages that macOS gives up at once: the
    // speculative, the purgeable and the file-backed ones.
    let pages: u64 = [
        "vm.page_free_count",
        "vm.page_speculative_count",
        "vm.page_purgeable_count",
        "vm.page_pageable_external_count",
    ]
    .into_iter()
    .filter_map(number)
    .sum();
    Some(Memory {
        total,
        available: pages.saturating_mul(page).min(total),
    })
}

/// The 1-minute load in `raw`, the bytes of `vm.loadavg`: a
/// `struct loadavg` with three 32-bit numbers and the scale, a `long`
/// at the byte 16.
///
/// ```
/// let mut raw = vec![0u8; 24];
/// raw[..4].copy_from_slice(&3072u32.to_ne_bytes());
/// raw[16..].copy_from_slice(&2048i64.to_ne_bytes());
/// assert_eq!(riff::platform::macos::load_of(&raw), Some(1.5));
/// assert_eq!(riff::platform::macos::load_of(&raw[..8]), None);
/// ```
pub fn load_of(raw: &[u8]) -> Option<f32> {
    let load = u32::from_ne_bytes(raw.get(..4)?.try_into().ok()?);
    let scale = i64::from_ne_bytes(raw.get(16..24)?.try_into().ok()?);
    (scale > 0).then(|| (f64::from(load) / scale as f64) as f32)
}

/// The GB of `bytes`.
fn gb(bytes: u64) -> u32 {
    u32::try_from(bytes / 1024 / 1024 / 1024).unwrap_or(u32::MAX)
}

pub(super) fn machine(threads: u16) -> Machine {
    let mhz = |name: &str| {
        number(name)
            .and_then(|hz| u32::try_from(hz / 1_000_000).ok())
            .filter(|&mhz| mhz > 0)
    };
    // A Mac with Apple silicon has no `hw.cpufrequency_max`.
    let max = mhz("hw.cpufrequency_max").unwrap_or(BASE_MHZ);
    let memory = memory();
    let mem_gb = memory.map_or(u32::from(threads) * 2, |m| gb(m.total));
    let load = match Ctl::new("vm.loadavg").and_then(|ctl| ctl.value()) {
        Ok(CtlValue::Struct(raw)) => load_of(&raw),
        _ => None,
    };
    Machine {
        threads,
        mhz: max,
        now_mhz: mhz("hw.cpufrequency").unwrap_or(max),
        mem_gb,
        avail_gb: memory.map_or(mem_gb, |m| gb(m.available)),
        load: load.unwrap_or(0.0),
    }
}

pub(super) fn cgroups(_uid: u32) -> Cgroups {
    Cgroups::default()
}
