//! Interactive tests: luish runs on a pseudo-terminal, as the session
//! leader with the pty as its controlling terminal, so job control is on.
//!
//! Each step waits for something observable rather than sleeping: either
//! some expected output, or a job taking over the terminal (the pty's
//! foreground process group changing), before Ctrl-Z or Ctrl-C is sent.
//! Carriage returns are removed from the transcript. `TERM=dumb` keeps the
//! line editor from emitting escape sequences, except in the completion and
//! highlighting tests, which need the editor.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

struct Pty {
    master: i32,
    pid: i32,
    /// Everything read so far, without carriage returns.
    out: Vec<u8>,
    /// Start of the output not yet matched by `expect`.
    mark: usize,
    dir: PathBuf,
    /// The directory is removed when the shell is dropped.
    owns_dir: bool,
    /// Whether to keep the marks for the terminal (OSC 133 and OSC 7) in
    /// `out`, rather than leave them out.
    marks: bool,
    /// Output not yet in `out`: what may be the start of a mark.
    partial: Vec<u8>,
}

impl Pty {
    fn spawn(name: &str) -> Pty {
        Pty::spawn_term(name, "dumb")
    }

    /// A shell in a new directory, whose configuration directory has a
    /// file, so that the first run's questions aren't asked.
    fn spawn_term(name: &str, term: &str) -> Pty {
        let dir = Pty::new_dir(name);
        std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
        std::fs::write(dir.join(".config/luish/luishrc"), "").unwrap();
        Pty::spawn_at(dir, term, true, None)
    }

