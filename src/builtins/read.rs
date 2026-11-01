//! `read` and `getopts`.

use std::time::{Duration, Instant};

use crate::input::Line;
use crate::interactive::ValueEdit;
use crate::lexer::is_valid_name;
use crate::options::Opt;
use crate::prompt::Prompt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals::{self, Disposition};
use crate::sys;
use crate::vars::Item;

/// The options of `read`.
struct Opts {
    /// `-r`: backslashes are ordinary characters.
    raw: bool,
    /// `-s`: no echo on a terminal.
    silent: bool,
    /// `-e`: the line editor, with `-i`'s text to start with.
    edit: bool,
    initial: Option<Vec<u8>>,
    prompt: Option<Vec<u8>>,
    /// `-d`: the byte that ends the line (NUL for `-d ''`).
    delim: u8,
    /// `-n` or `-N`: the most characters to read, and whether to read
    /// exactly so many (`-N`: the delimiter is an ordinary character, and
    /// the text isn't split).
    count: Option<usize>,
    exact: bool,
    /// `-t`, in microseconds.
    timeout: Option<u64>,
    /// `-u`.
    fd: i32,
    /// `-A` (None) or `-a NAME`.
    array: Option<Option<Vec<u8>>>,
}

/// How reading ended.
#[derive(Clone, Copy, PartialEq)]
enum End {
    Delim,
    Count,
    /// At the end of the input (or an error).
    Eof,
    /// A signal with a trap came (as in dash), or Ctrl-C in the editor.
    Signal,
    Timeout,
}

/// The text read: each byte, with whether a backslash quoted it.
#[derive(Default)]
struct Text {
    bytes: Vec<(u8, bool)>,
    escape: bool,
}

impl Text {
    /// Adds the byte `c`; true if it ends the line.
    fn add(&mut self, c: u8, o: &Opts) -> bool {
        // As in dash and bash, NUL bytes are dropped, unless one ends the
        // line (`-d ''`).
        if c == 0 && (o.delim != 0 || o.exact || self.escape) {
            return false;
        }
        if self.escape {
            self.escape = false;
            // A backslash before a newline continues the line.
            if c != b'\n' {
                self.bytes.push((c, true));
            }
            return false;
        }
        if c == b'\\' && !o.raw {
            self.escape = true;
            return false;
        }
        if c == o.delim && !o.exact {
            return true;
        }
        self.bytes.push((c, false));
        false
    }
}

/// The terminal read from, in the modes that `-s` (no echo) and `-n` or
/// `-N` (no line buffering) ask for while this lives. Meanwhile the signals
/// that would kill the shell are caught, so that it can restore the modes
/// before it dies of them (`die`).
struct Terminal {
    fd: i32,
    saved: libc::termios,
    caught: Vec<i32>,
}

impl Terminal {
    fn new(o: &Opts) -> Option<Terminal> {
        let cbreak = o.count.is_some();
        if !(o.silent || cbreak) || !sys::isatty(o.fd) {
            return None;
        }
        let saved = sys::tcgetattr(o.fd)?;
        let mut t = saved;
        if o.silent {
            t.c_lflag &= !(libc::ECHO | libc::ECHOE | libc::ECHOK | libc::ECHONL);
        }
        if cbreak {
            t.c_lflag &= !libc::ICANON;
            t.c_cc[libc::VMIN] = 1;
            t.c_cc[libc::VTIME] = 0;
        }
        let fatal = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];
        let caught: Vec<i32> = fatal.into_iter().filter(|&s| signals::is_default(s)).collect();
        for &s in &caught {
            signals::set_disposition(s, Disposition::Catch);
        }
        sys::tcsetattr(o.fd, &t);
        Some(Terminal {
            fd: o.fd,
            saved,
            caught,
        })
    }

    /// A signal that came and would have killed the shell.
    fn fatal(&self) -> Option<i32> {
        self.caught.iter().copied().find(|&s| signals::is_pending(s))
    }

    fn restore(&self) {
        sys::tcsetattr(self.fd, &self.saved);
        for &s in &self.caught {
            signals::set_disposition(s, Disposition::Default);
        }
    }

    /// Dies of `sig`, as the shell would have without this.
    fn die(&self, sig: i32) -> ! {
        self.restore();
        signals::clear_pending(sig as usize);
        sys::raise(sig);
        sys::exit(128 + sig)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.restore();
    }
}

/// What waiting for input with a timeout gave.
enum Wait {
    Ready,
    /// Not yet, or a signal came: check again.
    Again,
    Timeout,
}

fn wait_input(fd: i32, deadline: Instant) -> Wait {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Wait::Timeout;
    }
    let ms = left.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32;
    match sys::poll_readable(fd, ms) {
        Ok(false) | Err(libc::EINTR) => Wait::Again,
        // The read reports errors.
        Ok(true) | Err(_) => Wait::Ready,
    }
}

