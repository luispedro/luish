//! The call stack: a frame for the script, for each file read with `.` (and
//! each startup file), and for each function call. It gives bash's
//! `BASH_SOURCE`, `FUNCNAME` and `BASH_LINENO`, the `caller` built-in, and
//! the stack that error messages show, with the text of the failing line.

use std::rc::Rc;

use crate::shell::Shell;
use crate::vars::Special;

/// A file that code was read from.
#[derive(Debug)]
pub struct SourceFile {
    /// The path as it was given (to the shell or to `.`, or as found in
    /// `PATH`), for `BASH_SOURCE` and error messages.
    pub name: Box<[u8]>,
    /// The directory a relative `name` was in, so that error messages can
    /// read the file after `cd`.
    dir: Option<Box<[u8]>>,
}

impl SourceFile {
    pub fn new(name: &[u8], curdir: Option<&[u8]>) -> Rc<SourceFile> {
        Rc::new(SourceFile {
            name: name.into(),
            dir: curdir.filter(|_| name.first() != Some(&b'/')).map(Box::from),
        })
    }

    /// The directory a relative name is in, if it was recorded.
    pub fn dir(&self) -> Option<&[u8]> {
        self.dir.as_deref()
    }

    /// Its path: `name`, in the directory it was relative to.
    pub fn path(&self) -> Vec<u8> {
        match &self.dir {
            Some(dir) => [&dir[..], b"/", &self.name].concat(),
            None => self.name.to_vec(),
        }
    }

    /// Its path as shown to users: `path` without a `./` after the directory.
    pub fn display_path(&self) -> Vec<u8> {
        match &self.dir {
            Some(dir) => [&dir[..], b"/", self.name.strip_prefix(b"./").unwrap_or(&self.name)].concat(),
            None => self.name.to_vec(),
        }
    }

    /// Its name, as a link to it if `links` and its path is known.
    fn shown(&self, links: bool) -> Vec<u8> {
        let path = self.display_path();
        if links && path.first() == Some(&b'/') {
            crate::interactive::file_link(&self.name, &path)
        } else {
            self.name.to_vec()
        }
    }
}

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
    pub file: Option<Rc<SourceFile>>,
    /// Whether line numbers in it are lines of `file` (or of the `-c`
    /// command). Not for a function restored from the startup cache or a
    /// saved state, whose text was written anew, nor for one defined by
    /// `eval`.
    pub lines_in_file: bool,
    /// The line it was called from, in the frame below (0 for the script
    /// and the startup files).
    pub call_line: u32,
    /// The strings (`eval`, traps) running in the frame below when it was
    /// called, which [`Shell::pop_frame`] restores. Its call isn't at a
    /// line of the file if there were any.
    saved_in_string: u32,
}

impl Frame {
    pub fn new(kind: FrameKind, file: Option<Rc<SourceFile>>, lines_in_file: bool, call_line: u32) -> Frame {
        Frame {
            kind,
            file,
            lines_in_file,
            call_line,
            saved_in_string: 0,
        }
    }

    /// A frame for a file: the script, or one read with `.`.
    pub fn file(kind: FrameKind, file: Rc<SourceFile>, call_line: u32) -> Frame {
        Frame::new(kind, Some(file), true, call_line)
    }

    fn file_name(&self) -> &[u8] {
        self.file.as_ref().map_or(b"", |f| &f.name)
    }

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

/// The text of the failing line is cut after this many bytes.
const MAX_LINE_TEXT: usize = 160;

/// The text of the files that an error message shows lines of, by path
/// (`None` for the `-c` command), read once per message.
type TextCache = Vec<(Option<Vec<u8>>, Option<Vec<u8>>)>;

impl Shell {
    /// Pushes a frame, in which no string (`eval`, trap) runs yet.
    pub fn push_frame(&mut self, mut frame: Frame) {
        frame.saved_in_string = std::mem::take(&mut self.in_string);
        self.frames.push(frame);
    }

    pub fn pop_frame(&mut self) {
        if let Some(f) = self.frames.pop() {
            self.in_string = f.saved_in_string;
        }
    }

