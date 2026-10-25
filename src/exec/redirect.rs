//! Redirections, with save/restore for commands run in the shell process.

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;

/// Fds replaced by redirections: (fd, saved copy or `None` if it was closed).
#[must_use]
#[derive(Default)]
pub struct SavedFds(Vec<(i32, Option<i32>)>);

impl SavedFds {
    /// Where `fd` pointed before the redirections: its saved copy if it was
    /// replaced (`None` if it was closed then), otherwise `fd` itself.
    pub fn original(&self, fd: i32) -> Option<i32> {
        match self.0.iter().find(|(f, _)| *f == fd) {
            Some((_, copy)) => *copy,
            None => Some(fd),
        }
    }
}

/// Why redirections failed.
pub enum RedirError {
    /// A file couldn't be opened, or an fd duplicated: the command fails
    /// with this status (and, for a special built-in, the shell exits).
    Open(i32),
    /// An expansion error, which is fatal like any other.
    Flow(Flow),
}

impl From<Flow> for RedirError {
    fn from(f: Flow) -> Self {
        RedirError::Flow(f)
    }
}

impl From<RedirError> for Flow {
    fn from(e: RedirError) -> Self {
        match e {
            RedirError::Open(n) => Flow::Error(n),
            RedirError::Flow(f) => f,
        }
    }
}

enum Action {
    /// Move this newly opened fd into place.
    Owned(i32),
    /// Duplicate an existing fd.
    Dup(i32),
    Close,
}

pub fn open_error(e: i32) -> String {
    match e {
        libc::ENOENT | libc::ENOTDIR => "No such file".into(),
        _ => sys::strerror(e),
    }
}

pub fn create_error(e: i32) -> String {
    match e {
        libc::ENOENT | libc::ENOTDIR => "Directory nonexistent".into(),
        _ => sys::strerror(e),
    }
}

/// The file of `$(<file)` (or `$(0<file)`): a `$(...)` that is only one
/// redirection of standard input.
pub fn file_subst(list: &List) -> Option<&Word> {
    let [cc] = &list[..] else { return None };
    let pipeline = &cc.list.first;
    if cc.async_ || !cc.list.rest.is_empty() || pipeline.negated {
        return None;
    }
    let [Command::Simple(sc)] = &pipeline.cmds[..] else {
        return None;
    };
    let [r] = &sc.redirs[..] else { return None };
    match &r.target {
        RedirTarget::Word(w) if r.kind == RedirKind::In && r.fd.unwrap_or(0) == 0 => {
            (sc.words.is_empty() && sc.assigns.is_empty()).then_some(w)
        }
        _ => None,
    }
}

pub(super) fn clear_cloexec(fd: i32) {
    // SAFETY: plain fcntl.
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, 0);
    }
}

impl Shell {
    /// Applies redirections left to right. With `save`, the replaced fds are
    /// saved so that [`Shell::restore_redirs`] can undo them. On failure,
    /// everything done so far is undone.
    pub fn redirect(&mut self, redirs: &[Redirect], save: bool) -> Result<SavedFds, RedirError> {
        let mut saved = SavedFds::default();
        for r in redirs {
            if let Err(e) = self.apply_redirect(r, save, &mut saved) {
                self.restore_redirs(saved);
                return Err(e);
            }
        }
        Ok(saved)
    }

    pub fn restore_redirs(&mut self, saved: SavedFds) {
        for (fd, copy) in saved.0.into_iter().rev() {
            match copy {
                Some(c) => {
                    let _ = sys::dup2(c, fd);
                    sys::close(c);
                }
                None => sys::close(fd),
            }
        }
    }

