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
}

impl Pty {
    fn spawn(name: &str) -> Pty {
        Pty::spawn_term(name, "dumb")
    }

    fn spawn_term(name: &str, term: &str) -> Pty {
        let dir = std::env::temp_dir().join(format!("luish-pty-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
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
        let _ = std::fs::remove_dir_all(&self.dir);
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
    // A second tab lists the candidates.
    sh.send("echo alp\t\t");
    sh.expect("alpha1  alpha2");
    sh.send("\x03");
    // The new prompt (not the line redrawn under the list).
    sh.expect("\x1b[K$ ");
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
