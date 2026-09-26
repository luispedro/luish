//! Input sources: a whole string or file, a file descriptor read a line at
//! a time, or the interactive line editor.

use crate::interactive;
use crate::shell::Shell;
use crate::sys;

pub enum Input {
    /// The whole program text: of `-c` (when the flag is true, which lets
    /// the last command replace the shell) or of a script file.
    Whole(Option<Vec<u8>>, bool),
    /// Read lines from a file descriptor (stdin), never reading past the
    /// end of the current line so that commands can read the rest.
    Fd { fd: i32, seekable: bool, prompt: bool },
    /// The interactive line editor.
    Editor,
}

pub enum Line {
    Text(Vec<u8>),
    /// The user pressed Ctrl-C.
    Interrupted,
    Eof,
}

impl Input {
    pub fn fd(fd: i32, prompt: bool) -> Input {
        let seekable = sys::lseek(fd, 0, libc::SEEK_CUR).is_ok();
        Input::Fd { fd, seekable, prompt }
    }

    pub fn whole_text(&mut self) -> Option<(Vec<u8>, bool)> {
        match self {
            Input::Whole(t, command) => Some((t.take().unwrap_or_default(), *command)),
            _ => None,
        }
    }

    /// Reads the next line. `pending` is the text read so far of an
    /// incomplete command.
    pub fn read_line(&mut self, sh: &mut Shell, continuation: bool, pending: &[u8]) -> Line {
        match self {
            Input::Whole(..) => Line::Eof,
            Input::Fd { fd, seekable, prompt } => {
                if *prompt {
                    let p = interactive::prompt(sh, continuation);
                    sys::write_all(2, &p.text);
                }
                match read_fd_line(*fd, *seekable) {
                    Some(l) => Line::Text(l),
                    None => Line::Eof,
                }
            }
            Input::Editor => interactive::read_line(sh, continuation, pending),
        }
    }

    pub fn add_history(&mut self, text: &[u8]) {
        if let Input::Editor = self {
            interactive::add_history(text);
        }
    }
}

/// Reads up to and including the next newline.
fn read_fd_line(fd: i32, seekable: bool) -> Option<Vec<u8>> {
    if seekable {
        let mut buf = vec![0u8; 4096];
        let n = sys::read(fd, &mut buf, false).ok()?;
        if n == 0 {
            return None;
        }
        buf.truncate(n);
        if let Some(i) = buf.iter().position(|&c| c == b'\n') {
            let extra = n - (i + 1);
            if extra > 0 {
                let _ = sys::lseek(fd, -(extra as i64), libc::SEEK_CUR);
            }
            buf.truncate(i + 1);
        }
        return Some(buf);
    }
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match sys::read(fd, &mut b, false) {
            Ok(1) => {
                line.push(b[0]);
                if b[0] == b'\n' {
                    return Some(line);
                }
            }
            _ => return if line.is_empty() { None } else { Some(line) },
        }
    }
}
