//! Redirections, with save/restore for commands run in the shell process.

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;

/// Fds replaced by redirections: (fd, saved copy or `None` if it was closed).
#[must_use]
#[derive(Default)]
pub struct SavedFds(Vec<(i32, Option<i32>)>);

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

fn clear_cloexec(fd: i32) {
    // SAFETY: plain fcntl.
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, 0);
    }
}

impl Shell {
    /// Applies redirections left to right. With `save`, the replaced fds are
    /// saved so that [`Shell::restore_redirs`] can undo them. On failure,
    /// everything done so far is undone and `Flow::Error(2)` is returned.
    pub fn redirect(&mut self, redirs: &[Redirect], save: bool) -> Result<SavedFds, Flow> {
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

    fn apply_redirect(&mut self, r: &Redirect, save: bool, saved: &mut SavedFds) -> Result<(), Flow> {
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
            RedirTarget::Word(w) => {
                let target = self.expand_word_str(w)?;
                self.redirect_action(r.kind, &target)?
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

    fn redirect_action(&mut self, kind: RedirKind, target: &[u8]) -> Result<Action, Flow> {
        let name = String::from_utf8_lossy(target).into_owned();
        let open = |sh: &Shell, flags: i32, create: bool| {
            sys::open(target, flags, 0o666).map(Action::Owned).map_err(|e| {
                if create {
                    sh.error(format!("cannot create {name}: {}", create_error(e)));
                } else {
                    sh.error(format!("cannot open {name}: {}", open_error(e)));
                }
                Flow::Error(2)
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
                        return Err(Flow::Error(2));
                    }
                    open(self, O_WRONLY, true)
                }
                Err(e) => {
                    self.error(format!("cannot create {name}: {}", create_error(e)));
                    Err(Flow::Error(2))
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
                        if !sys::fd_is_open(n) {
                            self.error(format!("{n}: Bad file descriptor"));
                            return Err(Flow::Error(2));
                        }
                        Ok(Action::Dup(n))
                    }
                    _ => {
                        self.error("Syntax error: Bad fd number");
                        Err(Flow::Error(2))
                    }
                }
            }
            RedirKind::HereDoc => unreachable!(),
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
