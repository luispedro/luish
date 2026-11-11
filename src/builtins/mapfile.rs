//! `mapfile` (and `readarray`), as in bash: the lines of the input as the
//! elements of an array.

use super::{parse_uint, single_quote};
use crate::lexer::is_valid_name;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals;
use crate::sys;
use crate::vars::{Item, MAX_INDEX, Value};

/// The array filled when none is named.
const DEFAULT: &[u8] = b"MAPFILE";

/// How much is read at a time when reading ahead is harmless.
const BLOCK: usize = 64 << 10;

struct Opts {
    /// `-d`: the byte that ends a line (NUL for `-d ''`).
    delim: u8,
    /// `-t`: the lines are stored without their delimiter.
    chop: bool,
    /// `-n`: the most lines to store (0: all).
    count: usize,
    /// `-s`: the lines to skip first.
    skip: usize,
    /// `-O`: the index of the first line. Without it the array is replaced.
    origin: Option<usize>,
    /// `-u`.
    fd: i32,
    /// `-C`, and `-c`: the lines stored between two calls.
    callback: Option<Vec<u8>>,
    quantum: usize,
}

/// The input. It is read in blocks when what is read ahead can be given
/// back (a file: `sync` seeks back to the end of the last line taken), or
/// when all of it is to be read anyway. Otherwise, a byte at a time, as
/// `read` does, so that what follows the last line stays there to read (a
/// pipe with `-n`, or with a callback, which may read from it).
struct Input {
    fd: i32,
    buf: Vec<u8>,
    /// What is left of the block is `buf[pos..len]`.
    pos: usize,
    len: usize,
    seekable: bool,
}

impl Input {
    fn new(o: &Opts) -> Input {
        let seekable = sys::lseek(o.fd, 0, libc::SEEK_CUR).is_ok();
        let ahead = seekable || (o.count == 0 && o.callback.is_none());
        Input {
            fd: o.fd,
            buf: vec![0; if ahead { BLOCK } else { 1 }],
            pos: 0,
            len: 0,
            seekable,
        }
    }

    /// Reads the next block; false at the end of the input (or on an
    /// error). As in bash, the traps of signals that come meanwhile run, and
    /// the reading goes on (Ctrl-C in an interactive shell abandons it).
    fn fill(&mut self, sh: &mut Shell) -> Result<bool, Flow> {
        (self.pos, self.len) = (0, 0);
        loop {
            if signals::any_pending() {
                sh.run_pending_traps()?;
            }
            match sys::read(self.fd, &mut self.buf, true) {
                Ok(n) => {
                    self.len = n;
                    return Ok(n > 0);
                }
                Err(libc::EINTR) => {}
                Err(_) => return Ok(false),
            }
        }
    }

    /// The next line, with its delimiter (which the last one may lack).
    fn line(&mut self, sh: &mut Shell, delim: u8) -> Result<Option<Vec<u8>>, Flow> {
        let mut line = Vec::new();
        loop {
            if self.pos == self.len && !self.fill(sh)? {
                return Ok((!line.is_empty()).then_some(line));
            }
            let rest = &self.buf[self.pos..self.len];
            match rest.iter().position(|&c| c == delim) {
                Some(i) => {
                    line.extend_from_slice(&rest[..=i]);
                    self.pos += i + 1;
                    return Ok(Some(line));
                }
                None => {
                    line.extend_from_slice(rest);
                    self.pos = self.len;
                }
            }
        }
    }

    /// Leaves the fd after the last line taken.
    fn sync(&mut self) {
        if self.seekable && self.pos < self.len {
            let _ = sys::lseek(self.fd, self.pos as i64 - self.len as i64, libc::SEEK_CUR);
        }
        (self.pos, self.len) = (0, 0);
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.sync();
    }
}

/// The lines read and not yet assigned: they are assigned together, at the
/// end or before a callback (which may look at the array).
struct Pending<'a> {
    name: &'a [u8],
    lines: Vec<Vec<u8>>,
    /// `-O` was given: elements are assigned from an index on, and the
    /// others stay.
    keep: bool,
    /// The index of the first of `lines`.
    at: usize,
    /// Nothing was assigned yet.
    first: bool,
}

impl Pending<'_> {
    fn store(&mut self, sh: &mut Shell) -> Result<(), Flow> {
        let lines = std::mem::take(&mut self.lines);
        let first = std::mem::replace(&mut self.first, false);
        self.at += lines.len();
        if lines.is_empty() && (self.keep || !first) {
            return Ok(());
        }
        if !self.keep && first {
            return sh.set_var_value(self.name, Value::Array(Box::new(lines)));
        }
        if !self.keep {
            return sh.append_elements(self.name, lines);
        }
        // As `a+=([at]=x y)`: elements before `at` that don't exist are
        // made empty (in bash they are holes).
        let mut items: Vec<Item> = lines.into_iter().map(|value| Item { key: None, value }).collect();
        items[0].key = Some((self.at - items.len()).to_string().into_bytes());
        sh.assign_items(self.name, items, true)
    }
}