    fn new_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("luish-pty-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Another shell in the directory (and `$HOME`) of `other`.
    fn spawn_beside(other: &Pty) -> Pty {
        Pty::spawn_at(other.dir.clone(), "dumb", false, None)
    }

    /// `path` is `$PATH`, if not the tests' own. `$LUISH_BACKGROUND` is
    /// set, so that the shell doesn't ask the terminal for its background
    /// (and wait for the answers, which nothing gives).
    fn spawn_at(dir: PathBuf, term: &str, owns_dir: bool, path: Option<&str>) -> Pty {
        Pty::spawn_env(dir, term, owns_dir, path, &["LUISH_BACKGROUND=dark"])
    }

    /// `extra` are further variables of the environment.
    fn spawn_env(dir: PathBuf, term: &str, owns_dir: bool, path: Option<&str>, extra: &[&str]) -> Pty {
        Pty::spawn_args(dir, term, owns_dir, path, extra, &["-i"])
    }

    /// The SSH mode's client, with a server on this host (`luish --remote
    /// luish --serve`), in a new directory as `spawn_term`.
    fn spawn_remote(name: &str, term: &str) -> Pty {
        let dir = Pty::new_dir(name);
        std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
        std::fs::write(dir.join(".config/luish/luishrc"), "").unwrap();
        let luish = env!("CARGO_BIN_EXE_luish");
        Pty::spawn_args(
            dir,
            term,
            true,
            None,
            &["LUISH_BACKGROUND=dark"],
            &["--remote", luish, "--serve"],
        )
    }

    /// `args` are luish's arguments.
    fn spawn_args(dir: PathBuf, term: &str, owns_dir: bool, path: Option<&str>, extra: &[&str], args: &[&str]) -> Pty {
        let shell = CString::new(env!("CARGO_BIN_EXE_luish")).unwrap();
        let argv: Vec<CString> = std::iter::once("luish")
            .chain(args.iter().copied())
            .map(|a| CString::new(a).unwrap())
            .collect();
        let path = path.map_or_else(|| std::env::var("PATH").unwrap_or_default(), Into::into);
        let env = [
            format!("PATH={path}"),
            format!("HOME={}", dir.display()),
            "PS1=$ ".to_string(),
            format!("TERM={term}"),
            "LC_ALL=C".to_string(),
        ];
        let extra = extra.iter().map(|e| e.to_string());
        let env: Vec<CString> = env.into_iter().chain(extra).map(|e| CString::new(e).unwrap()).collect();
        let mut argv_p: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
        argv_p.push(std::ptr::null());
        let mut env_p: Vec<*const libc::c_char> = env.iter().map(|a| a.as_ptr()).collect();
        env_p.push(std::ptr::null());
        let cdir = CString::new(dir.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: openpty with valid out-pointers; the child only makes
        // async-signal-safe calls before execve.
        unsafe {
            let (mut master, mut slave) = (0, 0);
            let r = libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            );
            assert_eq!(r, 0, "openpty failed");
            let pid = libc::fork();
            assert!(pid >= 0, "fork failed");
            if pid == 0 {
                libc::setsid();
                libc::ioctl(slave, libc::TIOCSCTTY, 0);
                for fd in 0..3 {
                    libc::dup2(slave, fd);
                }
                libc::close(slave);
                libc::close(master);
                libc::chdir(cdir.as_ptr());
                libc::execve(shell.as_ptr(), argv_p.as_ptr(), env_p.as_ptr());
                libc::_exit(127);
            }
            libc::close(slave);
            Pty {
                master,
                pid,
                out: Vec::new(),
                mark: 0,
                dir,
                owns_dir,
                marks: false,
                partial: Vec::new(),
            }
        }
    }

    fn transcript(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }

    fn fail(&self, what: &str) -> ! {
        panic!("{what}\n--- transcript ---\n{}\n--- end ---", self.transcript());
    }

    /// Reads whatever output is available within `wait`. Returns false at
    /// end of file.
    fn read_some(&mut self, wait: Duration) -> bool {
        let mut pfd = libc::pollfd {
            fd: self.master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll and read on our own fd with a valid buffer.
        unsafe {
            if libc::poll(&mut pfd, 1, wait.as_millis() as i32) <= 0 {
                return true;
            }
            let mut buf = [0u8; 4096];
            let n = libc::read(self.master, buf.as_mut_ptr() as *mut _, buf.len());
            if n <= 0 {
                return false;
            }
            let mut data = std::mem::take(&mut self.partial);
            data.extend(buf[..n as usize].iter().filter(|&&c| c != b'\r'));
            if !self.marks {
                data = self.strip_marks(data);
            }
            self.out.extend(data);
        }
        true
    }

    /// `data` without the marks for the terminal; an unfinished one at its
    /// end is kept in `partial`.
    fn strip_marks(&mut self, data: Vec<u8>) -> Vec<u8> {
        const MARKS: [&[u8]; 2] = [b"\x1b]133;", b"\x1b]7;"];
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let rest = &data[i..];
            if let Some(m) = MARKS.iter().find(|m| rest.starts_with(m) || m.starts_with(rest)) {
                match rest.iter().position(|&c| c == 0x07).filter(|_| rest.starts_with(m)) {
                    Some(end) => {
                        i += end + 1;
                        continue;
                    }
                    None => {
                        self.partial = rest.to_vec();
                        break;
                    }
                }
            }
            out.push(data[i]);
            i += 1;
        }
        out
    }

    /// Sets the terminal's size.
    fn resize(&self, cols: u16, rows: u16) {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCSWINSZ on our own fd with a valid winsize.
        assert_eq!(unsafe { libc::ioctl(self.master, libc::TIOCSWINSZ, &ws) }, 0);
    }

    fn send(&mut self, s: &str) {
        // SAFETY: writing a valid buffer to our own fd.
        let n = unsafe { libc::write(self.master, s.as_ptr() as *const _, s.len()) };
        assert_eq!(n, s.len() as isize);
    }

    /// Waits until `needle` appears after the previous match, and returns
    /// the output up to the end of it.
    fn expect(&mut self, needle: &str) -> String {
        let start = Instant::now();
        loop {
            let hay = &self.out[self.mark..];
            if let Some(i) = hay.windows(needle.len()).position(|w| w == needle.as_bytes()) {
                let end = self.mark + i + needle.len();
                let got = String::from_utf8_lossy(&self.out[self.mark..end]).into_owned();
                self.mark = end;
                return got;
            }
            if start.elapsed() > TIMEOUT || !self.read_some(Duration::from_millis(50)) {
                self.fail(&format!("timed out waiting for {needle:?}"));
            }
        }
    }

    /// Sends a command line and waits for the next prompt. Returns the
    /// output in between (including the echoed command line).
    fn run(&mut self, line: &str) -> String {
        self.send(line);
        self.send("\n");
        self.expect("\n$ ")
    }

    /// Waits until a job has the terminal and a process named by each of
    /// `names` (after exec) is in the job's process group, so that a signal
    /// from the terminal reaches all of them.
    fn wait_for_procs(&mut self, names: &[&str]) {
        let start = Instant::now();
        loop {
            // SAFETY: tcgetpgrp on our own fd.
            let pgrp = unsafe { libc::tcgetpgrp(self.master) };
            if pgrp > 0 && pgrp != self.pid {
                let found = procs_in_group(pgrp);
                if names.iter().all(|n| found.iter().any(|f| f == n)) {
                    return;
                }
            }
            if start.elapsed() > TIMEOUT {
                self.fail(&format!("timed out waiting for {names:?} to get the terminal"));
            }
            self.read_some(Duration::from_millis(10));
        }
    }

    /// Waits until a process named by each of `names` (after exec) is in
    /// the foreground of the SSH mode's server's pty: in a session whose
    /// leader (the shell) is the child of the relay, the client's child.
    fn wait_for_remote_procs(&mut self, names: &[&str]) {
        let start = Instant::now();
        loop {
            let found = remote_foreground(self.pid);
            if names.iter().all(|n| found.iter().any(|f| f == n)) {
                return;
            }
            if start.elapsed() > TIMEOUT {
                self.fail(&format!("timed out waiting for {names:?} to get the server's terminal"));
            }
            self.read_some(Duration::from_millis(10));
        }
    }

    /// Waits for the shell to exit and returns its exit status.
    fn exit_status(&mut self) -> i32 {
        let start = Instant::now();
        loop {
            let mut status = 0;
            // SAFETY: waiting for our own child.
            let r = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
            if r == self.pid {
                assert!(libc::WIFEXITED(status), "shell did not exit normally: {status:#x}");
                return libc::WEXITSTATUS(status);
            }
            if start.elapsed() > TIMEOUT {
                self.fail("timed out waiting for the shell to exit");
            }
            self.read_some(Duration::from_millis(10));
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // SAFETY: cleaning up our own child and fd.
        unsafe {
            libc::kill(self.pid, libc::SIGKILL);
            libc::waitpid(self.pid, std::ptr::null_mut(), 0);
            libc::close(self.master);
        }
        if self.owns_dir {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// The fields of `/proc/PID/stat` after the command name, and the name.
fn proc_stat(pid: &str) -> Option<(String, Vec<String>)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // pid (comm) state ppid pgrp session tty_nr tpgid ...
    let (open, close) = (stat.find('(')?, stat.rfind(')')?);
    let fields = stat[close + 2..].split(' ').map(String::from).collect();
    Some((stat[open + 1..close].to_string(), fields))
}

/// Command names of the processes in the foreground of the pty of the SSH
/// mode's server under the client `client`.
fn remote_foreground(client: i32) -> Vec<String> {
    let ppid = |pid: &str| proc_stat(pid).and_then(|(_, f)| f.get(1).cloned());
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some((name, f)) = proc_stat(&e.file_name().to_string_lossy()) else {
            continue;
        };
        // In its terminal's foreground group, in a session whose leader's
        // parent's parent is the client.
        let (Some(pgrp), Some(session), Some(tpgid)) = (f.get(2), f.get(3), f.get(5)) else {
            continue;
        };
        let relay = ppid(session);
        if pgrp == tpgid && relay.and_then(|r| ppid(&r)) == Some(client.to_string()) {
            out.push(name);
        }
    }
    out
}

/// Command names of the processes in a process group (from `/proc`).
fn procs_in_group(pgrp: i32) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // pid (comm) state ppid pgrp ...
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let fields: Vec<&str> = stat[close + 2..].split(' ').collect();
        if fields.get(2).and_then(|f| f.parse::<i32>().ok()) == Some(pgrp) {
            out.push(stat[open + 1..close].to_string());
        }
    }
    out
}

fn assert_has(got: &str, want: &str) {
    assert!(got.contains(want), "expected {want:?} in:\n{got}");
}

#[test]
fn stop_background_foreground() {
    let mut sh = Pty::spawn("stop");
    sh.expect("$ ");
    sh.send("sleep 30\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x1a");
    assert_has(&sh.expect("\n$ "), "^Z[1] + Stopped                    sleep 30\n");
    assert_has(&sh.run("echo st=$?"), "st=148\n");
    assert_has(&sh.run("jobs"), "\n[1] + Stopped                    sleep 30\n");
    assert_has(&sh.run("bg"), "\n[1] sleep 30\n");
    assert_has(&sh.run("jobs"), "\n[1] + Running                    sleep 30\n");
    sh.send("fg %1\n");
    sh.expect("\nsleep 30\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x03");
    sh.expect("\n$ ");
    assert_has(&sh.run("echo st=$?"), "st=130\n");
    // The finished job is gone.
    assert_eq!(sh.run("jobs"), "jobs\n$ ");
    sh.send("exit 3\n");
    assert_eq!(sh.exit_status(), 3);
}

#[test]
fn stopped_pipeline_and_exit_warning() {
    let mut sh = Pty::spawn("pipeline");
    sh.expect("$ ");
    sh.send("sleep 30 | cat\n");
    sh.wait_for_procs(&["sleep", "cat"]);
    sh.send("\x1a");
    assert_has(&sh.expect("\n$ "), "[1] + Stopped                    sleep 30 | cat\n");
    // A new background job goes after the stopped one.
    sh.run("sleep 31 &");
    let jobs = sh.run("jobs");
    assert_has(
        &jobs,
        "[1] + Stopped                    sleep 30 | cat\n[2] - Running                    sleep 31\n",
    );
    assert_has(&sh.run("exit"), "You have stopped jobs.\n");
    // `wait` reports the signal, and the job is reported before the next
    // prompt.
    assert_has(
        &sh.run("kill %2; wait %2; echo st=$?"),
        "Terminated\nst=143\n[2] - Terminated                 sleep 31\n$ ",
    );
    // As in dash, a second `exit` right after the warning exits.
    assert_has(&sh.run("exit"), "You have stopped jobs.\n");
    sh.send("exit\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn disown_stopped() {
    let mut sh = Pty::spawn("disown");
    sh.expect("$ ");
    sh.send("sleep 30\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x1a");
    assert_has(&sh.expect("\n$ "), "[1] + Stopped                    sleep 30\n");
    // A stopped job is disowned with a warning that tells how to continue
    // it, and `exit` no longer warns about it.
    let out = sh.run("disown");
    let marker = "disown: warning: job is stopped, use `kill -CONT -";
    let at = out.find(marker).unwrap_or_else(|| panic!("no warning in:\n{out}")) + marker.len();
    let pgid: String = out[at..].chars().take_while(char::is_ascii_digit).collect();
    assert!(
        !pgid.is_empty() && out[at + pgid.len()..].starts_with("' to resume"),
        "{out}"
    );
    assert_eq!(sh.run("jobs"), "jobs\n$ ");
    sh.run(&format!("kill -KILL -{pgid}"));
    // `&!` disowns a job under job control too.
    sh.run("sleep 30 &!");
    assert_eq!(sh.run("jobs"), "jobs\n$ ");
    sh.run("kill $!");
    sh.send("exit\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn pipefail_job_control() {
    let mut sh = Pty::spawn("pipefail");
    sh.expect("$ ");
    // Under job control a pipeline is a job, whose status comes from
    // `Job::status`.
    assert_has(&sh.run("false | true; echo st=$?"), "st=0\n");
    sh.run("set -o pipefail");
    assert_has(&sh.run("(exit 3) | (exit 2) | true; echo st=$?"), "st=2\n");
    // `pipestatus` comes from the job's processes.
    assert_has(
        &sh.run("(exit 3) | (exit 2) | true; echo ps=${pipestatus[*]}"),
        "ps=3 2 0\n",
    );
    assert_has(&sh.run("(exit 3) | true & wait %1; echo st=$?"), "st=3\n");
    // A stopped job's status is that of the process that stopped.
    sh.send("sleep 30 | true\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x1a");
    sh.expect("\n$ ");
    assert_has(&sh.run("echo st=$?"), "st=148\n");
}

#[test]
fn interrupt_and_terminal_input() {
    let mut sh = Pty::spawn("interrupt");
    sh.expect("$ ");
    // Ctrl-C kills the job, and the shell abandons the rest of the line.
    sh.send("while :; do sleep 5; done; echo after\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x03");
    let out = sh.expect("\n$ ");
    assert!(!out.contains("\nafter\n"), "the loop went on after Ctrl-C:\n{out}");
    assert_has(&sh.run("echo st=$?"), "st=130\n");
    // A foreground job reads from the terminal, then the shell gets it back.
    sh.send("cat\n");
    sh.wait_for_procs(&["cat"]);
    sh.send("hello\n");
    sh.expect("hello\nhello\n");
    sh.send("\x04");
    sh.expect("$ ");
    assert_has(&sh.run("echo st=$? $-"), "st=0 Esmi\n");
    // A background job is reported before the prompt once it has finished.
    assert_has(
        &sh.run("sleep 0 & wait"),
        "\n[1] + Done                       sleep 0\n$ ",
    );
}

#[test]
fn terminal_modes_restored() {
    let mut sh = Pty::spawn("modes");
    sh.expect("$ ");
    let check = "case $(stty -a) in *' -echo '*) echo echo=off;; *) echo echo=on;; esac";
    // A program killed by a signal can't restore the terminal: the shell does.
    sh.run("sh -c 'stty -echo; kill -9 $$'");
    assert_has(&sh.run(check), "echo=on\n");
    // A program that exits normally keeps its settings (as in bash). With
    // echo off, the command lines are not echoed.
    sh.run("stty -echo");
    assert_eq!(sh.run(check), "echo=off\n$ ");
    sh.send("stty echo\n");
    assert_eq!(sh.expect("$ "), "$ ");
    assert_has(&sh.run(check), "echo=on\n");
}

#[test]
fn job_control_off() {
    let mut sh = Pty::spawn("nomonitor");
    sh.expect("$ ");
    assert_has(&sh.run("set +m; echo $-"), "Esi\n");
    // Background jobs get their own process group only under job control.
    std::fs::write(sh.path("pg.sh"), "ps -o pgid= -p $$ > \"$1\"\n").unwrap();
    sh.run("sh pg.sh off & wait");
    sh.run("set -m; sh pg.sh on & wait");
    let read = |p: &Path| std::fs::read_to_string(p).unwrap().trim().to_string();
    let off = read(&sh.path("off"));
    let on = read(&sh.path("on"));
    assert_eq!(
        off,
        sh.pid.to_string(),
        "without job control, jobs stay in the shell's group"
    );
    assert_ne!(on, sh.pid.to_string(), "under job control, a job has its own group");
}

#[test]
fn jobs_menu() {
    let mut sh = Pty::spawn_term("jobsmenu", "vt100");
    sh.expect("$ ");
    // Without jobs, there is no menu.
    sh.send("jobs -i; echo none=$?\n");
    sh.expect("none=0\n");
    // Each line waits for the prompt: the editor discards what is typed
    // before it.
    sh.send("sleep 31 & sleep 32 & echo started\n");
    sh.expect("started\n");
    sh.expect("$ ");
    // The current job is selected; s stops it and b continues it.
    sh.send("jobs -i\n");
    sh.expect("\x1b[7m> [2]+");
    sh.expect("q: quit");
    sh.send("s");
    sh.expect("Stopped (signal)");
    sh.send("b");
    sh.expect("Continued [2] in the background");
    // K lists the signals; any other key sends none.
    sh.send("K");
    sh.expect("Send to [2] sleep 32:");
    sh.send("x");
    sh.expect("Nothing sent");
    sh.send("Kk");
    sh.expect("Killed");
    sh.send("b");
    sh.expect("[2] has ended");
    // TERM, then KILL: the job ends at TERM.
    sh.send("1Ke");
    sh.expect("Terminated");
    sh.send("q");
    sh.expect("\x1b[?25h");
    sh.expect("$ ");
    // A job that ignores TERM gets KILL, which leaving the menu waits for.
    sh.send("(trap '' TERM; sleep 33) & echo started\n");
    sh.expect("started\n");
    sh.expect("$ ");
    sh.send("jobs -i\n");
    sh.expect("q: quit");
    sh.send("Ke");
    sh.expect("KILL in 5 s");
    sh.send("q");
    sh.expect("Waiting to send KILL");
    sh.expect("Killed");
    sh.expect("$ ");
    // f (or Enter) brings a job to the foreground, as fg does.
    sh.send("sleep 34 & echo started\n");
    sh.expect("started\n");
    sh.expect("$ ");
    sh.send("jobs -i\n");
    sh.expect("q: quit");
    sh.send("\r");
    sh.expect("sleep 34\n");
    sh.wait_for_procs(&["sleep"]);
    sh.send("\x03");
    sh.expect("$ ");
    sh.send("echo st=$?\n");
    sh.expect("st=130\n");
    sh.send("exit\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn syntax_highlighting() {
    let mut sh = Pty::spawn_term("highlight", "vt100");
    sh.expect("$ ");
    // A reserved word, a string, and an unknown command, in the default colours.
    sh.send("if nosuchcommand 'x'");
    sh.expect("\x1b[1;34mif\x1b[0m \x1b[1;31mnosuchcommand\x1b[0m \x1b[33m'x'\x1b[0m");
    sh.send("\x03");
    sh.expect("$ ");
    // An exported, a plain and an unset variable.
    sh.send("x=1\n");
    sh.expect("$ ");
    sh.send("echo $PWD $x $NOSUCH ");
    sh.expect("\x1b[1;36m$PWD\x1b[0m \x1b[36m$x\x1b[0m \x1b[2;36m$NOSUCH\x1b[0m ");
    sh.send("\x03");
    sh.expect("$ ");
    // A syntax error, marked from there on, but not while it is the word
    // being typed; incomplete input is not an error.
    sh.send("if true; then echo; fi; fi");
    sh.expect("\x1b[1m;\x1b[0m \x1b[1;34mfi\x1b[0m\x1b[28C");
    sh.send(" x");
    sh.expect("\x1b[1m;\x1b[0m \x1b[1;4;31mfi\x1b[0m\x1b[4;31m x\x1b[0m");
    sh.send("\x03");
    sh.expect("$ ");
    // On a continuation line, the error is found with the lines before.
    sh.send("if true\n");
    sh.expect("> ");
    sh.send("fi x");
    sh.expect("> \x1b[1;4;31mfi\x1b[0m\x1b[4;31m x\x1b[0m");
    sh.send("\x03");
    sh.expect("$ ");
    // With `setopt highlight.paths`, a word that names a file is
    // underlined, and so is the word being typed if it begins a name.
    sh.send("setopt highlight.paths; : >afile\n");
    sh.expect("$ ");
    sh.send("ls afile ~/af");
    sh.expect("\x1b[32mls\x1b[0m \x1b[4mafile\x1b[0m \x1b[4;34m~\x1b[0m\x1b[4m/af\x1b[0m");
    sh.send("x");
    sh.expect("\x1b[4mafile\x1b[0m \x1b[34m~\x1b[0m/afx");
    sh.send("\x03");
    sh.expect("$ ");
    // `style` changes a role; the second line continues a quote.
    sh.send("style string underline\n");
    sh.expect("$ ");
    sh.send("echo 'a\n");
    sh.expect("> ");
    sh.send("b' x");
    sh.expect("\x1b[4mb'\x1b[0m x");
    sh.send("\x03");
    sh.expect("$ ");
    // `setopt editor.no_highlight` turns it off.
    sh.send("setopt editor.no_highlight\n");
    sh.expect("$ ");
    sh.send("fi");
    sh.expect("fi");
    sh.send("\x03");
    sh.expect("$ ");
    assert!(!sh.transcript()[sh.transcript().rfind("no_highlight").unwrap()..].contains("\x1b[1;34m"));
    sh.send("unsetopt editor.no_highlight\n");
    sh.expect("$ ");
    // NO_COLOR turns it off.
    sh.send("NO_COLOR=1\n");
    sh.expect("$ ");
    sh.send("if");
    sh.expect("if");
    sh.send(" true; then echo o''k; fi\n");
    sh.expect("ok\n");
    assert!(!sh.transcript()[sh.transcript().rfind("NO_COLOR").unwrap()..].contains("\x1b[1;34m"));
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// Without `$LUISH_BACKGROUND`, the shell asks the terminal for its
/// background before the first prompt, and `style --detect` asks again.
#[test]
fn background_detection() {
    const QUERY: &str = "\x1b]11;?\x1b\\\x1b[c";
    let dir = Pty::new_dir("background");
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    std::fs::write(dir.join(".config/luish/luishrc"), "").unwrap();
    let mut sh = Pty::spawn_env(dir, "vt100", true, None, &[]);
    // Only DA1 answers: the background isn't known. The text typed before
    // the answer starts the line, without what follows a control key.
    sh.expect(QUERY);
    sh.send("echo typed\x1b[Ax\x1b[?62;22c");
    sh.expect("$ ");
    sh.send(" $LUISH_BACKGROUND.\n");
    sh.expect("\ntyped .\n");
    sh.send("style -c\n");
    sh.expect("* default-dark (background unknown)");
    // The terminal tells a light colour, ended by BEL.
    sh.send("style --detect\n");
    sh.expect(QUERY);
    sh.send("\x1b]11;rgb:ffff/ffff/dddd\x07\x1b[?62;22c");
    sh.expect("$ ");
    sh.send("style -c\n");
    sh.expect("* default-light (light background, from the terminal)");
    // Unknown again: an error, and the background stays as it was.
    sh.send("style --detect || echo failed\n");
    sh.expect(QUERY);
    sh.send("\x1b[?6c");
    sh.expect("style: the terminal didn't tell its background colour\nfailed\n");
    sh.send("echo $LUISH_BACKGROUND\n");
    sh.expect("\nlight\n");
}

/// A colour scheme's terminal colours: before setting them, the shell asks
/// what they were; it sets only what changes, and puts back what it found
/// (or resets what the terminal didn't tell) when they no longer apply and
/// when it exits.
#[test]
fn terminal_colors() {
    let dir = Pty::new_dir("termcolors");
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    let rc = "style -s g -i default-dark\n\
              style -s g terminal.background '#282828'\n\
              style -s g terminal.palette '#000000 #cc241d'\n\
              style -c g\n";
    std::fs::write(dir.join(".config/luish/luishrc"), rc).unwrap();
    let mut sh = Pty::spawn_env(dir, "vt100", true, None, &["LUISH_BACKGROUND=dark"]);
    sh.expect("\x1b]4;0;?\x1b\\\x1b]4;1;?\x1b\\\x1b]11;?\x1b\\\x1b[c");
    // Colour 1 of the palette isn't told: it is reset rather than put back.
    sh.send("\x1b]11;rgb:ffff/ffff/ffff\x07\x1b]4;0;rgb:0/0/0\x1b\\\x1b[?62c");
    const SET_PALETTE: &str = "\x1b]4;0;#000000\x1b\\\x1b]4;1;#cc241d\x1b\\";
    sh.expect(&format!("{SET_PALETTE}\x1b]11;#282828\x1b\\"));
    sh.expect("$ ");
    // Only what changed is set again.
    sh.send("style -s g terminal.background '#fbf1c7'\n");
    let got = sh.expect("\x1b]11;#fbf1c7\x1b\\");
    assert!(!got.contains("\x1b]4;"), "{got:?}");
    sh.expect("$ ");
    const RESTORE: &str = "\x1b]4;0;#000000\x1b\\\x1b]104;1\x1b\\\x1b]11;#ffffff\x1b\\";
    sh.send("style --terminal-colors off\n");
    sh.expect(RESTORE);
    sh.expect("$ ");
    // On again: set without asking again.
    sh.send("style --terminal-colors on\n");
    let got = sh.expect(&format!("{SET_PALETTE}\x1b]11;#fbf1c7\x1b\\"));
    assert!(!got.contains(";?"), "{got:?}");
    sh.expect("$ ");
    // A scheme without terminal colours puts them back too.
    sh.send("style -c default-dark\n");
    sh.expect(RESTORE);
    sh.expect("$ ");
    sh.send("style -c g\n");
    sh.expect("\x1b]11;#fbf1c7\x1b\\");
    sh.expect("$ ");
    // Asking for the background asks for the terminal's own, then the
    // scheme's colours are set again.
    sh.send("style --detect\n");
    sh.expect(&format!("{RESTORE}\x1b]11;?\x1b\\\x1b[c"));
    sh.send("\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[?62c");
    sh.expect(&format!("{SET_PALETTE}\x1b]11;#fbf1c7\x1b\\"));
    sh.expect("$ ");
    sh.send("exit\n");
    sh.expect(RESTORE);
    assert_eq!(sh.exit_status(), 0);
}

/// `exec` puts back the terminal colours that the scheme set.
#[test]
fn terminal_colors_exec() {
    let dir = Pty::new_dir("termcolors-exec");
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    let rc = "style -s g terminal.foreground '#ebdbb2'\nstyle -c g\n";
    std::fs::write(dir.join(".config/luish/luishrc"), rc).unwrap();
    let mut sh = Pty::spawn_env(dir, "vt100", true, None, &["LUISH_BACKGROUND=dark"]);
    sh.expect("\x1b]10;?\x1b\\\x1b[c");
    sh.send("\x1b[?62c");
    sh.expect("\x1b]10;#ebdbb2\x1b\\");
    sh.expect("$ ");
    // Not in a subshell, which doesn't own the terminal's colours.
    sh.send("(exec true); echo sub\n");
    let got = sh.expect("\nsub\n");
    assert!(!got.contains("\x1b]110"), "{got:?}");
    sh.expect("$ ");
    sh.send("exec true\n");
    sh.expect("\x1b]110\x1b\\");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn prompt_percent() {
    let mut sh = Pty::spawn_term("prompt", "vt100");
    sh.expect("$ ");
    // An OSC sequence in %{...%}, and colours: the line editor measures the
    // prompt without them, so the cursor goes to column 4 + 4 after "echo".
    sh.send("setopt promptpercent; PS1=\"$(printf '%%{\\033]0;t\\007%%}%%F{red}ab%%f%%%% ')\"\n");
    sh.expect("\x1b]0;t\x07\x1b[31mab\x1b[39m% ");
    sh.send("echo");
    sh.expect("echo\x1b[0m\x1b[8C");
    sh.send(" x\n");
    sh.expect("x\n");
    sh.expect("ab\x1b[39m% ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// The marks for the terminal: where each prompt, command line and
/// command output is (OSC 133), and the current directory (OSC 7) when it
/// changes; none with `terminal.no_integration`.
#[test]
fn terminal_integration() {
    let mut sh = Pty::spawn_term("integration", "vt100");
    sh.marks = true;
    let host = {
        let mut buf = [0u8; 256];
        // SAFETY: gethostname into a buffer of the given size.
        unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() - 1) };
        String::from_utf8_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap()]).into_owned()
    };
    let dir = sh.dir.display().to_string();
    const PROMPT: &str = "\x1b]133;A\x07$ \x1b]133;B\x07";
    sh.expect(&format!("\x1b]7;file://{host}{dir}\x07"));
    sh.expect(PROMPT);
    sh.send("echo hi; false\n");
    sh.expect("\n\x1b]133;C\x07hi\n\x1b]133;D;1\x07");
    sh.expect(PROMPT);
    // The directory is reported again only once it changes.
    sh.send("mkdir 'a b' && cd 'a b'\n");
    sh.expect(&format!("\x1b]133;D;0\x07\x1b]7;file://{host}{dir}/a%20b\x07"));
    sh.expect(PROMPT);
    sh.send(":\n");
    sh.expect("\x1b]133;C\x07");
    let got = sh.expect(PROMPT);
    assert!(got.contains("\x1b]133;D;0\x07") && !got.contains("\x1b]7;"), "{got:?}");
    // A continuation line's prompt is marked as one.
    sh.send("if :\n");
    sh.expect("\x1b]133;A;k=s\x07> \x1b]133;B\x07");
    sh.send("then echo ok; fi\n");
    sh.expect("\x1b]133;C\x07ok\n\x1b]133;D;0\x07");
    sh.expect(PROMPT);
    // A syntax error is the output of a command that failed.
    sh.send("fi\n");
    sh.expect("\x1b]133;C\x07");
    sh.expect("unexpected");
    sh.expect("\x1b]133;D;2\x07");
    sh.expect(PROMPT);
    sh.send("setopt terminal.no_integration\n");
    sh.expect("\x1b]133;D;0\x07");
    sh.expect("$ ");
    sh.send("cd .. && echo x\n");
    let got = sh.expect("\nx\n");
    let got = got + &sh.expect("$ ");
    assert!(!got.contains("\x1b]"), "{got:?}");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// In error messages, the names of files are links to them (OSC 8); not
/// with `terminal.no_integration`.
#[test]
fn error_links() {
    let mut sh = Pty::spawn_term("links", "vt100");
    let host = {
        let mut buf = [0u8; 256];
        // SAFETY: gethostname into a buffer of the given size.
        unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() - 1) };
        String::from_utf8_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap()]).into_owned()
    };
    let dir = sh.dir.display().to_string();
    std::fs::write(sh.path("lib.sh"), "f() { nosuchcmd_x; }\n").unwrap();
    let link = format!("\x1b]8;;file://{host}{dir}/lib.sh\x1b\\./lib.sh\x1b]8;;\x1b\\");
    sh.expect("$ ");
    sh.send(". ./lib.sh; f\n");
    sh.expect(&format!("{link}: 1: nosuchcmd_x: not found"));
    sh.expect("$ ");
    sh.send("setopt terminal.no_integration; f\n");
    sh.expect("\n./lib.sh: 1: nosuchcmd_x: not found");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// In vi mode, the cursor's shape follows the input mode: a bar to insert,
/// a block for commands, an underline to replace; the terminal's own is put
/// back for the commands that run.
#[test]
fn vi_cursor_shape() {
    let mut sh = Pty::spawn_term("vicursor", "vt100");
    sh.expect("$ ");
    sh.send("set -o vi\n");
    sh.expect("\x1b[6 q");
    sh.expect("$ ");
    sh.send("echo hi\x1b");
    sh.expect("\x1b[2 q");
    sh.send("R");
    sh.expect("\x1b[4 q");
    sh.send("\x1b");
    sh.expect("\x1b[2 q");
    sh.send("A");
    sh.expect("\x1b[6 q");
    sh.send("!\n");
    sh.expect("\x1b[0 q");
    sh.expect("hi!\n");
    sh.expect("\x1b[6 q");
    sh.send("setopt terminal.no_integration\n");
    sh.expect("\x1b[0 q");
    sh.send("echo x\x1bA\n");
    let got = sh.expect("\nx\n");
    let got = got + &sh.expect("$ ");
    assert!(!got.contains(" q"), "{got:?}");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// std's notify plugin: a notification (OSC 777) when a command that took
/// at least `$LUISH_NOTIFY_AFTER` seconds ends.
#[cfg(feature = "plugins")]
#[test]
fn notify_plugin() {
    let mut sh = Pty::spawn_term("notify", "vt100");
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("luish-std-plugins/notify.rhai");
    sh.expect("$ ");
    sh.send(&format!(
        "plugin load {}; LUISH_NOTIFY_AFTER=0; echo ready\n",
        plugin.display()
    ));
    sh.expect("\nready\n");
    sh.expect("$ ");
    sh.send("echo hi; false\n");
    sh.expect("\nhi\n\x1b]777;notify;Failed (status 1);echo hi; false (0 s)\x07");
    sh.expect("$ ");
    // The command's lines are joined.
    sh.send("if :\n");
    sh.expect("> ");
    sh.send("then :; fi\n");
    sh.expect("\x1b]777;notify;Done;if : then :; fi (0 s)\x07");
    sh.expect("$ ");
    // None for a command quicker than that.
    sh.send("LUISH_NOTIFY_AFTER=100\n");
    sh.expect("$ ");
    sh.send("echo x\n");
    let got = sh.expect("\nx\n");
    let got = got + &sh.expect("$ ");
    assert!(!got.contains("777"), "{got:?}");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// `clipcopy` sends its input or a file to the terminal's clipboard (OSC
/// 52, base64), through `/dev/tty`, so also from a pipeline whose output
/// goes elsewhere.
#[test]
fn clipcopy() {
    let mut sh = Pty::spawn("clipcopy");
    sh.expect("$ ");
    sh.send("printf 'hi\\n' | clipcopy >/dev/null; echo \"status $?\"\n");
    sh.expect("\x1b]52;c;aGkK\x07status 0\n");
    sh.send("printf foob > f; clipcopy f\n");
    sh.expect("\x1b]52;c;Zm9vYg==\x07");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn right_prompt() {
    let mut sh = Pty::spawn_term("rprompt", "vt100");
    sh.resize(20, 24);
    sh.expect("$ ");
    // On a terminal 20 wide, a right prompt 3 wide starts at column 17,
    // which leaves the last one free (ZLE_RPROMPT_INDENT), between a save
    // and a restore of the cursor.
    sh.send("NO_COLOR=1 RPROMPT='[$((1+1))]'\n");
    sh.expect("$ \x1b7\x1b[17G[2]\x1b8");
    // Shown while a column is left free before it.
    sh.send("echo 12345678");
    sh.expect("$ echo 12345678\x1b7\x1b[17G[2]\x1b8\x1b[15C");
    sh.send("9");
    sh.expect("$ echo 123456789\x1b[16C");
    sh.send("\x7f");
    sh.expect("$ echo 12345678\x1b7\x1b[17G[2]\x1b8\x1b[15C");
    // It stays when the line is accepted.
    sh.send("\n");
    sh.expect("$ echo 12345678\x1b7\x1b[17G[2]\x1b8\x1b[15C");
    sh.expect("\n12345678\n");
    sh.expect("$ \x1b7\x1b[17G[2]\x1b8");
    // Also with the right prompt of PS2 and an indent of 0.
    sh.send("ZLE_RPROMPT_INDENT=0 RPS2='<>'\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    sh.send("if :\n");
    sh.expect("> \x1b7\x1b[19G<>\x1b8");
    sh.send("then echo ok; fi\n");
    sh.expect("\nok\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    // The autosuggestion pushes it out as the line does.
    sh.send("setopt autosuggest\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    sh.send("echo abcdefghijkl\n");
    sh.expect("\nabcdefghijkl\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    sh.send("echo a");
    sh.expect("$ echo a\x1b[90mbcdefghijkl\x1b[0m");
    sh.send("\x15");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    // With prompt.transient_rprompt, the line is redrawn without it once
    // it is accepted.
    sh.send("setopt prompt.transient_rprompt\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    sh.send("echo hi");
    sh.expect("$ echo hi\x1b7\x1b[18G[2]\x1b8");
    sh.send("\n");
    sh.expect("$ echo hi\x1b[9C");
    sh.expect("\nhi\n");
    sh.expect("$ \x1b7\x1b[18G[2]\x1b8");
    // It sees the variables that plugins give the prompt.
    #[cfg(feature = "plugins")]
    {
        std::fs::write(sh.path("vars.rhai"), "sh::hook(\"prompt-vars\", || #{ pv: \"x\" });\n").unwrap();
        sh.send("ZLE_RPROMPT_INDENT=1 RPROMPT='[$pv]'; plugin load ~/vars.rhai\n");
        sh.expect("$ \x1b7\x1b[17G[x]\x1b8");
    }
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn tab_completion() {
    let mut sh = Pty::spawn_term("complete", "vt100");
    std::fs::write(sh.path("completeme file"), "found it\n").unwrap();
    std::fs::write(sh.path("alpha1"), "").unwrap();
    std::fs::write(sh.path("alpha2"), "").unwrap();
    sh.expect("$ ");
    // A filename, quoted as it is completed.
    sh.send("cat compl\t\n");
    sh.expect("found it\n");
    sh.expect("$ ");
    // A function name in command position.
    sh.send("myuniquefunc() { echo ran-$1; }\n");
    sh.expect("$ ");
    sh.send("myuniquef\tx\n");
    sh.expect("ran-x\n");
    sh.expect("$ ");
    // A match ignoring case, then one in the middle of the name: the text
    // typed is replaced.
    std::fs::write(sh.path("Upper Case"), "upper\n").unwrap();
    sh.send("cat upp\t\n");
    sh.expect("upper\n");
    sh.expect("$ ");
    sh.send("cat etem\t\n");
    sh.expect("found it\n");
    sh.expect("$ ");
    // A second tab opens the menu.
    sh.send("echo alp\t\t");
    sh.expect("alpha1  alpha2");
    sh.send("\x03");
    // The new prompt (not the line redrawn under the menu).
    sh.expect("\x1b[K$ ");
    // Job specs, from the jobs when the prompt was shown, with their
    // commands.
    sh.send(": | sleep 100 & : | sleep 101 & echo started\n");
    sh.expect("started\n");
    sh.expect("\x1b[?2004h");
    sh.send("fg %\t");
    sh.expect("%1  \x1b[90m-- : | sleep 100\x1b[0m");
    sh.expect("%2  \x1b[90m-- : | sleep 101\x1b[0m");
    sh.send("\x03");
    // The next prompt: input sent before it may be discarded.
    sh.expect("\x1b[?2004h");
    sh.send("kill %1 %2; wait; echo killed\n");
    sh.expect("killed\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn completion_menu() {
    let mut sh = Pty::spawn_term("menu", "vt100");
    for f in ["alpha1", "alpha2", "beta1", "beta2"] {
        std::fs::write(sh.path(f), "").unwrap();
    }
    sh.expect("$ ");
    // The first Tab completes what the matches have in common, the second
    // opens the menu, and the third selects the first match, putting it in
    // the line.
    sh.send("echo alp\t\t\t");
    sh.expect("\x1b[7malpha1\x1b[0m  alpha2");
    // Right moves to the next column, and Enter keeps the match.
    sh.send("\x1b[C");
    sh.expect("alpha1  \x1b[7malpha2\x1b[0m");
    sh.send("\rx\n");
    sh.expect("alpha2 x\n");
    sh.expect("$ ");
    // Typing goes on after the match.
    sh.send("echo alp\t\t\t\tx\n");
    sh.expect("alpha2 x\n");
    sh.expect("$ ");
    // Shift-Tab goes backwards, and Ctrl-G puts back the text typed.
    sh.send("echo alp\t\t\x1b[Z");
    sh.expect("alpha1  \x1b[7malpha2\x1b[0m");
    sh.send("\x07\n");
    sh.expect("alpha\n");
    sh.expect("$ ");
    // So does Esc on its own.
    sh.send("echo bet\t\t\t");
    sh.expect("\x1b[7mbeta1\x1b[0m");
    sh.send("\x1b");
    sh.expect("echo\x1b[0m beta\x1b[");
    sh.send("\n");
    sh.expect("beta\n");
    sh.expect("$ ");
    // In vi mode, Esc closes the menu, and a second one goes to command
    // mode.
    sh.send("set -o vi\n");
    sh.expect("\x1b[?2004h");
    sh.send("echo bet\t\t\t");
    sh.expect("\x1b[7mbeta1\x1b[0m");
    sh.send("\x1b");
    sh.expect("echo\x1b[0m beta\x1b[");
    sh.send("\x1bAx\n");
    sh.expect("betax\n");
    sh.expect("$ ");
    sh.send("set +o vi\n");
    sh.expect("\x1b[?2004h");
    // A menu taller than the screen scrolls, along the rows.
    for i in 0..30 {
        std::fs::write(sh.path(&format!("c{i:02}")), "").unwrap();
    }
    sh.resize(30, 5);
    sh.send("echo c\t");
    sh.expect("c00  c01  c02  c03  c04  c05\nc06");
    sh.expect("rows 1-3 of 5");
    sh.send("\t\x1b[6~");
    sh.expect("\x1b[7mc18\x1b[0m");
    sh.expect("rows 2-4 of 5");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    sh.resize(80, 24);
    // Enter runs the line if nothing is selected, and the menu is erased.
    sh.send("echo bet\t\t");
    sh.expect("beta1  beta2");
    sh.send("\r");
    sh.expect("\x1b[?2026h");
    let out = sh.expect("beta\n");
    assert!(!out.contains("beta1"), "{out}");
    sh.expect("$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// `plugin` confirms what it did at the prompt, in the colours of the
/// scheme (`plugin.*` styles), and is silent in functions and with
/// `$NO_COLOR` plain.
#[cfg(feature = "plugins")]
#[test]
fn plugin_feedback() {
    let mut sh = Pty::spawn("plugin-feedback");
    std::fs::write(sh.path("a.rhai"), "let x = 1;\n").unwrap();
    sh.expect("$ ");
    sh.send("plugin list-loaded\n");
    sh.expect("\x1b[90mNo plugins loaded\x1b[m\n");
    sh.expect("$ ");
    sh.send("plugin load ./a.rhai\n");
    sh.expect("\x1b[32mLoaded\x1b[m \x1b[1ma\x1b[m\n");
    sh.expect("$ ");
    sh.send("plugin load ./a.rhai\n");
    sh.expect("\x1b[32mReloaded\x1b[m \x1b[1ma\x1b[m\n");
    sh.expect("$ ");
    sh.send("plugin list-loaded\n");
    sh.expect("\x1b[1ma\x1b[m\n");
    sh.expect("$ ");
    // Not in a function, nor in a command substitution.
    sh.send("f() { plugin load ./a.rhai; }; f; echo \"[$(plugin load ./a.rhai)]\"\n");
    sh.expect("[]\n");
    sh.expect("$ ");
    sh.send("plugin unload a\n");
    sh.expect("\x1b[32mUnloaded\x1b[m \x1b[1ma\x1b[m\n");
    sh.expect("$ ");
    sh.send("NO_COLOR=1\n");
    sh.expect("$ ");
    sh.send("plugin load ./a.rhai\n");
    sh.expect("Loaded a\n");
    sh.expect("$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
    let transcript = sh.transcript();
    let after = &transcript[transcript.rfind("NO_COLOR=1").unwrap()..];
    assert!(!after.contains("\x1b[32m"), "{after}");
}

#[cfg(feature = "plugins")]
#[test]
fn plugin_completer() {
    let mut sh = Pty::spawn_term("completer", "vt100");
    std::fs::write(
        sh.path("comp.rhai"),
        r#"sh::completer("frob", |words, i| {
    let w = words[i];
    if i == 1 {
        // An external command, and one whose output is captured.
        sh::run("/bin/true");
        let r = sh::capture(["echo", "captured"]);
        [r.out, "alpha", #{value: "beta", desc: "the second"}, #{value: "--opt=", suffix: ""}]
    } else if w == "boom" {
        throw "bad completer";
    } else if w == "loop" {
        loop {}
    } else if w == "nx" {
        // The words after the cursor are passed too.
        ["nx-" + words[i + 1] + "-" + words.len()]
    } else {
        ()
    }
});
"#,
    )
    .unwrap();
    sh.expect("$ ");
    sh.send("plugin load ./comp.rhai; frob() { echo \"frob:$*\"; }; echo \"loaded $?\"\n");
    sh.expect("loaded 0\n");
    sh.expect("$ ");
    sh.send("frob al\t\n");
    sh.expect("frob:alpha\n");
    sh.expect("$ ");
    sh.send("frob --o\tx\n");
    sh.expect("frob:--opt=x\n");
    sh.expect("$ ");
    sh.send("frob cap\t\n");
    sh.expect("frob:captured\n");
    sh.expect("$ ");
    // Through an alias.
    sh.send("alias fr=frob; echo aliased\n");
    sh.expect("aliased\n");
    sh.expect("$ ");
    sh.send("fr al\t\n");
    sh.expect("frob:alpha\n");
    sh.expect("$ ");
    // In the middle of the line (Ctrl-B moves back).
    sh.send("frob x nx tail\x02\x02\x02\x02\x02\t\n");
    sh.expect("frob:x nx-tail-4 tail\n");
    sh.expect("$ ");
    // `()` gives the default completion (filenames).
    sh.send("frob x comp.r\t\n");
    sh.expect("frob:x comp.rhai\n");
    sh.expect("$ ");
    // Descriptions are shown after their candidates in the menu.
    sh.send("frob \t");
    sh.expect("beta");
    sh.expect("\x1b[90m-- the second\x1b[0m");
    sh.send("\x03");
    // The next prompt: input sent before it may be discarded.
    sh.expect("\x1b[?2004h");
    // An error is shown below the line, which is drawn again.
    sh.send("frob x boom\t");
    sh.expect("bad completer");
    sh.expect("x boom");
    sh.send("\x03");
    // The next prompt: input sent before it may be discarded.
    sh.expect("\x1b[?2004h");
    // A completer that runs too long is stopped.
    sh.send("frob x loop\t");
    sh.expect("took too long");
    sh.send("\x03");
    // The next prompt: input sent before it may be discarded.
    sh.expect("\x1b[?2004h");
    // The completer's command wasn't a job, so the terminal modes the shell
    // restores after a job that dies are not the editor's raw modes.
    sh.send("sh -c 'kill -9 $$'; stty -a; echo stty-done\n");
    let out = sh.expect("stty-done\n");
    // GNU's stty separates the settings with spaces, uutils' with newlines.
    let modes: Vec<&str> = out.split_whitespace().collect();
    assert!(modes.contains(&"icanon") && !modes.contains(&"-icanon"), "{out}");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// The Cobra plugin of the documentation, with a stand-in for a program
/// built with Cobra.
#[cfg(feature = "plugins")]
#[test]
fn cobra_completer() {
    use std::os::unix::fs::PermissionsExt;
    let mut sh = Pty::spawn_term("cobra", "vt100");
    let plugin = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/examples/cobra.rhai")).unwrap();
    std::fs::write(
        sh.path("cobra.rhai"),
        plugin + "sh::completer(\"frob\", Fn(\"cobra\"));\n",
    )
    .unwrap();
    std::fs::create_dir(sh.path("bin")).unwrap();
    let frob = sh.path("bin/frob");
    std::fs::write(
        &frob,
        r#"#!/bin/sh
[ "$1" = __complete ] || { echo "frob:$*"; exit; }
shift
case $#:$1 in
1:*) printf 'serve\tStart the server\nstatus\tShow the status\n:4\n' ;;
*:serve) printf -- '--port=\n--verbose\n:6\n' ;;
*) echo ':0' ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&frob, std::fs::Permissions::from_mode(0o755)).unwrap();
    sh.expect("$ ");
    sh.send("PATH=$HOME/bin:$PATH; plugin load ./cobra.rhai; echo \"loaded $?\"\n");
    sh.expect("loaded 0\n");
    sh.expect("$ ");
    sh.send("frob ser\t\n");
    sh.expect("frob:serve\n");
    sh.expect("$ ");
    // No space after `--port=` (flag 2).
    sh.send("frob serve --p\t80\n");
    sh.expect("frob:serve --port=80\n");
    sh.expect("$ ");
    // Filenames when the program gives nothing (flag 4 unset).
    sh.send("frob status cob\t\n");
    sh.expect("frob:status cobra.rhai\n");
    sh.expect("$ ");
    sh.send("frob \t\t");
    sh.expect("serve   \x1b[90m-- Start the server\x1b[0m");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// The bash-completion plugin of luish-std-plugins, with completion files
/// of its own (in `$HOME/.local/share/bash-completion/completions`, where
/// bash-completion looks too). Skipped without bash-completion.
#[cfg(feature = "plugins")]
#[test]
fn bash_completion_bridge() {
    if !std::path::Path::new("/usr/share/bash-completion/bash_completion").exists() {
        eprintln!("skipped: no bash-completion");
        return;
    }
    let mut sh = Pty::spawn_term("bashcomp", "vt100");
    let examples = concat!(env!("CARGO_MANIFEST_DIR"), "/luish-std-plugins/bash-completion");
    std::fs::create_dir(sh.path("bash-completion")).unwrap();
    for f in ["extension.rhai", "bridge.bash"] {
        std::fs::copy(format!("{examples}/{f}"), sh.path("bash-completion").join(f)).unwrap();
    }
    let completions = sh.path(".local/share/bash-completion/completions");
    std::fs::create_dir_all(&completions).unwrap();
    std::fs::write(
        completions.join("frob"),
        r#"_frob() {
    local cur prev words cword
    _init_completion -n = || return
    case $cur in
    --mode=*) COMPREPLY=($(compgen -W 'fast slow' -- "${cur#*=}")) ;;
    -*) COMPREPLY=($(compgen -W '--mode= --verbose' -- "$cur")); [[ $COMPREPLY == *= ]] && compopt -o nospace ;;
    *) COMPREPLY=($(compgen -W 'serve status' -- "$cur")) ;;
    esac
}
complete -F _frob frob
"#,
    )
    .unwrap();
    std::fs::write(
        completions.join("quiet"),
        "_quiet() { COMPREPLY=(); }\ncomplete -o default -F _quiet quiet\n",
    )
    .unwrap();
    std::fs::write(sh.path("notes.txt"), "").unwrap();
    std::fs::create_dir(sh.path("my dir")).unwrap();
    std::fs::write(sh.path("my dir/inner.txt"), "").unwrap();
    sh.expect("$ ");
    sh.send("plugin load ./bash-completion/; frob() { echo \"frob:$*\"; }; quiet() { echo \"quiet:$*\"; }; cat() { echo \"cat:${1#\"$HOME/\"}\"; }; echo \"loaded $?\"\n");
    sh.expect("loaded 0\n");
    sh.expect("$ ");
    sh.send("frob se\t\n");
    sh.expect("frob:serve\n");
    sh.expect("$ ");
    // No space after `--mode=`, then what follows the `=`.
    sh.send("frob --m\tf\t\n");
    sh.expect("frob:--mode=fast\n");
    sh.expect("$ ");
    // Filenames with `-o default` when the function gives nothing, and for
    // commands that bash-completion doesn't know.
    sh.send("quiet not\t\n");
    sh.expect("quiet:notes.txt\n");
    sh.expect("$ ");
    sh.send("echo not\t\n");
    sh.expect("notes.txt\n");
    sh.expect("$ ");
    // bash-completion's filenames (`cat` has `_longopt`), from `~` and with
    // a space: bash-completion quotes the word for compgen, and directories
    // get a `/`.
    sh.send("cat ~/my\\ d\tin\t\n");
    sh.expect("cat:my dir/inner.txt\n");
    sh.expect("$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// The completion plugin of luish-std-plugins, for git,, in a repository with a
/// modified file, an untracked one, a staged one and two branches. A
/// function `git` prints its arguments (the completer runs `command git`).
#[cfg(feature = "plugins")]
#[test]
fn git_completion() {
    let mut sh = Pty::spawn_term("gitcomp", "vt100");
    let repo = sh.path("repo");
    let git = |args: &[&str]| {
        let st = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(&repo)
            .env("HOME", sh.path(""))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?}");
    };
    std::fs::create_dir_all(repo.join("src/deep")).unwrap();
    git(&["init", "-q"]);
    std::fs::write(repo.join("changed.txt"), "a\n").unwrap();
    std::fs::write(repo.join("src/deep/staged.rs"), "a\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    git(&["branch", "topic"]);
    git(&["config", "alias.sw", "switch"]);
    std::fs::write(repo.join("changed.txt"), "b\n").unwrap();
    std::fs::write(repo.join("src/deep/staged.rs"), "b\n").unwrap();
    git(&["add", "src"]);
    std::fs::write(repo.join("untracked.txt"), "").unwrap();
    let plugin = concat!(env!("CARGO_MANIFEST_DIR"), "/luish-std-plugins/completion");
    sh.expect("$ ");
    sh.send(&format!(
        "plugin load {plugin}; git() {{ echo \"git:$*\"; }}; cd repo; echo \"loaded $?\"\n"
    ));
    sh.expect("loaded 0\n");
    sh.expect("$ ");
    // Commands, then branches.
    sh.send("git swi\tto\t\n");
    sh.expect("git:switch topic\n");
    sh.expect("$ ");
    // An alias completes as the command it stands for.
    sh.send("git sw to\t\n");
    sh.expect("git:sw topic\n");
    sh.expect("$ ");
    // `git add` offers modified and untracked files, but not staged ones.
    sh.send("git add \t\t");
    sh.expect("changed.txt");
    sh.expect("untracked.txt");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    // One directory at a time, then the staged file.
    sh.send("git restore --staged s\t\t\t\n");
    sh.expect("git:restore --staged src/deep/staged.rs\n");
    sh.expect("$ ");
    // The end of a range.
    sh.send("git log main..to\t\n");
    sh.expect("git:log main..topic\n");
    sh.expect("$ ");
    // Options, as git lists them.
    sh.send("git commit --ame\t\n");
    sh.expect("git:commit --amend\n");
    sh.expect("$ ");
    // The descriptions of commands.
    sh.send("git sta\t\t");
    sh.expect("Show the working tree status");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    // `-C` chooses the repository, through an alias with `~`.
    sh.send("cd; alias g='git -C ~/repo'; echo aliased\n");
    sh.expect("aliased\n");
    sh.expect("$ ");
    sh.send("g switch to\t\n");
    sh.expect("/repo switch topic\n");
    sh.expect("$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn histcmd_shlvl() {
    let mut sh = Pty::spawn("histcmd");
    sh.expect("$ ");
    sh.run("echo one");
    // `$HISTCMD` is the event number of the command being run.
    assert_has(&sh.run("echo h=$HISTCMD"), "h=2\n");
    // Not set in the environment, SHLVL starts at 1 in an interactive shell.
    assert_has(&sh.run("echo l=$SHLVL; env | grep ^SHLVL"), "l=1\nSHLVL=1\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn fc_history() {
    let mut sh = Pty::spawn("fc");
    std::fs::write(
        sh.path("ed.sh"),
        "sed 's/one/ONE/' \"$1\" > \"$1.new\" && mv \"$1.new\" \"$1\"\n",
    )
    .unwrap();
    sh.expect("$ ");
    sh.run("echo one");
    sh.run("echo two");
    // The `fc` command itself is left out.
    assert_eq!(sh.run("fc -l"), "fc -l\n1\techo one\n2\techo two\n$ ");
    // Re-running echoes the command and puts it in place of the `fc` entry.
    assert_eq!(sh.run("fc -s two=TWO 2"), "fc -s two=TWO 2\necho TWO\nTWO\n$ ");
    assert_eq!(sh.run("fc -ln -2"), "fc -ln -2\n\tfc -l\n\techo TWO\n$ ");
    assert_eq!(sh.run("fc -lr 1 2"), "fc -lr 1 2\n2\techo two\n1\techo one\n$ ");
    // A string names the newest command starting with it.
    assert_eq!(
        sh.run("fc -l ech"),
        "fc -l ech\n4\techo TWO\n5\tfc -ln -2\n6\tfc -lr 1 2\n$ "
    );
    assert_eq!(sh.run("fc -e 'sh ed.sh' 1"), "fc -e 'sh ed.sh' 1\necho ONE\nONE\n$ ");
    assert_eq!(sh.run("fc -l -1"), "fc -l -1\n8\techo ONE\n$ ");
    // A failing editor suppresses the re-execution.
    sh.run("fc -e false");
    assert_has(&sh.run("echo st=$?"), "st=1\n");
    // Lines after the first of a multi-line command are indented.
    sh.send("for i in 1 2\ndo echo $i\ndone\n");
    sh.expect("\n2\n$ ");
    assert_eq!(
        sh.run("fc -l -1"),
        "fc -l -1\n12\tfor i in 1 2\n\tdo echo $i\n\tdone\n$ "
    );
    let err = sh.run("fc -l nosuch");
    assert_has(&err, "fc: history pattern not found: nosuch\n");
    assert_has(&sh.run("echo st=$?"), "st=2\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn history_expansion() {
    let mut sh = Pty::spawn("bang");
    sh.expect("$ ");
    // Off by default, as `!` isn't special in dash.
    sh.run("echo one two three");
    assert_eq!(sh.run("echo !!"), "echo !!\n!!\n$ ");
    sh.run("setopt history.expand");
    sh.run("echo one two three");
    // The expanded line is echoed, run, and added to the history.
    assert_eq!(sh.run("!!"), "!!\necho one two three\none two three\n$ ");
    assert_eq!(sh.run("echo !$ !^"), "echo !$ !^\necho three one\nthree one\n$ ");
    assert_eq!(sh.run("^one^ONE"), "^one^ONE\necho three ONE\nthree ONE\n$ ");
    // (`!!` repeated the command before, which isn't added again.)
    assert_eq!(sh.run("fc -l -2"), "fc -l -2\n5\techo three one\n6\techo three ONE\n$ ");
    // Not in single quotes, nor for `$!` or `[!...]`.
    assert_eq!(sh.run("echo 'a!!' $! [!.]"), "echo 'a!!' $! [!.]\na!! [!.]\n$ ");
    // A reference that fails drops the command, which isn't recorded, and
    // leaves `$?` alone.
    sh.run("false");
    let err = sh.run("echo !nosuch");
    assert_has(&err, "!nosuch: event not found\n");
    assert_has(&sh.run("echo st=$?"), "st=1\n");
    assert_eq!(sh.run("fc -ln -1"), "fc -ln -1\n\techo st=$?\n$ ");
    // `:p` prints and records without running.
    assert_eq!(sh.run("echo x !-2:0:p"), "echo x !-2:0:p\necho x echo\n$ ");
    assert_eq!(sh.run("fc -ln -1"), "fc -ln -1\n\techo x echo\n$ ");
    // Each line of a command is expanded as it is read, following quotes
    // and here-documents from the lines before.
    let lines = |sh: &mut Pty, lines: &[&str]| {
        for l in &lines[..lines.len() - 1] {
            sh.send(&format!("{l}\n"));
            sh.expect("\n> ");
        }
        sh.run(lines[lines.len() - 1])
    };
    assert_eq!(
        lines(&mut sh, &["echo 'a", "!!' \"", "!!\""]),
        "!!\"\nfc -ln -1\"\na\n!! \nfc -ln -1\n$ "
    );
    assert_eq!(lines(&mut sh, &["cat <<E", "!!", "E"]), "E\n!!\n$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);

    // With `history.verify`, the expansion is edited before it runs (on a
    // terminal the line editor supports).
    let mut sh = Pty::spawn_term("bang-verify", "vt100");
    sh.expect("$ ");
    sh.send("setopt history.expand history.verify\n");
    sh.expect("\x1b[?2004h");
    sh.send("echo verified\n");
    sh.expect("\nverified\n");
    sh.expect("\x1b[?2004h");
    sh.send("!! again\n");
    sh.expect("echo\x1b[0m verified again");
    sh.send("\n");
    sh.expect("\nverified again\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn history_file() {
    let mut sh = Pty::spawn("histfile");
    sh.expect("$ ");
    sh.run("setopt hist_ignore_space hist_reduce_blanks");
    sh.run("echo   one   'a  b'");
    sh.run(" echo secret");
    // A command starting with a space is replaced by the next one.
    assert_eq!(sh.run("fc -l -1"), "fc -l -1\n2\techo one 'a  b'\n$ ");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
    // By default the file is in $XDG_STATE_HOME, in zsh's format.
    let text = std::fs::read_to_string(sh.path(".local/state/luish/history")).unwrap();
    let commands: Vec<&str> = text.lines().map(|l| l.split_once(';').unwrap().1).collect();
    assert_eq!(
        commands,
        [
            "setopt hist_ignore_space hist_reduce_blanks",
            "echo one 'a  b'",
            "fc -l -1",
            "exit 0"
        ]
    );
    assert!(text.starts_with(": 1") && text.contains(":0;setopt"), "{text}");

    // HISTFILE set in a startup file is read after it.
    std::fs::create_dir_all(sh.path(".config/luish")).unwrap();
    std::fs::write(sh.path(".config/luish/luishrc"), "HISTFILE=~/h\n").unwrap();
    std::fs::write(sh.path("h"), ": 1790000000:0;echo from\\\nzsh\n").unwrap();
    let mut sh2 = Pty::spawn_beside(&sh);
    sh2.expect("$ ");
    assert_eq!(sh2.run("fc -l"), "fc -l\n1\techo from\n\tzsh\n$ ");
}

#[test]
fn share_history() {
    let mut a = Pty::spawn("share");
    a.expect("$ ");
    a.run("setopt share_history");
    std::fs::create_dir_all(a.path(".config/luish")).unwrap();
    std::fs::write(a.path(".config/luish/luishrc"), "setopt share_history\n").unwrap();
    let mut b = Pty::spawn_beside(&a);
    b.expect("$ ");
    a.run("echo from a");
    // b reads it before its next prompt.
    b.run("");
    assert_eq!(b.run("fc -l"), "fc -l\n1\tsetopt share_history\n2\techo from a\n$ ");
    a.run(": another");
    b.run(": from b");
    assert_eq!(
        a.run("fc -ln"),
        "fc -ln\n\tsetopt share_history\n\techo from a\n\tfc -l\n\t: another\n\t: from b\n$ "
    );
}

#[test]
fn path_cache() {
    use std::os::unix::fs::PermissionsExt;
    let mut sh = Pty::spawn("pathcache");
    let install = |sh: &Pty, dir: &str| {
        let file = sh.path(dir).join("tool");
        std::fs::write(&file, format!("#!/bin/sh\necho from-{dir}\n")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    std::fs::create_dir(sh.path("a")).unwrap();
    std::fs::create_dir(sh.path("b")).unwrap();
    install(&sh, "b");
    sh.expect("$ ");
    sh.run("PATH=$HOME/a:$HOME/b:$PATH");
    assert_has(&sh.run("tool"), "from-b\n");
    // The shell looked the command up itself (not in the forked process),
    // so it remembers it.
    assert_has(&sh.run("hash"), &format!("{}\n", sh.path("b/tool").display()));
    // A command installed from outside, earlier in PATH, is found by the
    // next command line, although the remembered one is still there.
    install(&sh, "a");
    assert_has(&sh.run("tool"), "from-a\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn line_editor_keys() {
    let mut sh = Pty::spawn_term("keys", "vt100");
    sh.expect("$ ");
    let run = |sh: &mut Pty, keys: &str, want: &str| {
        sh.send(keys);
        sh.expect(want);
        // The next prompt: input sent before it may be discarded.
        sh.expect("\x1b[?2004h");
    };
    run(&mut sh, "echo first\n", "\nfirst\n");
    run(&mut sh, ": other\n", "\n");
    run(&mut sh, "echo second\n", "\nsecond\n");
    // Up searches for the text before the cursor, skipping lines equal to
    // the one shown; Down past the newest match gives back what was typed.
    run(&mut sh, "ec\x1b[A\x1b[A\n", "\nfirst\n");
    run(&mut sh, "echo s\x1b[A\x1b[B\x1b[Bx\n", "\nsx\n");
    // Ctrl-W stops at characters not in WORDCHARS (zsh's default has `/`).
    run(&mut sh, "echo x /tmp/aa/bb\x17y\n", "\nx y\n");
    run(&mut sh, "WORDCHARS=\n", "\n");
    run(&mut sh, "echo /tmp/aa/bb\x17cc\n", "\n/tmp/aa/cc\n");
    // Consecutive kills are yanked together; Alt-B moves back a word.
    run(&mut sh, "echo /tmp/aa/bb\x17\x17-\x19\x1bb\x1bb+\n", "\n/tmp/-+aa/bb\n");
    // Alt-. inserts the last word of the previous command, and again that
    // of the one before.
    run(&mut sh, ": lastword1\n", "\n");
    run(&mut sh, ": lastword2\n", "\n");
    run(&mut sh, "echo \x1b.\x1b.\n", "\nlastword1\n");
    // Ctrl-O runs the line and starts the next one with the entry after it.
    run(&mut sh, "echo o1\n", "\no1\n");
    run(&mut sh, "echo o2\n", "\no2\n");
    run(&mut sh, "echo o\x1b[A\x1b[A\x0f", "\no1\n");
    run(&mut sh, "\n", "\no2\n");
    // bindkey changes them.
    run(&mut sh, "bindkey '^W' kill-whole-line\n", "\n");
    run(&mut sh, "echo gone\x17echo kept\n", "\nkept\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn autosuggestions() {
    let mut sh = Pty::spawn_term("suggest", "vt100");
    sh.expect("$ ");
    let run = |sh: &mut Pty, keys: &str, want: &str| {
        sh.send(keys);
        sh.expect(want);
        sh.expect("\x1b[?2004h");
    };
    run(&mut sh, "echo suggested-text more\n", "\nsuggested-text more\n");
    // Off by default.
    run(&mut sh, "echo sug\x1b[Cx\n", "\nsugx\n");
    run(&mut sh, "setopt autosuggest\n", "\n");
    // The rest of the newest entry that starts with the line, in grey;
    // Right accepts it.
    sh.send("echo sug");
    sh.expect("\x1b[90mx\x1b[0m");
    sh.send("g");
    sh.expect("\x1b[90mested-text more\x1b[0m");
    run(&mut sh, "\x1b[C\n", "\nsuggested-text more\n");
    // Alt-F accepts a word of it.
    run(&mut sh, "echo s\x1bfx\n", "\nsuggested-text x\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn help_builtin() {
    let mut sh = Pty::spawn("help");
    sh.expect("$ ");
    // `help` is a built-in only in interactive shells, but also in their
    // subshells (here, a pipeline's).
    assert_has(&sh.run("type help"), "help is a shell builtin");
    assert_has(&sh.run("help true"), "Do nothing, successfully");
    assert_has(&sh.run("help false | cat"), "Do nothing, unsuccessfully");
    // In colour on a terminal, as plain text otherwise.
    assert_has(&sh.run("help pwd"), "\x1b[32mpwd\x1b[0m [-L | -P]");
    assert_has(&sh.run("help pwd | cat"), "\npwd [-L | -P]");
    assert_has(&sh.run("NO_COLOR=1 help pwd"), "\npwd [-L | -P]");
    let out = sh.run("help nosuch; echo \"status $?\"");
    assert_has(&out, "help: no help for nosuch");
    assert_has(&out, "status 1");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn print_builtin() {
    let mut sh = Pty::spawn("print");
    sh.expect("$ ");
    // `print` is a built-in only in interactive shells.
    assert_has(&sh.run("type print"), "print is a shell builtin");
    assert_has(&sh.run("print -P '%%|%(?.ok.bad)'"), "%|ok\n");
    // `-s` adds an entry after the command's own.
    sh.run("print -s echo pushed");
    assert_eq!(
        sh.run("fc -l -2"),
        "fc -l -2\n3\tprint -s echo pushed\n4\techo pushed\n$ "
    );
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);

    // `-z` pushes text that the next command lines start with, the last
    // pushed first (on a terminal the line editor supports).
    let mut sh = Pty::spawn_term("print-z", "vt100");
    sh.expect("$ ");
    sh.send("print -z echo first; print -z echo second\n");
    sh.expect("second");
    sh.send("\n");
    sh.expect("\nsecond\n");
    sh.expect("first");
    sh.send("\n");
    sh.expect("\nfirst\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn plugin_builtin() {
    let mut sh = Pty::spawn("plugin");
    sh.expect("$ ");
    // Like `help`, `plugin` is a built-in only in interactive shells.
    assert_has(&sh.run("type plugin"), "plugin is a shell builtin");
    let out = sh.run("plugin unload nosuch; echo \"status $?\"");
    assert_has(&out, "plugin: nosuch: not loaded");
    assert_has(&out, "status 1");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// What `sh::capture_cached` kept of a program is dropped when a command
/// line names the program (it may have installed something), but not
/// for another command.
#[cfg(feature = "plugins")]
#[test]
fn capture_cached_invalidation() {
    let mut sh = Pty::spawn("capture-cached");
    sh.expect("$ ");
    std::fs::write(
        sh.path("cc.rhai"),
        r#"sh::builtin("cc", |argv| { print("runs=" + sh::capture_cached(60, ["sh", "-c", "echo >> runs; wc -l < runs"]).out); });"#,
    )
    .unwrap();
    sh.run("plugin load ~/cc.rhai");
    assert_has(&sh.run("cc"), "runs=1");
    assert_has(&sh.run("cc"), "runs=1");
    sh.run("true");
    assert_has(&sh.run("cc"), "runs=1");
    sh.run("FOO=1 /bin/sh -c :");
    assert_has(&sh.run("cc"), "runs=2");
    assert_has(&sh.run("cc"), "runs=2");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

/// The first run: with an empty configuration directory, the shell offers
/// to write `config.toml` before reading it, in a menu.
#[cfg(feature = "plugins")]
#[test]
fn first_run() {
    let dir = Pty::new_dir("firstrun");
    let config = dir.join(".config/luish/config.toml");
    let (down, enter) = ("\x1b[B", "\r");
    // Ctrl-C (like the last item) leaves the directory alone, so the next
    // shell asks again.
    let mut sh = Pty::spawn_at(dir.clone(), "vt100", true, None);
    sh.expect("Welcome to luish!\n\nThe configuration directory, ~/.config/luish, is empty.\n");
    sh.expect("> 1. Write the recommended configuration");
    sh.expect("4. Just start for now and ask again next time\n");
    sh.send("\x03");
    sh.expect("$ ");
    assert!(!dir.join(".config/luish").exists());
    sh.send("exit\n");
    assert_eq!(sh.exit_status(), 0);

    // A personal plugin (after a bad one): a minimal configuration with
    // only it, added as `plugin add` would, and loaded.
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    std::fs::create_dir(dir.join("mine")).unwrap();
    std::fs::write(dir.join("mine/init.lsh"), "echo mine loaded\n").unwrap();
    let mut sh2 = Pty::spawn_at(dir.clone(), "vt100", false, None);
    sh2.expect("4. Just start");
    sh2.send(&format!("{down}{down}{enter}"));
    sh2.expect("> 3. Add a personal plugin (for more advanced users)");
    sh2.expect("Plugin (a GitHub repository, as OWNER/REPO or its URL, a git URL or a path): ");
    sh2.send("./nope\n");
    sh2.expect("./nope: no such file or directory\n");
    sh2.expect("4. Just start");
    // Up from the first item goes round to the last; 3 chooses the third.
    sh2.send("\x1b[A");
    sh2.expect("> 4. Just start");
    sh2.send("3");
    sh2.expect("Plugin (");
    sh2.send("./mine\n");
    sh2.expect("Wrote ~/.config/luish/config.toml\n");
    sh2.expect("mine loaded\n");
    let text = std::fs::read_to_string(&config).unwrap();
    assert_eq!(
        text,
        format!("[plugins.enabled]\nmine = {{ path = \"{}/mine\" }}\n", dir.display())
    );
    sh2.send("exit\n");
    assert_eq!(sh2.exit_status(), 0);

    // An empty configuration: everything commented out.
    std::fs::remove_dir_all(dir.join(".config/luish")).unwrap();
    let mut sh3 = Pty::spawn_at(dir.clone(), "vt100", false, None);
    sh3.expect("4. Just start");
    sh3.send("2");
    sh3.expect("Wrote ~/.config/luish/config.toml\n");
    sh3.expect("$ ");
    let text = std::fs::read_to_string(&config).unwrap();
    assert_has(&text, "\n# [options.editor]\n# autosuggest = true\n");
    assert_has(&text, "\n# std.completion = \"*\"");
    sh3.send("exit\n");
    assert_eq!(sh3.exit_status(), 0);

    // Now the directory has a file: no questions.
    let mut sh5 = Pty::spawn_beside(&sh);
    assert_eq!(sh5.expect("$ "), "$ ");
    sh5.send("exit\n");
    assert_eq!(sh5.exit_status(), 0);

    // The recommended configuration, chosen by number on a terminal that
    // can't move the cursor, where the standard plugins can't be fetched
    // (there is no git).
    std::fs::remove_dir_all(dir.join(".config/luish")).unwrap();
    let empty = dir.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let mut sh4 = Pty::spawn_at(dir.clone(), "dumb", false, Some(empty.to_str().unwrap()));
    sh4.expect("  4. Just start for now and ask again next time\nChoose 1-4 [1]: ");
    sh4.send("5\n");
    sh4.expect("Choose 1-4 [1]: ");
    sh4.send("\n");
    sh4.expect("Wrote ~/.config/luish/config.toml\n");
    sh4.expect("Run plugin sync once the plugins can be fetched\n");
    sh4.expect("$ ");
    let text = std::fs::read_to_string(&config).unwrap();
    assert_has(&text, "\n[options.editor]\nautosuggest = true\n");
    assert_has(&text, "\n[options.prompt]\npercent = true\n");
    assert_has(
        &text,
        "\n[options.history]\nexpand = true\n# file = \"~/.local/state/luish/history\"",
    );
    assert_has(&text, "\n[plugins.enabled]\nstd.completion = \"*\"");
    sh4.send("exit\n");
    assert_eq!(sh4.exit_status(), 0);
}

/// The first run without plugins: no personal plugin item, and the
/// recommended configuration enables no plugins.
#[cfg(not(feature = "plugins"))]
#[test]
fn first_run() {
    let dir = Pty::new_dir("firstrun");
    let config = dir.join(".config/luish/config.toml");
    let mut sh = Pty::spawn_at(dir.clone(), "dumb", false, None);
    sh.expect("  2. Write an empty configuration (so this question will not be asked again)\n");
    sh.expect("  3. Just start for now and ask again next time\nChoose 1-3 [1]: ");
    sh.send("\n");
    sh.expect("Wrote ~/.config/luish/config.toml\n");
    sh.expect("$ ");
    let text = std::fs::read_to_string(&config).unwrap();
    assert_has(&text, "\n[options.editor]\nautosuggest = true\n");
    sh.send("exit\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn remote_mode() {
    let mut sh = Pty::spawn_remote("remote", "dumb");
    sh.expect("$ ");
    // The commands run on the server's pty, not the client's.
    sh.send("tty; tty <&2 | cat; echo here\n");
    sh.expect("here\n");
    sh.expect("$ ");
    assert_has(&sh.run("echo $((6 * 7))"), "42\n");
    // Job control on the server's pty.
    sh.send("sleep 30\n");
    sh.wait_for_remote_procs(&["sleep"]);
    sh.send("\x1a");
    assert_has(&sh.expect("\n$ "), "[1] + Stopped                    sleep 30\n");
    assert_has(&sh.run("echo st=$?"), "st=148\n");
    sh.send("fg\n");
    sh.wait_for_remote_procs(&["sleep"]);
    sh.send("\x03");
    sh.expect("\n$ ");
    assert_has(&sh.run("echo st=$?"), "st=130\n");
    // A program reading the terminal.
    sh.send("cat\n");
    sh.wait_for_remote_procs(&["cat"]);
    sh.send("typed\n\x04");
    assert_has(&sh.expect("\n$ "), "typed\ntyped\n");
    // The window's size, as the client's terminal changes: before a command
    // line is sent, and while a command runs.
    sh.resize(100, 40);
    assert_has(&sh.run("stty size"), "40 100\n");
    sh.send("cat >/dev/null; stty size\n");
    sh.wait_for_remote_procs(&["cat"]);
    sh.resize(90, 30);
    sh.send("\x04");
    assert_has(&sh.expect("\n$ "), "30 90\n");
    // Keys typed while a command runs start the next line.
    sh.send("cat >/dev/null; echo done\n");
    sh.wait_for_remote_procs(&["cat"]);
    sh.send("\x04echo typed-ahead\n");
    sh.expect("done\n");
    sh.expect("\ntyped-ahead\n$ ");
    // Lines sent together: the second comes while the client is passing
    // keys to the pty, or while the request for it is on its way. (Each
    // prints what isn't in its text: with `TERM=dumb`, the terminal echoes
    // keys where they come, maybe before the prompt.)
    for i in 100..105 {
        sh.send(&format!("true\necho second-$(({i}))\n"));
        sh.expect(&format!("second-{i}\n"));
    }
    // Keys typed once a command's output has come but before the prompt:
    // after the server has taken those left on its pty.
    for i in 100..105 {
        sh.send(&format!("echo output-$(({i}))\n"));
        sh.expect(&format!("output-{i}\n"));
        sh.send(&format!("echo next-$(({i}))\n"));
        sh.expect(&format!("next-{i}\n"));
    }
    // A program the shell `exec`s has the terminal until it exits.
    sh.send("exec /bin/sh -c 'read x; echo got-$x; exit 3'\n");
    sh.wait_for_remote_procs(&["sh"]);
    sh.send("abc\n");
    sh.expect("got-abc\n");
    assert_eq!(sh.exit_status(), 3);
}

#[test]
fn remote_editing() {
    let mut sh = Pty::spawn_remote("remote-edit", "vt100");
    std::fs::create_dir(sh.path("somedir")).unwrap();
    std::fs::write(sh.path("somedir/afile"), "found it\n").unwrap();
    sh.expect("$ ");
    // Tab completes on the server, from its directory.
    sh.send("cat som\t\t\n");
    sh.expect("found it\n");
    sh.expect("\x1b[?2004h");
    // A second Tab opens the menu, drawn by the client.
    sh.send("zqfa() { :; }; zqfb() { :; }\n");
    sh.expect("\x1b[?2004h");
    sh.send("zqf\t\t");
    sh.expect("zqfa  zqfb");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    // The history is the server's, recalled by the client.
    sh.send("echo from-history\n");
    sh.expect("from-history\n");
    sh.expect("\x1b[?2004h");
    sh.send("\x1b[A\n");
    sh.expect("from-history\n");
    sh.expect("\x1b[?2004h");
    sh.send("fc -l -1\n");
    sh.expect("echo from-history\n");
    sh.expect("\x1b[?2004h");
    // The highlighter knows the commands in the server's `PATH`, and asks it
    // about files.
    sh.send("mkdir bin; printf '#!/bin/sh\\n' >bin/zqtool; chmod +x bin/zqtool; PATH=$PWD/bin:$PATH\n");
    sh.expect("\x1b[?2004h");
    sh.send("zqtool");
    sh.expect("\x1b[32mzqtool\x1b[0m");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    sh.send("setopt highlight.paths; : >afile\n");
    sh.expect("\x1b[?2004h");
    sh.send("ls afile ~/af");
    sh.send("x");
    sh.expect("\x1b[4mafile\x1b[0m \x1b[34m~\x1b[0m/afx");
    sh.send("\x03");
    sh.expect("\x1b[?2004h");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn remote_environment() {
    // The server starts with an environment of its own (as over ssh), and
    // takes the client's terminal variables, and its locale where it has
    // none.
    let dir = Pty::new_dir("remote-env");
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    std::fs::write(dir.join(".config/luish/luishrc"), "").unwrap();
    let home = format!("HOME={}", dir.display());
    let path = format!("PATH={}", std::env::var("PATH").unwrap_or_default());
    let args = [
        "--remote",
        "env",
        "-i",
        &home,
        &path,
        "PS1=$ ",
        "LUISH_BACKGROUND=dark",
        "LANG=C.UTF-8",
        "COLORTERM=server",
        env!("CARGO_BIN_EXE_luish"),
        "--serve",
    ];
    let extra = [
        "LUISH_BACKGROUND=dark",
        "COLORTERM=truecolor",
        "TERM_PROGRAM=testterm",
        "LANG=POSIX",
        "LC_CTYPE=C",
        "OTHER=client",
    ];
    let mut sh = Pty::spawn_args(dir, "dumb", true, None, &extra, &args);
    sh.expect("$ ");
    let got = sh.run("echo \"v-$TERM $COLORTERM $TERM_PROGRAM $LANG $LC_ALL $LC_CTYPE ${OTHER-unset}\"");
    assert_has(&got, "v-dumb truecolor testterm C.UTF-8 C C unset\n");
    sh.send("exit 0\n");
    assert_eq!(sh.exit_status(), 0);
}

#[test]
fn remote_ssh() {
    // `--ssh` with a stand-in for ssh, which runs the command as ssh does
    // (with the user's shell), and luish only where `--luish-path` says.
    let dir = Pty::new_dir("remote-ssh");
    std::fs::create_dir_all(dir.join(".config/luish")).unwrap();
    std::fs::write(dir.join(".config/luish/luishrc"), "").unwrap();
    std::fs::create_dir(dir.join("bin")).unwrap();
    let ssh = dir.join("bin/ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >\"$HOME/ssh-args\"\nshift 4\nexec /bin/sh -c \"$*\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&ssh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_luish"), dir.join("server")).unwrap();
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let args = ["--ssh", "-p", "2222", "--luish-path=~/server", "myhost"];
    let mut sh = Pty::spawn_args(dir, "dumb", true, Some(&path), &["LUISH_BACKGROUND=dark"], &args);
    sh.expect("$ ");
    assert_has(&sh.run("echo x-$((6 * 7))"), "x-42\n");
    let sent = std::fs::read_to_string(sh.path("ssh-args")).unwrap();
    assert_eq!(sent, "-T\n-p\n2222\nmyhost\n~/server\n--serve\n");
    sh.send("exit 4\n");
    assert_eq!(sh.exit_status(), 4);
}
