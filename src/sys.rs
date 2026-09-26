//! Thin wrappers around the system calls the shell uses. Everything here
//! works on byte strings and raw fds.

use std::ffi::{CStr, CString};

pub fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

pub fn strerror(e: i32) -> String {
    // SAFETY: strerror returns a pointer to a static string.
    unsafe { CStr::from_ptr(libc::strerror(e)).to_string_lossy().into_owned() }
}

pub fn cstr(s: &[u8]) -> CString {
    CString::new(s.iter().copied().filter(|&c| c != 0).collect::<Vec<u8>>()).unwrap()
}

/// Writes all of `data`, retrying on EINTR. Returns false on error.
pub fn write_all(fd: i32, mut data: &[u8]) -> bool {
    while !data.is_empty() {
        // SAFETY: writing from a valid buffer.
        let n = unsafe { libc::write(fd, data.as_ptr() as *const _, data.len()) };
        if n < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            return false;
        }
        data = &data[n as usize..];
    }
    true
}

/// `read(2)`, retrying on EINTR unless `interruptible`.
pub fn read(fd: i32, buf: &mut [u8], interruptible: bool) -> Result<usize, i32> {
    loop {
        // SAFETY: reading into a valid buffer.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n >= 0 {
            return Ok(n as usize);
        }
        let e = errno();
        if e != libc::EINTR || interruptible {
            return Err(e);
        }
    }
}

pub fn exit(status: i32) -> ! {
    // SAFETY: terminating the process.
    unsafe { libc::_exit(status & 0xff) }
}

pub fn getppid() -> i32 {
    // SAFETY: always succeeds.
    unsafe { libc::getppid() }
}

pub fn geteuid() -> u32 {
    // SAFETY: always succeeds.
    unsafe { libc::geteuid() }
}

pub fn getegid() -> u32 {
    // SAFETY: always succeeds.
    unsafe { libc::getegid() }
}

pub fn getcwd() -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    std::env::current_dir().ok().map(|p| p.as_os_str().as_bytes().to_vec())
}

pub fn stat(path: &[u8]) -> Option<libc::stat> {
    let c = cstr(path);
    // SAFETY: valid path and output buffer.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        (libc::stat(c.as_ptr(), &mut st) == 0).then_some(st)
    }
}

pub fn lstat(path: &[u8]) -> Option<libc::stat> {
    let c = cstr(path);
    // SAFETY: valid path and output buffer.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        (libc::lstat(c.as_ptr(), &mut st) == 0).then_some(st)
    }
}

pub fn same_file(a: &[u8], b: &[u8]) -> bool {
    match (stat(a), stat(b)) {
        (Some(x), Some(y)) => x.st_dev == y.st_dev && x.st_ino == y.st_ino,
        _ => false,
    }
}

pub fn is_dir(path: &[u8]) -> bool {
    stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR)
}

pub fn access(path: &[u8], mode: i32) -> bool {
    let c = cstr(path);
    // SAFETY: valid path.
    unsafe { libc::access(c.as_ptr(), mode) == 0 }
}

/// `open(2)`; returns the fd or errno.
pub fn open(path: &[u8], flags: i32, mode: u32) -> Result<i32, i32> {
    let c = cstr(path);
    loop {
        // SAFETY: valid path.
        let fd = unsafe { libc::open(c.as_ptr(), flags | libc::O_CLOEXEC, mode) };
        if fd >= 0 {
            return Ok(fd);
        }
        let e = errno();
        if e != libc::EINTR {
            return Err(e);
        }
    }
}