/// `mapfile [-t] [-d delim] [-n count] [-O origin] [-s count] [-u fd]
/// [-C callback] [-c quantum] [array]`.
pub fn mapfile(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let name = &argv[0];
    let mut o = Opts {
        delim: b'\n',
        chop: false,
        count: 0,
        skip: 0,
        origin: None,
        fd: 0,
        callback: None,
        quantum: 5000,
    };
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == b"--" {
            i += 1;
            break;
        }
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        let mut j = 1;
        while j < a.len() {
            let c = a[j];
            j += 1;
            match c {
                b't' => o.chop = true,
                b'd' | b'n' | b'O' | b's' | b'u' | b'C' | b'c' => {
                    let arg = if j < a.len() {
                        &a[j..]
                    } else if let Some(v) = argv.get(i + 1) {
                        i += 1;
                        v
                    } else {
                        sh.berr(name, format!("No arg for -{} option", c as char));
                        return Ok(2);
                    };
                    j = a.len();
                    let number = |what: &str, min: i64, max: i64| match parse_uint(arg) {
                        Some(n) if (min..=max).contains(&n) => Ok(n as usize),
                        _ => {
                            sh.berr(name, format!("{}: invalid {what}", String::from_utf8_lossy(arg)));
                            Err(1)
                        }
                    };
                    let max = i32::MAX as i64;
                    let parsed = match c {
                        b'd' => {
                            o.delim = arg.first().copied().unwrap_or(0);
                            Ok(())
                        }
                        b'C' => {
                            o.callback = Some(arg.to_vec());
                            Ok(())
                        }
                        b'n' => number("line count", 0, max).map(|n| o.count = n),
                        b's' => number("line count", 0, max).map(|n| o.skip = n),
                        b'O' => number("array origin", 0, MAX_INDEX).map(|n| o.origin = Some(n)),
                        b'u' => number("file descriptor specification", 0, max).map(|n| o.fd = n as i32),
                        _ => number("callback quantum", 1, max).map(|n| o.quantum = n),
                    };
                    if let Err(status) = parsed {
                        return Ok(status);
                    }
                }
                c => {
                    sh.berr(name, format!("Illegal option -{}", c as char));
                    return Ok(2);
                }
            }
        }
        i += 1;
    }
    // As in bash, further names are ignored.
    let array = argv.get(i).map_or(DEFAULT, |a| a.as_slice());
    if !is_valid_name(array) {
        sh.berr(name, format!("{}: bad variable name", String::from_utf8_lossy(array)));
        return Ok(2);
    }
    if sh.vars.is_assoc(array) {
        sh.berr(
            name,
            format!("{}: not an indexed array", String::from_utf8_lossy(array)),
        );
        return Ok(1);
    }
    // Before anything is read, and without exiting the shell, as in bash.
    if sh.vars.var(array).is_some_and(|v| v.readonly) {
        sh.berr(name, format!("{}: is read only", String::from_utf8_lossy(array)));
        return Ok(1);
    }
    if o.fd != 0 && !sys::fd_is_users(o.fd) {
        sh.berr(name, format!("{}: invalid file descriptor: Bad file descriptor", o.fd));
        return Ok(1);
    }
    let mut input = Input::new(&o);
    for _ in 0..o.skip {
        if input.line(sh, o.delim)?.is_none() {
            break;
        }
    }
    let mut pending = Pending {
        name: array,
        lines: Vec::new(),
        keep: o.origin.is_some(),
        at: o.origin.unwrap_or(0),
        first: true,
    };
    let mut count = 0;
    while let Some(mut line) = input.line(sh, o.delim)? {
        // NUL bytes are dropped, as `read` drops them.
        if o.delim != 0 && line.contains(&0) {
            line.retain(|&c| c != 0);
        }
        // A NUL delimiter always goes (in bash, it ends the value).
        if (o.chop || o.delim == 0) && line.last() == Some(&o.delim) {
            line.pop();
        }
        count += 1;
        if let Some(callback) = &o.callback
            && count % o.quantum == 0
        {
            // The callback finds the fd after this line, and the lines
            // before it in the array.
            input.sync();
            pending.store(sh)?;
            let mut text = callback.clone();
            text.extend(format!(" {} ", pending.at).as_bytes());
            text.extend(single_quote(&line));
            sh.run_string(&text)?;
        }
        pending.lines.push(line);
        if count == o.count {
            break;
        }
    }
    input.sync();
    pending.store(sh)?;
    Ok(0)
}
