//! Differential tests: every `tests/cases/**/*.sh` is run under luish and
//! under dash, and the results must match.
//!
//! - stdout and the exit status must be identical;
//! - stderr must be empty for luish iff it is empty for dash (error messages
//!   differ between shells), or identical if the script contains the line
//!   `# stderr: exact`;
//! - if `NAME.expected` exists, luish's stdout is compared with it instead
//!   and dash is not run (and `NAME.status`, if present, holds the expected
//!   exit status);
//! - if `NAME.stdin` exists, it is fed to the script's standard input;
//! - if the script contains the line `# reference: zsh`, it is compared
//!   with `zsh --emulate sh` (from pixi) instead of dash, for behaviour
//!   where luish follows zsh (see `DEVIATIONS.md`).
//!
//! Plugin cases (`tests/plugins/*.sh`, only with the `plugins` feature)
//! can't run under dash, so each has a `NAME.expected`, and stderr must be
//! empty or match `NAME.stderr`.
//!
//! Scripts run in a fresh temporary directory (which is also `$HOME`),
//! with `$SH` set to the shell under test. Set `LUISH_CASE` to a substring
//! to run only matching cases.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, PartialEq)]
struct Outcome {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status: String,
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "sh") {
            out.push(p);
        }
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// The shell a case is compared with, and the arguments that go before the
/// script.
struct Reference<'a> {
    name: &'static str,
    path: Option<&'a Path>,
    args: &'static [&'static str],
}

/// `id` names the directory, which must be the same for both shells (it
/// can appear in the output).
fn run(shell: &Path, args: &[&str], script: &Path, id: &str) -> Outcome {
    let dir = std::env::temp_dir().join(format!("luish-test-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let name = script.file_name().unwrap();
    std::fs::copy(script, dir.join(name)).unwrap();
    let stdin = match std::fs::File::open(script.with_extension("stdin")) {
        Ok(f) => Stdio::from(f),
        Err(_) => Stdio::null(),
    };
    let out_path = dir.join(".stdout");
    let err_path = dir.join(".stderr");
    let mut child = Command::new(shell)
        .args(args)
        .arg(name)
        .current_dir(&dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &dir)
        .env("LC_ALL", "C")
        .env("SH", shell)
        .stdin(stdin)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", shell.display()));
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&dir);
            return Outcome {
                stdout: Vec::new(),
                stderr: Vec::new(),
                status: "timeout".into(),
            };
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let stdout = std::fs::read(&out_path).unwrap();
    let stderr = std::fs::read(&err_path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    use std::os::unix::process::ExitStatusExt;
    let status = match (status.code(), status.signal()) {
        (Some(c), _) => c.to_string(),
        (None, Some(s)) => format!("signal {s}"),
        _ => "unknown".into(),
    };
    Outcome { stdout, stderr, status }
}

fn show(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    if s.is_empty() {
        "(empty)\n".into()
    } else {
        s.lines().map(|l| format!("    |{l}\n")).collect()
    }
}

/// `plugin_case`: stderr must match `NAME.stderr`, or be empty.
fn check(luish: &Path, refs: &[Reference], script: &Path, id: &str, plugin_case: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(script).unwrap_or_default();
    let exact_stderr = text.lines().any(|l| l.trim() == "# stderr: exact");
    let reference = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("# reference: "))
        .unwrap_or("dash");
    let got = run(luish, &[], script, id);
    let expected_file = script.with_extension("expected");
    let stderr_file = script.with_extension("stderr");
    let exact_stderr = exact_stderr || plugin_case;
    let want = if expected_file.exists() {
        let status = std::fs::read_to_string(script.with_extension("status"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "0".into());
        Outcome {
            stdout: std::fs::read(&expected_file).unwrap(),
            stderr: match std::fs::read(&stderr_file) {
                Ok(e) => e,
                Err(_) if plugin_case => Vec::new(),
                Err(_) => got.stderr.clone(),
            },
            status,
        }
    } else {
        let Some(r) = refs.iter().find(|r| r.name == reference) else {
            return Err(format!("  unknown reference shell {reference}\n"));
        };
        let Some(path) = r.path else {
            return Err(format!(
                "  {reference} is required for this case (run the tests through pixi)\n"
            ));
        };
        run(path, r.args, script, id)
    };
    let mut problems = String::new();
    if got.stdout != want.stdout {
        problems += &format!(
            "  stdout differs:\n  luish:\n{}  expected:\n{}",
            show(&got.stdout),
            show(&want.stdout)
        );
    }
    if got.status != want.status {
        problems += &format!("  exit status: luish {}, expected {}\n", got.status, want.status);
    }
    let stderr_ok = if exact_stderr {
        got.stderr == want.stderr
    } else {
        got.stderr.is_empty() == want.stderr.is_empty()
    };
    if !stderr_ok {
        problems += &format!(
            "  stderr differs:\n  luish:\n{}  expected:\n{}",
            show(&got.stderr),
            show(&want.stderr)
        );
    }
    if problems.is_empty() { Ok(()) } else { Err(problems) }
}

#[test]
fn differential() {
    run_cases("tests/cases", false);
}

#[cfg(feature = "plugins")]
#[test]
fn plugins() {
    run_cases("tests/plugins", true);
}

fn run_cases(dir: &str, plugin_cases: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut cases = Vec::new();
    collect(&root, &mut cases);
    cases.sort();
    if let Ok(filter) = std::env::var("LUISH_CASE") {
        cases.retain(|c| c.to_string_lossy().contains(&filter));
    }
    let luish = PathBuf::from(env!("CARGO_BIN_EXE_luish"));
    let dash = find_in_path("dash");
    let zsh = find_in_path("zsh");
    let refs = [
        Reference {
            name: "dash",
            path: dash.as_deref(),
            args: &[],
        },
        Reference {
            name: "zsh",
            path: zsh.as_deref(),
            args: &["--emulate", "sh"],
        },
    ];
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(case) = cases.get(i) else { break };
                    // Unique across the tests in this process, which run in parallel.
                    let id = format!("{}{i}", if plugin_cases { "p" } else { "" });
                    if let Err(msg) = check(&luish, &refs, case, &id, plugin_cases) {
                        let rel = case.strip_prefix(&root).unwrap_or(case);
                        failures.lock().unwrap().push(format!("{}:\n{msg}", rel.display()));
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    if !failures.is_empty() {
        panic!(
            "{} of {} cases failed:\n\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n")
        );
    }
    let what = if plugin_cases { "plugin" } else { "differential" };
    eprintln!("{} {what} cases passed", cases.len());
}