/// Creates a file named `prefix` followed by six random characters, with
/// mode 0600 and close-on-exec (`mkostemp`). Returns the fd and the path.
pub fn mkstemp(prefix: &[u8]) -> Result<(i32, Vec<u8>), i32> {
    let mut template = prefix.to_vec();
    template.extend_from_slice(b"XXXXXX\0");
    // SAFETY: template is a writable, NUL-terminated buffer.
    let fd = unsafe { libc::mkostemp(template.as_mut_ptr() as *mut libc::c_char, libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(errno());
    }
    template.pop();
    Ok((fd, template))
}

pub fn unlink(path: &[u8]) {
    let c = cstr(path);
    // SAFETY: valid path.
    unsafe {
        libc::unlink(c.as_ptr());
    }
}

pub fn close(fd: i32) {
    // SAFETY: closing an fd we own.
    unsafe {
        libc::close(fd);
    }
}

pub fn dup2(from: i32, to: i32) -> Result<(), i32> {
    // SAFETY: plain dup2.
    if unsafe { libc::dup2(from, to) } < 0 {
        Err(errno())
    } else {
        Ok(())
    }
}

/// Duplicates `fd` to a close-on-exec fd numbered 10 or above.
pub fn dup_high(fd: i32) -> Result<i32, i32> {
    // SAFETY: plain fcntl.
    let r = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    if r < 0 { Err(errno()) } else { Ok(r) }
}

pub fn fd_is_open(fd: i32) -> bool {
    // SAFETY: plain fcntl.
    unsafe { libc::fcntl(fd, libc::F_GETFD) >= 0 }
}

/// A close-on-exec pipe: (read end, write end).
pub fn pipe() -> Result<(i32, i32), i32> {
    let mut fds = [0; 2];
    // SAFETY: valid output array.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        Err(errno())
    } else {
        Ok((fds[0], fds[1]))
    }
}

pub fn isatty(fd: i32) -> bool {
    // SAFETY: plain isatty.
    unsafe { libc::isatty(fd) == 1 }
}

pub fn fork() -> Result<i32, i32> {
    // SAFETY: the shell is single-threaded.
    let pid = unsafe { libc::fork() };
    if pid < 0 { Err(errno()) } else { Ok(pid) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitStatus {
    Exited(i32),
    Signaled(i32, bool),
    Stopped(i32),
    Continued,
}

impl WaitStatus {
    /// The value of `$?` for this status.
    pub fn code(&self) -> i32 {
        match *self {
            WaitStatus::Exited(c) => c,
            WaitStatus::Signaled(s, _) | WaitStatus::Stopped(s) => 128 + s,
            WaitStatus::Continued => 0,
        }
    }
}

/// `waitpid(2)`. `Ok(None)` means no child changed state (WNOHANG).
pub fn waitpid(pid: i32, flags: i32) -> Result<Option<(i32, WaitStatus)>, i32> {
    let mut status = 0;
    // SAFETY: valid output pointer.
    let r = unsafe { libc::waitpid(pid, &mut status, flags) };
    if r < 0 {
        return Err(errno());
    }
    if r == 0 {
        return Ok(None);
    }
    let ws = if libc::WIFEXITED(status) {
        WaitStatus::Exited(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        WaitStatus::Signaled(libc::WTERMSIG(status), libc::WCOREDUMP(status))
    } else if libc::WIFSTOPPED(status) {
        WaitStatus::Stopped(libc::WSTOPSIG(status))
    } else {
        WaitStatus::Continued
    };
    Ok(Some((r, ws)))
}

pub fn kill(pid: i32, sig: i32) -> Result<(), i32> {
    // SAFETY: plain kill.
    if unsafe { libc::kill(pid, sig) } < 0 {
        Err(errno())
    } else {
        Ok(())
    }
}

/// Null-terminated pointer arrays for `argv` and `env`. The pointers borrow
/// from the returned `CString`s and from `env`.
fn exec_arrays(
    argv: &[Vec<u8>],
    env: &[CString],
) -> (Vec<CString>, Vec<*const libc::c_char>, Vec<*const libc::c_char>) {
    let argv: Vec<CString> = argv.iter().map(|a| cstr(a)).collect();
    let mut argv_p: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    argv_p.push(std::ptr::null());
    let mut env_p: Vec<*const libc::c_char> = env.iter().map(|a| a.as_ptr()).collect();
    env_p.push(std::ptr::null());
    (argv, argv_p, env_p)
}

pub fn execve(path: &[u8], argv: &[Vec<u8>], env: &[CString]) -> i32 {
    let path = cstr(path);
    let (_argv, argv_p, env_p) = exec_arrays(argv, env);
    // SAFETY: null-terminated arrays of valid C strings.
    unsafe {
        libc::execve(path.as_ptr(), argv_p.as_ptr(), env_p.as_ptr());
    }
    errno()
}

/// Runs a program in a new process with `posix_spawn`, which glibc
/// implements with `clone(CLONE_VM | CLONE_VFORK)`: unlike `fork`, it doesn't
/// copy the page tables. The child inherits the signal mask, ignored
/// signals and file descriptors. Returns the pid, or the error of `execve`.
pub fn spawn(path: &[u8], argv: &[Vec<u8>], env: &[CString]) -> Result<i32, i32> {
    let path = cstr(path);
    let (_argv, argv_p, env_p) = exec_arrays(argv, env);
    let mut pid = 0;
    // SAFETY: null-terminated arrays of valid C strings; null attributes and
    // file actions mean the defaults.
    let r = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            argv_p.as_ptr() as *const *mut libc::c_char,
            env_p.as_ptr() as *const *mut libc::c_char,
        )
    };
    if r == 0 { Ok(pid) } else { Err(r) }
}

pub fn home_dir(user: &[u8]) -> Option<Vec<u8>> {
    let c = cstr(user);
    // SAFETY: getpwnam returns a pointer to static storage or null.
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            return None;
        }
        Some(CStr::from_ptr((*pw).pw_dir).to_bytes().to_vec())
    }
}

