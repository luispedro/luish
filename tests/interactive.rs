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
}

impl Pty {
    fn spawn(name: &str) -> Pty {
        Pty::spawn_term(name, "dumb")
    }

    fn spawn_term(name: &str, term: &str) -> Pty {
        let dir = std::env::temp_dir().join(format!("luish-pty-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Pty::spawn_at(dir, term, true)
    }

    /// Another shell in the directory (and `$HOME`) of `other`.
    fn spawn_beside(other: &Pty) -> Pty {
        Pty::spawn_at(other.dir.clone(), "dumb", false)
    }

    fn spawn_at(dir: PathBuf, term: &str, owns_dir: bool) -> Pty {
        let shell = CString::new(env!("CARGO_BIN_EXE_luish")).unwrap();
        let argv = [CString::new("luish").unwrap(), CString::new("-i").unwrap()];
        let env = [
            format!("PATH={}", std::env::var("PATH").unwrap_or_default()),
            format!("HOME={}", dir.display()),
            "PS1=$ ".to_string(),
            format!("TERM={term}"),
            "LC_ALL=C".to_string(),
        ];
        let env: Vec<CString> = env.into_iter().map(|e| CString::new(e).unwrap()).collect();
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
            self.out.extend(buf[..n as usize].iter().filter(|&&c| c != b'\r'));
        }
        true
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
fn syntax_highlighting() {
    let mut sh = Pty::spawn_term("highlight", "vt100");
    sh.expect("$ ");
    // A reserved word, a string, and an unknown command, in the default colours.
    sh.send("if nosuchcommand 'x'");
    sh.expect("\x1b[1;34mif\x1b[0m \x1b[1;31mnosuchcommand\x1b[0m \x1b[33m'x'\x1b[0m");
    sh.send("\x03");
    sh.expect("$ ");
    // A set and an unset variable.
    sh.send("echo $PWD $NOSUCH ");
    sh.expect("\x1b[36m$PWD\x1b[0m \x1b[2;36m$NOSUCH\x1b[0m ");
    sh.send("\x03");
    sh.expect("$ ");
    // $LUISH_HIGHLIGHT overrides a class; the second line continues a quote.
    sh.send("LUISH_HIGHLIGHT='string=4'\n");
    sh.expect("$ ");
    sh.send("echo 'a\n");
    sh.expect("> ");
    sh.send("b' x");
    sh.expect("\x1b[4mb'\x1b[0m x");
    sh.send("\x03");
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
        let r = sh::capture("echo captured");
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
    assert!(out.contains(" icanon") && !out.contains("-icanon"), "{out}");
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

/// The git-completion plugin of luish-std-plugins, in a repository with a
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
    let plugin = concat!(env!("CARGO_MANIFEST_DIR"), "/luish-std-plugins/git-completion.rhai");
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
    let out = sh.run("help nosuch; echo \"status $?\"");
    assert_has(&out, "help: no help for nosuch");
    assert_has(&out, "status 1");
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
