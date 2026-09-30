//! The `fs` module: enough file access for common prompt work, such as
//! finding `.git` or checking whether a cache is stale, without forking.
//!
//! Paths are shell bytes (`to_shell`); relative ones are relative to the
//! current directory. Questions about files that can't be answered (a file
//! that doesn't exist) give `false` or `()` rather than an error.

use rhai::{Array, Dynamic, Module};

use super::bytes::to_str;
use super::rhai::{RhaiResult, to_shell, with_shell};
use crate::sys;

fn file_type(st: &libc::stat) -> libc::mode_t {
    st.st_mode & libc::S_IFMT
}

fn is_type(path: &str, t: libc::mode_t) -> RhaiResult<bool> {
    Ok(sys::stat(&to_shell(path)?).is_some_and(|st| file_type(&st) == t))
}

fn kind_name(t: libc::mode_t) -> &'static str {
    match t {
        libc::S_IFREG => "file",
        libc::S_IFDIR => "dir",
        libc::S_IFLNK => "link",
        libc::S_IFIFO => "fifo",
        libc::S_IFSOCK => "socket",
        libc::S_IFBLK => "block",
        libc::S_IFCHR => "char",
        _ => "unknown",
    }
}

fn mtime(st: &libc::stat) -> (i64, i64) {
    (st.st_mtime, st.st_mtime_nsec)
}

/// Whether `a` was modified after `b`, or exists while `b` doesn't (as
/// bash's and make's "newer", so that a missing target is out of date).
fn newer(a: &[u8], b: &[u8]) -> bool {
    match (sys::stat(a), sys::stat(b)) {
        (Some(x), Some(y)) => mtime(&x) > mtime(&y),
        (Some(_), None) => true,
        _ => false,
    }
}

/// Reads a whole file, or `None` if it can't be read.
pub fn read(path: &[u8]) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read(std::ffi::OsStr::from_bytes(path)).ok()
}

/// The directory to start from: `dir`, or the current directory, made
/// absolute.
pub fn start_dir(dir: Option<&str>) -> RhaiResult<Vec<u8>> {
    let dir = dir.map(to_shell).transpose()?;
    if let Some(d) = &dir
        && d.first() == Some(&b'/')
    {
        return Ok(crate::builtins::cd::canonicalize(d));
    }
    let cwd = with_shell(|sh| Ok(sh.curdir.clone().or_else(sys::getcwd).unwrap_or_else(|| b"/".to_vec())))?;
    Ok(match dir {
        Some(d) => crate::builtins::cd::canonicalize(&[cwd.as_slice(), b"/", d.as_slice()].concat()),
        None => cwd,
    })
}

/// The parent of an absolute, canonical directory, or `None` for `/`.
pub fn parent(dir: &[u8]) -> Option<&[u8]> {
    if dir == b"/" {
        return None;
    }
    let i = dir.iter().rposition(|&c| c == b'/')?;
    Some(if i == 0 { b"/" } else { &dir[..i] })
}

/// `dir/name`, without doubling the slash of `/`.
pub fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut p = dir.to_vec();
    if p.last() != Some(&b'/') {
        p.push(b'/');
    }
    p.extend_from_slice(name);
    p
}

/// The nearest `name` in `dir` or one of its parents.
fn find_up(name: &str, dir: Option<&str>) -> RhaiResult<Dynamic> {
    let name = to_shell(name)?;
    let mut dir = start_dir(dir)?;
    loop {
        let p = join(&dir, &name);
        if sys::lstat(&p).is_some() {
            return Ok(to_str(&p).into());
        }
        match parent(&dir) {
            Some(d) => dir = d.to_vec(),
            None => return Ok(Dynamic::UNIT),
        }
    }
}

pub fn module() -> Module {
    let mut m = Module::new();
    m.set_native_fn("exists", |p: &str| Ok(sys::stat(&to_shell(p)?).is_some()));
    m.set_native_fn("is_file", |p: &str| is_type(p, libc::S_IFREG));
    m.set_native_fn("is_dir", |p: &str| is_type(p, libc::S_IFDIR));
    m.set_native_fn("is_link", |p: &str| {
        Ok(sys::lstat(&to_shell(p)?).is_some_and(|st| file_type(&st) == libc::S_IFLNK))
    });
    m.set_native_fn("kind", |p: &str| {
        Ok(sys::lstat(&to_shell(p)?).map_or(Dynamic::UNIT, |st| kind_name(file_type(&st)).into()))
    });
    m.set_native_fn("is_readable", |p: &str| Ok(sys::access(&to_shell(p)?, libc::R_OK)));
    m.set_native_fn("is_writable", |p: &str| Ok(sys::access(&to_shell(p)?, libc::W_OK)));
    m.set_native_fn("is_executable", |p: &str| Ok(sys::access(&to_shell(p)?, libc::X_OK)));
    m.set_native_fn("size", |p: &str| {
        Ok(sys::stat(&to_shell(p)?).map_or(Dynamic::UNIT, |st| st.st_size.into()))
    });
    m.set_native_fn("mtime", |p: &str| {
        Ok(sys::stat(&to_shell(p)?).map_or(Dynamic::UNIT, |st| st.st_mtime.into()))
    });
    m.set_native_fn("newer", |a: &str, b: &str| Ok(newer(&to_shell(a)?, &to_shell(b)?)));
    m.set_native_fn("older", |a: &str, b: &str| Ok(newer(&to_shell(b)?, &to_shell(a)?)));
    m.set_native_fn("read_file", |p: &str| {
        Ok(read(&to_shell(p)?).map_or(Dynamic::UNIT, |t| to_str(&t).into()))
    });
    m.set_native_fn("list_dir", |p: &str| {
        let p = to_shell(p)?;
        // `read_dir` takes "" as the current directory.
        let Some(mut names) = sys::read_dir(&p).filter(|_| !p.is_empty()) else {
            return Ok(Dynamic::UNIT);
        };
        names.retain(|n| n != b"." && n != b"..");
        names.sort();
        Ok(names.iter().map(|n| Dynamic::from(to_str(n))).collect::<Array>().into())
    });
    m.set_native_fn("readlink", |p: &str| {
        use std::os::unix::ffi::OsStrExt;
        let p = to_shell(p)?;
        Ok(std::fs::read_link(std::ffi::OsStr::from_bytes(&p))
            .map_or(Dynamic::UNIT, |t| to_str(t.as_os_str().as_bytes()).into()))
    });
    m.set_native_fn("realpath", |p: &str| {
        use std::os::unix::ffi::OsStrExt;
        let p = to_shell(p)?;
        Ok(std::fs::canonicalize(std::ffi::OsStr::from_bytes(&p))
            .map_or(Dynamic::UNIT, |t| to_str(t.as_os_str().as_bytes()).into()))
    });
    m.set_native_fn("find_up", |name: &str| find_up(name, None));
    m.set_native_fn("find_up", |name: &str, dir: &str| find_up(name, Some(dir)));
    m
}