pub fn own_home_dir() -> Option<Vec<u8>> {
    // SAFETY: getpwuid returns a pointer to static storage or null.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        Some(CStr::from_ptr((*pw).pw_dir).to_bytes().to_vec())
    }
}

/// The name of the user with the real user id.
pub fn user_name() -> Option<Vec<u8>> {
    // SAFETY: getpwuid returns a pointer to static storage or null.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        Some(CStr::from_ptr((*pw).pw_name).to_bytes().to_vec())
    }
}

/// The host name (empty if it can't be found).
pub fn hostname() -> Vec<u8> {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname into a buffer of the given size.
    let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() - 1) };
    if r != 0 {
        return Vec::new();
    }
    buf.iter().take_while(|&&c| c != 0).copied().collect()
}

/// The name of the terminal open on `fd`.
pub fn ttyname(fd: i32) -> Option<Vec<u8>> {
    let mut buf = [0u8; 256];
    // SAFETY: ttyname_r into a buffer of the given size.
    let r = unsafe { libc::ttyname_r(fd, buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    (r == 0).then(|| buf.iter().take_while(|&&c| c != 0).copied().collect())
}

/// The current local time.
pub fn localtime() -> libc::tm {
    // SAFETY: time with a null pointer only returns the time, and
    // localtime_r fills the `tm` it is given.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    }
}

/// Formats a time with `strftime`.
pub fn strftime(fmt: &[u8], tm: &libc::tm) -> Vec<u8> {
    if fmt.is_empty() {
        return Vec::new();
    }
    let c = cstr(fmt);
    let mut buf = vec![0u8; 256];
    loop {
        // SAFETY: strftime writes at most `buf.len()` bytes into `buf`.
        let n = unsafe { libc::strftime(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), c.as_ptr(), tm) };
        // 0 means the result didn't fit, or is empty.
        if n > 0 || buf.len() >= 4096 {
            buf.truncate(n);
            return buf;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

pub fn chdir(path: &[u8]) -> Result<(), i32> {
    let c = cstr(path);
    // SAFETY: valid path.
    if unsafe { libc::chdir(c.as_ptr()) } < 0 {
        Err(errno())
    } else {
        Ok(())
    }
}

pub fn umask(mask: u32) -> u32 {
    // SAFETY: plain umask.
    unsafe { libc::umask(mask as libc::mode_t) as u32 }
}

pub fn lseek(fd: i32, off: i64, whence: i32) -> Result<i64, i32> {
    // SAFETY: plain lseek.
    let r = unsafe { libc::lseek(fd, off, whence) };
    if r < 0 { Err(errno()) } else { Ok(r) }
}

/// Lists a directory, including `.` and `..`. `None` if it can't be read.
pub fn read_dir(path: &[u8]) -> Option<Vec<Vec<u8>>> {
    let c = cstr(if path.is_empty() { b"." } else { path });
    // SAFETY: opendir/readdir/closedir on a valid path.
    unsafe {
        let d = libc::opendir(c.as_ptr());
        if d.is_null() {
            return None;
        }
        let mut out = Vec::new();
        loop {
            let e = libc::readdir(d);
            if e.is_null() {
                break;
            }
            let name = CStr::from_ptr((*e).d_name.as_ptr()).to_bytes();
            out.push(name.to_vec());
        }
        libc::closedir(d);
        Some(out)
    }
}

pub fn getpid() -> i32 {
    // SAFETY: always succeeds.
    unsafe { libc::getpid() }
}

pub fn getpgrp() -> i32 {
    // SAFETY: always succeeds.
    unsafe { libc::getpgrp() }
}

pub fn setpgid(pid: i32, pgid: i32) -> Result<(), i32> {
    // SAFETY: plain setpgid.
    if unsafe { libc::setpgid(pid, pgid) } < 0 {
        Err(errno())
    } else {
        Ok(())
    }
}

pub fn tcgetpgrp(fd: i32) -> Result<i32, i32> {
    // SAFETY: plain tcgetpgrp.
    let r = unsafe { libc::tcgetpgrp(fd) };
    if r < 0 { Err(errno()) } else { Ok(r) }
}

/// `tcsetpgrp(3)`, with all signals blocked so that it can't be interrupted
/// (as dash does).
pub fn tcsetpgrp(fd: i32, pgid: i32) -> Result<(), i32> {
    // SAFETY: blocking and restoring the signal mask around a tcsetpgrp.
    unsafe {
        let mut all: libc::sigset_t = std::mem::zeroed();
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut all);
        libc::sigprocmask(libc::SIG_SETMASK, &all, &mut old);
        let r = libc::tcsetpgrp(fd, pgid);
        let e = errno();
        libc::sigprocmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        if r < 0 { Err(e) } else { Ok(()) }
    }
}

pub fn tcgetattr(fd: i32) -> Option<libc::termios> {
    // SAFETY: valid output buffer.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        (libc::tcgetattr(fd, &mut t) == 0).then_some(t)
    }
}

pub fn tcsetattr(fd: i32, t: &libc::termios) {
    // SAFETY: valid termios.
    unsafe {
        libc::tcsetattr(fd, libc::TCSADRAIN, t);
    }
}

pub fn raise(sig: i32) {
    // SAFETY: plain raise.
    unsafe {
        libc::raise(sig);
    }
}

/// Blocks all signals and returns the previous mask.
pub fn block_signals() -> libc::sigset_t {
    // SAFETY: plain sigprocmask with valid sets.
    unsafe {
        let mut all: libc::sigset_t = std::mem::zeroed();
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut all);
        libc::sigprocmask(libc::SIG_SETMASK, &all, &mut old);
        old
    }
}

pub fn set_signal_mask(mask: &libc::sigset_t) {
    // SAFETY: plain sigprocmask with a valid set.
    unsafe {
        libc::sigprocmask(libc::SIG_SETMASK, mask, std::ptr::null_mut());
    }
}