    /// The file of a redirection. With `setopt expand.braces`, its braces
    /// are expanded, and more than one word is an error, as in bash.
    fn redirect_target(&mut self, w: &Word) -> Result<Vec<u8>, RedirError> {
        if self.opt(Opt::BraceExpand)
            && crate::expand::brace::has_brace(w)
            && let Some(words) = crate::expand::brace::expand(w, &mut |w| self.expand_word_str(w))?
        {
            if let [w] = words.as_slice() {
                return Ok(self.expand_word_str(w)?);
            }
            self.error(format!("{}: ambiguous redirect", crate::cmdtext::word(w)));
            return Err(RedirError::Open(1));
        }
        Ok(self.expand_word_str(w)?)
    }

    /// The value of `$(<file)`: the file's contents, read by the shell
    /// itself, as in ksh, bash and zsh (dash only opens it, so the value is
    /// empty). As in bash and zsh, the name is expanded in the shell, not
    /// in a subshell (`${x:=f}` assigns, and an expansion error is fatal).
    /// A file that can't be opened or read (a directory) gives status 1, as
    /// in zsh.
    #[inline(never)]
    pub fn file_subst(&mut self, w: &Word) -> Result<Vec<u8>, Flow> {
        let mut out = Vec::new();
        let status = match self.redirect_target(w) {
            Ok(name) => self.read_file(&name, &mut out)?,
            Err(RedirError::Open(n)) => n,
            Err(RedirError::Flow(f)) => return Err(f),
        };
        self.subst_status = Some(status);
        out.retain(|&b| b != 0);
        while out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }

    /// Appends the contents of the file `name` to `out`, running traps for
    /// signals that arrive meanwhile. Returns the status.
    fn read_file(&mut self, name: &[u8], out: &mut Vec<u8>) -> Result<i32, Flow> {
        let shown = String::from_utf8_lossy(name);
        let fd = match sys::open(name, libc::O_RDONLY, 0) {
            Ok(fd) => fd,
            Err(e) => {
                self.error(format!("cannot open {shown}: {}", open_error(e)));
                return Ok(1);
            }
        };
        if let Some(st) = sys::fstat(fd)
            && st.st_mode & libc::S_IFMT == libc::S_IFREG
        {
            out.reserve(st.st_size.max(0) as usize);
        }
        let mut buf = [0u8; 8192];
        let r = loop {
            // Also before the first read: the signal may have come already.
            if let Err(f) = self.run_pending_traps() {
                break Err(f);
            }
            match sys::read(fd, &mut buf, true) {
                Ok(0) => break Ok(0),
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(libc::EINTR) => {}
                Err(e) => {
                    self.error(format!("cannot read {shown}: {}", sys::strerror(e)));
                    break Ok(1);
                }
            }
        };
        sys::close(fd);
        r
    }

    fn apply_redirect(&mut self, r: &Redirect, save: bool, saved: &mut SavedFds) -> Result<(), RedirError> {
        let fd = r.fd.unwrap_or(r.kind.default_fd()) as i32;
        let action = match &r.target {
            RedirTarget::HereDoc(body) => {
                let body = body.borrow().clone();
                let text = if body.quoted {
                    body.body.as_literal().unwrap_or_default().to_vec()
                } else {
                    self.expand_word_str(&body.body)?
                };
                Action::Owned(self.heredoc_fd(&text)?)
            }
            RedirTarget::Word(w) if r.kind == RedirKind::HereString => {
                let mut text = self.expand_here_string(w)?;
                text.push(b'\n');
                Action::Owned(self.heredoc_fd(&text)?)
            }
            RedirTarget::Word(w) => {
                let target = self.redirect_target(w)?;
                self.redirect_action(r.kind, &target, fd)?
            }
        };
        if save && !saved.0.iter().any(|(f, _)| *f == fd) {
            let copy = sys::dup_high(fd).ok();
            saved.0.push((fd, copy));
        }
        match action {
            Action::Owned(new) => {
                if new != fd {
                    let _ = sys::dup2(new, fd);
                    sys::close(new);
                } else {
                    clear_cloexec(fd);
                }
            }
            Action::Dup(src) => {
                if src != fd {
                    let _ = sys::dup2(src, fd);
                }
            }
            Action::Close => sys::close(fd),
        }
        Ok(())
    }