/// Reads from the fd a byte at a time, so that nothing after the line is
/// consumed, until the line ends: at the delimiter, after `-n`'s count, at
/// end of file, at `-t`'s timeout, or, as in dash, when a signal with a
/// trap arrived (which runs after `read`, as after any command). Ctrl-C in
/// an interactive shell abandons the command.
fn read_text(sh: &mut Shell, o: &Opts, text: &mut Text, term: Option<&Terminal>) -> Result<End, Flow> {
    let deadline = o
        .timeout
        .and_then(|t| Instant::now().checked_add(Duration::from_micros(t)));
    let mut buf = [0u8; 1];
    loop {
        if o.count.is_some_and(|n| text.bytes.len() >= n) {
            return Ok(End::Count);
        }
        // Also before the first read: the signal may have come already.
        if signals::any_pending() {
            if sh.trapped_pending().is_some() {
                return Ok(End::Signal);
            }
            if let Some(t) = term
                && let Some(sig) = t.fatal()
            {
                t.die(sig);
            }
            sh.run_pending_traps()?;
        }
        if let Some(d) = deadline {
            match wait_input(o.fd, d) {
                Wait::Ready => {}
                Wait::Again => continue,
                Wait::Timeout => return Ok(End::Timeout),
            }
        }
        match sys::read(o.fd, &mut buf, true) {
            Ok(1) => {}
            Err(libc::EINTR) => continue,
            _ => return Ok(End::Eof),
        }
        if text.add(buf[0], o) {
            return Ok(End::Delim);
        }
    }
}

/// Reads the line with the line editor (`-e`), starting with `-i`'s text.
/// None if the editor can't be used.
fn edit(sh: &mut Shell, o: &Opts, text: &mut Text) -> Result<Option<End>, Flow> {
    let v = ValueEdit {
        prompt: Prompt::plain(o.prompt.clone().unwrap_or_default()),
        right: None,
        text: String::from_utf8_lossy(o.initial.as_deref().unwrap_or_default()).into_owned(),
        history: true,
        eof: true,
        vi: sh.opt(Opt::Vi),
    };
    Ok(Some(match crate::interactive::edit_value(sh, v) {
        Ok(Line::Text(t)) => {
            let _ = t.iter().any(|&c| text.add(c, o));
            End::Delim
        }
        Ok(Line::Eof) => End::Eof,
        Ok(Line::Interrupted) => {
            super::vared::interrupted(sh)?;
            End::Signal
        }
        Err(_) => return Ok(None),
    }))
}

/// A count or fd: decimal digits only.
fn number(s: &[u8]) -> Option<u64> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
}

/// `-t`'s seconds, which may have a fraction, in microseconds (as bash's
/// `uconvert`, so `''` is 0 and `1.` is 1). None if invalid.
fn timeout(s: &[u8]) -> Option<u64> {
    let (int, frac) = match s.iter().position(|&c| c == b'.') {
        Some(p) => (&s[..p], &s[p + 1..]),
        None => (s, &b""[..]),
    };
    if !int.iter().chain(frac).all(u8::is_ascii_digit) {
        return None;
    }
    let digit = |c: &u8| u64::from(c - b'0');
    let seconds = int
        .iter()
        .fold(0u64, |n, c| n.saturating_mul(10).saturating_add(digit(c)));
    let micros = frac.iter().chain(b"000000").take(6).fold(0, |n, c| n * 10 + digit(c));
    Some(seconds.saturating_mul(1_000_000).saturating_add(micros))
}

