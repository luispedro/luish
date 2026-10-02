//! The call stack: a frame for the script, for each file read with `.` (and
//! each startup file), and for each function call. It gives bash's
//! `BASH_SOURCE`, `FUNCNAME` and `BASH_LINENO`, the `caller` built-in, and
//! the stack that error messages show.

use std::rc::Rc;

use crate::shell::Shell;
use crate::vars::Special;

#[derive(Debug, Clone)]
pub enum FrameKind {
    /// The script the shell runs.
    Script,
    /// A file read with `.` or `source`, or a startup file.
    Source,
    /// A function call, with the function's name.
    Function(Rc<[u8]>),
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub kind: FrameKind,
    /// The file the code is in: `None` outside a file (`-c`, standard input,
    /// the command line).
    pub file: Option<Rc<[u8]>>,
    /// Whether line numbers in it are lines of `file`. Not for a function
    /// restored from the startup cache or a saved state, whose text was
    /// written anew.
    pub lines_in_file: bool,
    /// The line it was called from, in the frame below (0 for the script
    /// and the startup files).
    pub call_line: u32,
}

impl Frame {
    /// Its name in `FUNCNAME`, as in bash.
    fn name(&self) -> &[u8] {
        match &self.kind {
            FrameKind::Script => b"main",
            FrameKind::Source => b"source",
            FrameKind::Function(n) => n,
        }
    }
}

/// Error messages show at most this many lines of the stack (with the middle
/// left out).
const MAX_STACK_LINES: usize = 20;

impl Shell {
    /// The file of the code running, and whether its line numbers are the
    /// file's, for a function defined now.
    pub fn current_file(&self) -> (Option<Rc<[u8]>>, bool) {
        match self.frames.last() {
            Some(f) => (f.file.clone(), f.lines_in_file),
            None => (None, true),
        }
    }

    /// The elements of `BASH_SOURCE`, `FUNCNAME` or `BASH_LINENO`, innermost
    /// first, or `None` where bash has them unset: all three without a
    /// frame (in `-c` and on standard input), and `FUNCNAME` while no
    /// function runs.
    pub fn stack_elements(&self, s: Special) -> Option<Vec<Vec<u8>>> {
        if self.frames.is_empty() {
            return None;
        }
        let frames = self.frames.iter().rev();
        Some(match s {
            Special::BashSource => frames.map(|f| f.file.as_deref().unwrap_or_default().to_vec()).collect(),
            Special::Funcname => {
                if !(self.frames.iter()).any(|f| matches!(f.kind, FrameKind::Function(_))) {
                    return None;
                }
                frames.map(|f| f.name().to_vec()).collect()
            }
            _ => frames.map(|f| f.call_line.to_string().into_bytes()).collect(),
        })
    }

    /// Where an error happened, for the start of its message: the file of
    /// the code running (the script, a file read with `.`, or the file a
    /// function was defined in), else `$0`, and the line, if it is known.
    pub fn error_location(&self) -> (&[u8], Option<u32>) {
        let line = (!self.interactive || self.lineno > 0).then_some(self.lineno);
        match self.frames.last() {
            Some(Frame {
                file: Some(file),
                lines_in_file,
                ..
            }) => (file, line.filter(|_| *lines_in_file)),
            _ => (&self.arg0, line),
        }
    }

    /// The call stack for an error message: a line for each function call
    /// and each file read with `.` that led to the code running, innermost
    /// first, with where it was called. Empty at the top level of a script.
    pub fn stack_trace(&self) -> Vec<u8> {
        let mut lines: Vec<(Vec<u8>, usize)> = Vec::new();
        for (i, f) in self.frames.iter().enumerate().rev() {
            let below = i.checked_sub(1).map(|j| &self.frames[j]);
            let (mut line, verb) = match &f.kind {
                FrameKind::Script => continue,
                // A startup file.
                FrameKind::Source if below.is_none() && f.call_line == 0 => continue,
                FrameKind::Source => ([b"  in ", f.file.as_deref().unwrap_or_default()].concat(), "sourced"),
                FrameKind::Function(name) => ([b"  in function ", &name[..]].concat(), "called"),
            };
            let at = match below {
                Some(Frame {
                    file: Some(file),
                    lines_in_file,
                    ..
                }) => {
                    let mut at = file.to_vec();
                    if *lines_in_file {
                        at.extend_from_slice(format!(":{}", f.call_line).as_bytes());
                    }
                    Some(at)
                }
                _ if self.interactive || f.call_line == 0 => None,
                _ => Some(format!("line {}", f.call_line).into_bytes()),
            };
            if let Some(at) = at {
                line.extend_from_slice(format!(", {verb} at ").as_bytes());
                line.extend(at);
            }
            // Recursion repeats the same line.
            match lines.last_mut() {
                Some((last, n)) if *last == line => *n += 1,
                _ => lines.push((line, 1)),
            }
        }
        let mut out = Vec::new();
        let n = lines.len();
        for (i, (line, times)) in lines.into_iter().enumerate() {
            if n > MAX_STACK_LINES && (MAX_STACK_LINES / 2..n - MAX_STACK_LINES / 2).contains(&i) {
                if i == MAX_STACK_LINES / 2 {
                    out.extend_from_slice(format!("  ... ({} more)\n", n - MAX_STACK_LINES).as_bytes());
                }
                continue;
            }
            out.extend(line);
            if times > 1 {
                out.extend_from_slice(format!(" ({times} times)").as_bytes());
            }
            out.push(b'\n');
        }
        out
    }

    /// `caller [N]`, as bash's: without `N`, the line the current function
    /// (or file read with `.`) was called from and the file of the caller;
    /// with `N`, that of frame `N` (0 is the innermost), with the caller's
    /// name between them. Status 1 without such a frame.
    pub fn caller(&self, n: Option<usize>) -> Option<Vec<u8>> {
        let len = self.frames.len();
        let frame = &self.frames[len.checked_sub(1 + n.unwrap_or(0))?];
        let caller = len.checked_sub(2 + n.unwrap_or(0)).map(|i| &self.frames[i]);
        let mut out = frame.call_line.to_string().into_bytes();
        out.push(b' ');
        match (n, caller) {
            (None, None) => out.extend_from_slice(b"NULL"),
            (None, Some(c)) => out.extend_from_slice(c.file.as_deref().unwrap_or_default()),
            (Some(_), None) => return None,
            (Some(_), Some(c)) => {
                out.extend_from_slice(c.name());
                out.push(b' ');
                out.extend_from_slice(c.file.as_deref().unwrap_or_default());
            }
        }
        out.push(b'\n');
        Some(out)
    }
}