    fn redirect_action(&mut self, kind: RedirKind, target: &[u8], fd: i32) -> Result<Action, RedirError> {
        let name = String::from_utf8_lossy(target).into_owned();
        let open = |sh: &Shell, flags: i32, create: bool| {
            sys::open(target, flags, 0o666).map(Action::Owned).map_err(|e| {
                if create {
                    sh.error(format!("cannot create {name}: {}", create_error(e)));
                } else {
                    sh.error(format!("cannot open {name}: {}", open_error(e)));
                }
                RedirError::Open(2)
            })
        };
        use libc::{O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
        match kind {
            RedirKind::In => open(self, O_RDONLY, false),
            RedirKind::Out if self.opt(Opt::Noclobber) => match sys::open(target, O_WRONLY | O_CREAT | O_EXCL, 0o666) {
                Ok(fd) => Ok(Action::Owned(fd)),
                Err(libc::EEXIST) => {
                    let regular = sys::stat(target).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG);
                    if regular {
                        self.error(format!("cannot create {name}: File exists"));
                        return Err(RedirError::Open(2));
                    }
                    open(self, O_WRONLY, true)
                }
                Err(e) => {
                    self.error(format!("cannot create {name}: {}", create_error(e)));
                    Err(RedirError::Open(2))
                }
            },
            RedirKind::Out | RedirKind::Clobber => open(self, O_WRONLY | O_CREAT | O_TRUNC, true),
            RedirKind::Append => open(self, O_WRONLY | O_CREAT | O_APPEND, true),
            RedirKind::ReadWrite => open(self, O_RDWR | O_CREAT, true),
            RedirKind::DupIn | RedirKind::DupOut => {
                if target == b"-" {
                    return Ok(Action::Close);
                }
                let n = std::str::from_utf8(target).ok().and_then(|s| s.parse::<i32>().ok());
                match n {
                    Some(n) if target.iter().all(|c| c.is_ascii_digit()) => {
                        // As in dash, `3>&3` does nothing, even if 3 is closed.
                        if n != fd && !sys::fd_is_open(n) {
                            self.error(format!("{n}: Bad file descriptor"));
                            return Err(RedirError::Open(2));
                        }
                        Ok(Action::Dup(n))
                    }
                    _ => {
                        // A syntax error in dash, so fatal.
                        self.error("Syntax error: Bad fd number");
                        Err(RedirError::Flow(Flow::Error(2)))
                    }
                }
            }
            RedirKind::HereDoc | RedirKind::HereString => unreachable!(),
        }
    }

    /// Creates a temporary file in `$TMPDIR` (or `/tmp`) whose name starts
    /// with `name`. Returns the fd (close-on-exec) and the path.
    pub fn temp_file(&self, name: &[u8]) -> Result<(i32, Vec<u8>), Flow> {
        let mut prefix = self
            .get_var(b"TMPDIR")
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| b"/tmp".to_vec());
        prefix.push(b'/');
        prefix.extend_from_slice(name);
        sys::mkstemp(&prefix).map_err(|e| {
            self.error(format!("cannot create temp file: {}", sys::strerror(e)));
            Flow::Error(2)
        })
    }

    /// An fd from which the here-doc text can be read.
    fn heredoc_fd(&mut self, text: &[u8]) -> Result<i32, Flow> {
        if text.len() <= 65536
            && let Ok((r, w)) = sys::pipe()
        {
            sys::write_all(w, text);
            sys::close(w);
            return Ok(r);
        }
        // Too big for a pipe buffer: use an unlinked temporary file.
        let (fd, path) = self.temp_file(b"luish-heredoc-")?;
        sys::unlink(&path);
        sys::write_all(fd, text);
        let _ = sys::lseek(fd, 0, libc::SEEK_SET);
        Ok(fd)
    }
}