    /// The file of the code running, and whether its line numbers are the
    /// file's, for a function defined now.
    pub fn current_file(&self) -> (Option<Rc<SourceFile>>, bool) {
        let (file, lines) = match self.frames.last() {
            Some(f) => (f.file.clone(), f.lines_in_file),
            None => (None, true),
        };
        (file, lines && self.in_string == 0)
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
            Special::BashSource => frames.map(|f| f.file_name().to_vec()).collect(),
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
    /// With `links`, the file's name is a link to it (OSC 8).
    pub fn error_location(&self, links: bool) -> (Vec<u8>, Option<u32>) {
        let line = (!self.interactive || self.lineno > 0).then_some(self.lineno);
        match self.frames.last() {
            Some(f) => (
                f.file.as_ref().map_or(self.arg0.to_vec(), |f| f.shown(links)),
                line.filter(|_| f.lines_in_file),
            ),
            None => (self.arg0.to_vec(), line),
        }
    }

    /// The text of the line that failed, for an error message: read again
    /// from its file (or, for `-c`, from the command line in `/proc`), so
    /// that it costs nothing until an error. `None` where the line isn't a
    /// line of a file or of the `-c` command (in `eval`, a trap, a function
    /// restored from the startup cache, or code typed at the prompt), or if
    /// the file can't be read.
    pub fn error_line_text(&self) -> Option<Vec<u8>> {
        if self.in_string > 0 {
            return None;
        }
        self.line_text(self.frames.last(), self.lineno, &mut Vec::new())
    }

    /// The text of line `lineno` of the code of `frame` (`None` for `-c`
    /// without a frame), for an error message. Files read are kept in
    /// `cache`, keyed by path (`None` for the `-c` command).
    fn line_text(&self, frame: Option<&Frame>, lineno: u32, cache: &mut TextCache) -> Option<Vec<u8>> {
        if lineno == 0 || frame.is_some_and(|f| !f.lines_in_file) {
            return None;
        }
        let key = frame.and_then(|f| f.file.as_ref()).map(|f| f.path());
        let i = match cache.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                let text = match &key {
                    Some(path) => std::fs::read(crate::interactive::to_path(path)).ok(),
                    None => self.command_arg.and_then(|i| {
                        let cmdline = std::fs::read("/proc/self/cmdline").ok()?;
                        Some(cmdline.split(|&c| c == 0).nth(i)?.to_vec())
                    }),
                };
                cache.push((key, text));
                cache.len() - 1
            }
        };
        let text = cache[i].1.as_deref()?;
        let line = text.split(|&c| c == b'\n').nth(lineno as usize - 1)?;
        let line = line.trim_ascii();
        if line.is_empty() {
            return None;
        }
        let mut out = Vec::new();
        for &c in line.iter().take(MAX_LINE_TEXT) {
            match c {
                b'\t' => out.push(b' '),
                c if c < b' ' || c == 0x7f => out.push(b'?'),
                c => out.push(c),
            }
        }
        if line.len() > MAX_LINE_TEXT {
            // Not in the middle of a UTF-8 sequence.
            while out.last().is_some_and(|&c| c & 0xc0 == 0x80) {
                out.pop();
            }
            out.pop_if(|c| *c >= 0xc0);
            out.extend_from_slice(b" ...");
        }
        Some(out)
    }

    /// The call stack for an error message: a line for each function call
    /// and each file read with `.` that led to the code running, innermost
    /// first, with where it was called and the text of that line. Empty at
    /// the top level of a script.
    pub fn stack_trace(&self, links: bool) -> Vec<u8> {
        let mut lines: Vec<(Vec<u8>, Option<Vec<u8>>, usize)> = Vec::new();
        let mut cache = TextCache::new();
        for (i, f) in self.frames.iter().enumerate().rev() {
            let below = i.checked_sub(1).map(|j| &self.frames[j]);
            let (mut line, verb) = match &f.kind {
                FrameKind::Script => continue,
                // A startup file.
                FrameKind::Source if below.is_none() && f.call_line == 0 => continue,
                FrameKind::Source => {
                    let name = f.file.as_ref().map_or(Vec::new(), |f| f.shown(links));
                    ([&b"  in "[..], &name].concat(), "sourced")
                }
                FrameKind::Function(name) => ([b"  in function ", &name[..]].concat(), "called"),
            };
            let at = match below {
                Some(Frame {
                    file: Some(file),
                    lines_in_file,
                    ..
                }) => {
                    let mut at = file.shown(links);
                    if *lines_in_file {
                        at.extend_from_slice(format!(":{}", f.call_line).as_bytes());
                    }
                    Some(at)
                }
                _ if self.interactive || f.call_line == 0 => None,
                _ => Some(format!("line {}", f.call_line).into_bytes()),
            };
            let mut text = None;
            if let Some(at) = at {
                line.extend_from_slice(format!(", {verb} at ").as_bytes());
                line.extend(at);
                if f.saved_in_string == 0 {
                    text = self.line_text(below, f.call_line, &mut cache);
                }
            }
            // Recursion repeats the same line.
            match lines.last_mut() {
                Some((last, last_text, n)) if *last == line && *last_text == text => *n += 1,
                _ => lines.push((line, text, 1)),
            }
        }
        let mut out = Vec::new();
        let n = lines.len();
        for (i, (line, text, times)) in lines.into_iter().enumerate() {
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
            if let Some(text) = text {
                out.extend_from_slice(b"      ");
                out.extend(text);
                out.push(b'\n');
            }
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
            (None, Some(c)) => out.extend_from_slice(c.file_name()),
            (Some(_), None) => return None,
            (Some(_), Some(c)) => {
                out.extend_from_slice(c.name());
                out.push(b' ');
                out.extend_from_slice(c.file_name());
            }
        }
        out.push(b'\n');
        Some(out)
    }
}