/// `read`, with bash's options. `-a NAME` (bash) and `-A` (zsh, with one
/// name) read the fields into an array, split as field splitting does
/// (bash: zsh has an empty element after a trailing delimiter).
pub fn read(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let name = &argv[0];
    let mut o = Opts {
        raw: false,
        silent: false,
        edit: false,
        initial: None,
        prompt: None,
        delim: b'\n',
        count: None,
        exact: false,
        timeout: None,
        fd: 0,
        array: None,
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
                b'r' => o.raw = true,
                b's' => o.silent = true,
                b'e' | b'E' => o.edit = true,
                b'A' if o.array.is_none() => o.array = Some(None),
                b'A' => {}
                b'a' | b'd' | b'i' | b'n' | b'N' | b'p' | b't' | b'u' => {
                    let arg = if j < a.len() {
                        a[j..].to_vec()
                    } else if let Some(v) = argv.get(i + 1) {
                        i += 1;
                        v.clone()
                    } else {
                        sh.berr(name, format!("No arg for -{} option", c as char));
                        return Ok(2);
                    };
                    j = a.len();
                    let invalid = |what: &str| {
                        sh.berr(name, format!("{}: invalid {what}", String::from_utf8_lossy(&arg)));
                        Ok(1)
                    };
                    match c {
                        b'n' | b'N' => match number(&arg).filter(|&n| n <= i32::MAX as u64) {
                            Some(n) => (o.count, o.exact) = (Some(n as usize), c == b'N'),
                            None => return invalid("number"),
                        },
                        b't' => match timeout(&arg) {
                            Some(t) => o.timeout = Some(t),
                            None => return invalid("timeout specification"),
                        },
                        b'u' => match number(&arg).and_then(|n| i32::try_from(n).ok()) {
                            Some(fd) => o.fd = fd,
                            None => return invalid("file descriptor specification"),
                        },
                        b'a' => o.array = Some(Some(arg)),
                        b'd' => o.delim = arg.first().copied().unwrap_or(0),
                        b'i' => o.initial = Some(arg),
                        _ => o.prompt = Some(arg),
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
    let mut names = &argv[i..];
    let default_name;
    match &o.array {
        // As in bash, names after `-a NAME` are ignored.
        Some(Some(n)) => {
            default_name = [n.clone()];
            names = &default_name;
        }
        Some(None) if names.len() > 1 => {
            sh.berr(name, "only one array argument allowed");
            return Ok(2);
        }
        // As in zsh.
        Some(None) if names.is_empty() => {
            default_name = [b"reply".to_vec()];
            names = &default_name;
        }
        _ => {}
    }
    // Without a name, the line goes to REPLY as it is (bash; zsh strips
    // IFS whitespace), and `-N` doesn't split it.
    let whole = names.is_empty() || o.exact;
    let reply = [b"REPLY".to_vec()];
    if names.is_empty() {
        names = &reply;
    }
    for n in names {
        if !is_valid_name(n) {
            sh.berr(name, format!("{}: bad variable name", String::from_utf8_lossy(n)));
            return Ok(2);
        }
    }
    if o.fd != 0 && !sys::fd_is_users(o.fd) {
        sh.berr(name, format!("{}: invalid file descriptor: Bad file descriptor", o.fd));
        return Ok(1);
    }
    // `-t 0`: only whether there is input (or the end of it) to read.
    if o.timeout == Some(0) {
        return Ok(if sys::poll_readable(o.fd, 0) == Ok(true) { 0 } else { 1 });
    }
    let mut text = Text::default();
    let editor = o.edit && o.fd == 0 && sys::isatty(0);
    // The editor reads a line, without a timeout, a count or no echo.
    let editor = editor && !o.silent && o.count.is_none() && o.timeout.is_none() && o.delim == b'\n';
    let edited = if editor { edit(sh, &o, &mut text)? } else { None };
    let end = match edited {
        Some(end) => end,
        None => {
            if let Some(p) = &o.prompt
                && sys::isatty(o.fd)
            {
                sys::write_all(2, p);
            }
            let term = Terminal::new(&o);
            read_text(sh, &o, &mut text, term.as_ref())?
        }
    };
    let status = match end {
        End::Delim | End::Count => 0,
        End::Eof | End::Signal => 1,
        // 128 + SIGALRM, as in bash.
        End::Timeout => 128 + libc::SIGALRM,
    };
    let line = text.bytes;
    if whole {
        let value: Vec<u8> = line.iter().map(|c| c.0).collect();
        if o.array.is_some() {
            let items = (!value.is_empty()).then_some(Item { key: None, value });
            sh.assign_items(&names[0], items.into_iter().collect(), false)?;
        } else {
            sh.set_var(&names[0], value)?;
            for n in &names[1..] {
                sh.set_var(n, Vec::new())?;
            }
        }
        return Ok(status);
    }
    let ifs = sh.get_var(b"IFS").unwrap_or_else(|| b" \t\n".to_vec());
    let is_ifs = |c: &(u8, bool)| !c.1 && ifs.contains(&c.0);
    let is_ws = |c: &(u8, bool)| is_ifs(c) && matches!(c.0, b' ' | b'\t' | b'\n');
    let mut pos = 0;
    while pos < line.len() && is_ws(&line[pos]) {
        pos += 1;
    }
    // The field at `pos`, moving `pos` past it and its delimiter.
    let next_field = |pos: &mut usize| -> Vec<u8> {
        let start = *pos;
        while *pos < line.len() && !is_ifs(&line[*pos]) {
            *pos += 1;
        }
        let v = line[start..*pos].iter().map(|c| c.0).collect();
        // Skip the delimiter: whitespace, at most one non-whitespace IFS
        // character, then whitespace.
        while *pos < line.len() && is_ws(&line[*pos]) {
            *pos += 1;
        }
        if *pos < line.len() && is_ifs(&line[*pos]) && !is_ws(&line[*pos]) {
            *pos += 1;
            while *pos < line.len() && is_ws(&line[*pos]) {
                *pos += 1;
            }
        }
        v
    };
    if o.array.is_some() {
        let mut fields = Vec::new();
        while pos < line.len() {
            fields.push(next_field(&mut pos));
        }
        sh.assign_items(
            &names[0],
            fields.into_iter().map(|value| Item { key: None, value }).collect(),
            false,
        )?;
        return Ok(status);
    }
    for (k, name) in names.iter().enumerate() {
        let value: Vec<u8> = if k + 1 == names.len() {
            // The last variable gets the rest of the line, minus trailing
            // IFS whitespace.
            let mut end = line.len();
            while end > pos && is_ws(&line[end - 1]) {
                end -= 1;
            }
            let v = line[pos.min(end)..end].iter().map(|c| c.0).collect();
            pos = line.len();
            v
        } else {
            next_field(&mut pos)
        };
        sh.set_var(name, value)?;
    }
    Ok(status)
}

/// A port of dash's `getopts`. The position is `sh.optind` (the next
/// argument, as in `$OPTIND`) and `sh.optoff` (the offset in the previous
/// argument when it has more option letters); assigning `OPTIND`, `set --`,
/// `shift` and function calls reset it.
pub fn getopts(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() < 3 {
        sh.berr(&argv[0], "Usage: getopts optstring var [arg...]");
        return Ok(2);
    }
    let (optstr, optvar) = (&argv[1], &argv[2]);
    let base: Vec<Vec<u8>> = if argv.len() == 3 {
        sh.positional.clone()
    } else {
        argv[3..].to_vec()
    };
    if sh.optind > base.len() + 1 {
        sh.optind = 1;
        sh.optoff = None;
    }
    let ind = sh.optind;
    let off = sh.optoff;
    // `next`: the index of the next argument; `p`: the position of the
    // next option letter in `base[next - 1]`.
    let mut next = ind - 1;
    let mut p = match off {
        Some(o) if ind > 1 && base[next - 1].len() >= o => Some(o),
        _ => None,
    };
    let mut c = b'?';
    let mut done = false;
    'out: {
        if p.is_none_or(|o| o == base[next - 1].len()) {
            // The current argument is done: advance.
            match base.get(next) {
                Some(w) if w.len() > 1 && w[0] == b'-' => {
                    next += 1;
                    p = Some(1);
                    if w == b"--" {
                        p = None;
                        done = true;
                        break 'out;
                    }
                }
                _ => {
                    p = None;
                    done = true;
                    break 'out;
                }
            }
        }
        let word = &base[next - 1];
        let mut o = p.unwrap();
        c = word[o];
        o += 1;
        p = Some(o);
        let mut q = 0;
        while optstr.get(q) != Some(&c) {
            if q >= optstr.len() {
                if optstr.first() == Some(&b':') {
                    sh.set_var(b"OPTARG", vec![c])?;
                } else {
                    sys::write_all(2, format!("Illegal option -{}\n", c as char).as_bytes());
                    let _ = sh.unset_var(b"OPTARG");
                }
                c = b'?';
                break 'out;
            }
            q += 1;
            if optstr.get(q) == Some(&b':') {
                q += 1;
            }
        }
        if optstr.get(q + 1) == Some(&b':') {
            let arg = if o < word.len() {
                word[o..].to_vec()
            } else if let Some(w) = base.get(next) {
                next += 1;
                w.clone()
            } else {
                if optstr.first() == Some(&b':') {
                    sh.set_var(b"OPTARG", vec![c])?;
                    c = b':';
                } else {
                    sys::write_all(2, format!("No arg for -{} option\n", c as char).as_bytes());
                    let _ = sh.unset_var(b"OPTARG");
                    c = b'?';
                }
                break 'out;
            };
            sh.set_var(b"OPTARG", arg)?;
            p = None;
        } else {
            sh.set_var(b"OPTARG", Vec::new())?;
        }
    }
    let ind = next + 1;
    sh.set_var(b"OPTIND", ind.to_string().into_bytes())?;
    // As in dash, a bad name is found only now, after OPTIND and OPTARG.
    if !is_valid_name(optvar) {
        sh.berr(
            &argv[0],
            format!("{}: bad variable name", String::from_utf8_lossy(optvar)),
        );
        (sh.optind, sh.optoff) = (ind, p);
        return Ok(2);
    }
    sh.set_var(optvar, vec![c])?;
    sh.optoff = p;
    sh.optind = ind;
    Ok(done as i32)
}
