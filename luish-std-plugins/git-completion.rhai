// Completion for git: its commands (with descriptions) and aliases, the
// options of its commands (which git lists itself), and, depending on the
// command, branches, tags, remotes, stashes, worktrees and the files that
// the command can act on (modified files for `git add`, staged ones for
// `git restore --staged`, ...).
//
// git runs with the options of the command line that choose the repository
// (`-C DIR`, `--git-dir=DIR`, ...), so that `alias g='git -C ~/src'` works.

// The options of git itself that take the next word as their value.
fn takes_value(w) {
    w in ["-C", "-c", "--git-dir", "--work-tree", "--namespace", "--config-env", "--exec-path"]
}

fn tilde(p) {
    if p == "~" || p.starts_with("~/") {
        let home = sh::getvar("HOME") ?? "";
        return home + p.sub_string(1);
    }
    p
}

// The lines git prints, without its errors, or [] if it fails.
fn git_lines(g, args) {
    let r = sh::capture(`${g} ${args} 2>/dev/null`);
    if r.status != 0 || r.out == "" {
        return [];
    }
    r.out.split("\n")
}

// git's commands, but not its plumbing, with their descriptions, as
// `git help -a` lists them, and the aliases.
fn commands(g) {
    let out = [];
    let section = "";
    for line in git_lines("LC_ALL=C " + g, "help -a") {
        if !line.starts_with("   ") {
            if line != "" {
                section = line;
            }
            continue;
        }
        if section.starts_with("Low-level") || section.contains("interfaces") {
            continue;
        }
        let s = line.sub_string(3);
        let sp = s.index_of(' ');
        if sp < 0 {
            out.push(s);
            continue;
        }
        let desc = s.sub_string(sp);
        desc.trim();
        if section == "Command aliases" {
            desc = `alias for '${desc}'`;
        }
        out.push(#{value: s.sub_string(0, sp), desc: desc});
    }
    out
}

// What an alias stands for (its first word), or () for an alias that runs a
// shell command or for a command that isn't an alias.
fn alias_of(g, name) {
    let v = git_lines(g, "config --get " + sh::quote("alias." + name));
    if v.is_empty() || v[0].starts_with("!") {
        return ();
    }
    let w = v[0].split();
    if w.is_empty() { () } else { w[0] }
}

// The candidates for options of `cmd`, as git lists them, plus `extra`.
// Options that start with `--no-` only when the word does.
fn options(g, cmd, cur, extra) {
    let out = [];
    let negated = false;
    let listed = [];
    for l in git_lines(g, sh::quote(cmd) + " --git-completion-helper") {
        listed += l.split();
    }
    for o in listed + extra {
        if o == "--" {
            negated = true;
        } else if !negated || cur.starts_with("--no") {
            out.push(if o.ends_with("=") { #{value: o, suffix: ""} } else { o });
        }
    }
    out
}

fn short_refs(g, patterns) {
    git_lines(g, "for-each-ref --format='%(refname:short)' " + patterns)
}

fn branches(g) {
    short_refs(g, "refs/heads")
}

fn remotes(g) {
    git_lines(g, "remote")
}

// The branches of the remotes without the remote's name (`main` for
// `origin/main`), which `git switch` and `git checkout` create a branch for.
fn remote_names(g) {
    let out = [];
    for r in git_lines(g, "for-each-ref --format='%(refname:lstrip=3)' refs/remotes") {
        if r != "HEAD" && !(r in out) {
            out.push(r);
        }
    }
    out
}

fn refs(g) {
    let out = short_refs(g, "refs/heads refs/tags refs/remotes");
    out.insert(0, "HEAD");
    out
}

// Branches and tags, and for a range (`A..B`, `A...B`) the part after the
// dots.
fn revisions(g, cur) {
    let dots = cur.index_of("..");
    if dots < 0 {
        return refs(g);
    }
    let end = dots + 2;
    if cur.sub_string(end).starts_with(".") {
        end += 1;
    }
    #{prefix: cur.sub_string(0, end), candidates: refs(g)}
}

fn stashes(g) {
    let out = [];
    for l in git_lines(g, "stash list --format='%gd%x09%gs'") {
        let tab = l.index_of('\t');
        out.push(#{value: l.sub_string(0, tab), desc: l.sub_string(tab + 1)});
    }
    out
}

fn worktrees(g) {
    let out = [];
    for l in git_lines(g, "worktree list --porcelain") {
        if l.starts_with("worktree ") {
            out.push(l.sub_string(9));
        }
    }
    out
}

// The files git lists (relative to the current directory), completed one
// directory at a time: a file under a directory that the word doesn't
// reach yet gives the directory.
fn files(g, args, cur) {
    let out = [];
    let seen = #{};
    for f in git_lines(g, "-c core.quotePath=false " + args) {
        if f == "" || !f.starts_with(cur) || seen.contains(f) {
            continue;
        }
        let slash = f.sub_string(cur.len()).index_of('/');
        if slash < 0 {
            seen[f] = true;
            out.push(f);
        } else {
            let d = f.sub_string(0, cur.len() + slash + 1);
            if !seen.contains(d) {
                seen[d] = true;
                out.push(#{value: d, suffix: ""});
            }
        }
    }
    out
}

fn modified(g, cur) {
    files(g, "ls-files --modified", cur)
}

fn staged(g, cur) {
    files(g, "diff --cached --name-only --relative", cur)
}

// Values for `--option=`, as a completion of the part after `=`.
fn option_value(cur) {
    let eq = cur.index_of('=');
    let opt = cur.sub_string(0, eq);
    let values = switch opt {
        "--format" | "--pretty" => ["oneline", "short", "medium", "full", "fuller", "reference", "email", "raw"],
        "--color" | "--color-moved" => ["always", "never", "auto"],
        "--date" => ["relative", "local", "iso", "iso-strict", "rfc", "short", "raw", "human", "unix"],
        "--cleanup" => ["strip", "whitespace", "verbatim", "scissors", "default"],
        "--diff-algorithm" => ["default", "myers", "minimal", "patience", "histogram"],
        "--untracked-files" => ["no", "normal", "all"],
        "--conflict" => ["merge", "diff3", "zdiff3"],
        "--track" => ["direct", "inherit"],
        _ => [],
    };
    #{prefix: cur.sub_string(0, eq + 1), candidates: values}
}

// Options of `git log` and `git show` that git doesn't list.
fn log_options() {
    [
        "--oneline", "--graph", "--all", "--decorate", "--stat", "--patch", "--follow", "--reverse",
        "--author=", "--committer=", "--since=", "--until=", "--grep=", "--max-count=", "--skip=",
        "--format=", "--pretty=", "--date=", "--abbrev-commit", "--name-only", "--name-status",
        "--first-parent", "--merges", "--no-merges", "--branches", "--tags", "--remotes", "--walk-reflogs",
        "--left-right", "--cherry-pick", "--boundary", "--simplify-by-decoration", "--full-history",
        "--relative-date", "--show-signature", "--word-diff", "--color-words",
    ]
}

// The candidates for an argument of `git CMD`: `args` are the words from
// the command to the cursor, and `cur` the word being completed.
fn arguments(g, cmd, args, cur, depth) {
    let dd = args.index_of("--") >= 0;
    let prev = if args.is_empty() { "" } else { args[-1] };
    // The arguments that aren't options.
    let pos = args.filter(|a| !a.starts_with("-")).len();
    let has = |opts| args.some(|a| a in opts);
    switch cmd {
        "add" | "stage" => files(g, "ls-files --modified --others --exclude-standard", cur),
        "restore" => if has.call(["--staged", "-S"]) { staged(g, cur) } else { modified(g, cur) },
        "rm" => files(g, "ls-files", cur),
        "commit" => if prev in ["-m", "--message", "-F", "--file", "-C", "-c", "--author"] { [] } else { () },
        "switch" => {
            if prev in ["-c", "-C", "--create", "--force-create", "--orphan"] {
                []
            } else if has.call(["-c", "-C", "--create", "--force-create", "--detach", "-d"]) {
                revisions(g, cur)
            } else {
                let b = branches(g);
                for r in remote_names(g) {
                    if !(r in b) {
                        b.push(r);
                    }
                }
                b
            }
        }
        "checkout" => {
            if dd {
                modified(g, cur)
            } else if prev in ["-b", "-B", "--orphan"] {
                []
            } else {
                let r = refs(g);
                for n in remote_names(g) {
                    if !(n in r) {
                        r.push(n);
                    }
                }
                r + modified(g, cur)
            }
        }
        "branch" => {
            if prev in ["-u", "--set-upstream-to"] {
                short_refs(g, "refs/remotes")
            } else if has.call(["-d", "-D", "--delete", "-m", "-M", "--move", "-c", "-C", "--copy", "--edit-description"]) {
                branches(g)
            } else if pos == 0 {
                []   // the new branch's name
            } else {
                revisions(g, cur)
            }
        }
        "tag" => {
            if has.call(["-d", "--delete", "-v", "--verify"]) {
                short_refs(g, "refs/tags")
            } else if pos == 0 || prev in ["-m", "--message", "-F", "--file"] {
                []
            } else {
                revisions(g, cur)
            }
        }
        "diff" => if dd { modified(g, cur) } else { revisions(g, cur) + modified(g, cur) },
        "reset" => if dd { staged(g, cur) } else { revisions(g, cur) + staged(g, cur) },
        "log" | "show" | "shortlog" | "whatchanged" | "cherry" | "range-diff" | "format-patch" | "describe" | "merge"
        | "rebase" | "cherry-pick" | "revert" | "merge-base" | "rev-list" => {
            if dd { () } else { revisions(g, cur) }
        }
        "fetch" | "pull" => {
            if pos == 0 {
                remotes(g)
            } else {
                let remote = args.filter(|a| !a.starts_with("-"))[0];
                git_lines(g, "for-each-ref --format='%(refname:lstrip=3)' " + sh::quote("refs/remotes/" + remote))
                    .filter(|r| r != "HEAD")
            }
        }
        "push" => if pos == 0 { remotes(g) } else { branches(g) + short_refs(g, "refs/tags") },
        "remote" => {
            if pos == 0 {
                [
                    #{value: "add", desc: "Add a remote"},
                    #{value: "rename", desc: "Rename a remote"},
                    #{value: "remove", desc: "Remove a remote"},
                    #{value: "set-head", desc: "Set the default branch of a remote"},
                    #{value: "set-branches", desc: "Change the branches a remote tracks"},
                    #{value: "get-url", desc: "Show the URLs of a remote"},
                    #{value: "set-url", desc: "Change the URLs of a remote"},
                    #{value: "show", desc: "Show information about a remote"},
                    #{value: "prune", desc: "Delete stale remote-tracking branches"},
                    #{value: "update", desc: "Fetch updates for remotes or groups"},
                ]
            } else if pos == 1 && args.filter(|a| !a.starts_with("-"))[0] != "add" {
                remotes(g)
            } else {
                []
            }
        }
        "stash" => {
            if pos == 0 {
                [
                    #{value: "push", desc: "Save the changes in a new stash"},
                    #{value: "pop", desc: "Apply a stash and remove it"},
                    #{value: "apply", desc: "Apply a stash"},
                    #{value: "list", desc: "List the stashes"},
                    #{value: "show", desc: "Show the changes in a stash"},
                    #{value: "drop", desc: "Remove a stash"},
                    #{value: "branch", desc: "Create a branch from a stash"},
                    #{value: "clear", desc: "Remove all the stashes"},
                ]
            } else {
                switch args.filter(|a| !a.starts_with("-"))[0] {
                    "pop" | "apply" | "show" | "drop" => stashes(g),
                    "branch" => if pos == 1 { [] } else { stashes(g) },
                    "push" => modified(g, cur),
                    _ => [],
                }
            }
        }
        "worktree" => {
            if pos == 0 {
                [
                    #{value: "add", desc: "Create a worktree"},
                    #{value: "list", desc: "List the worktrees"},
                    #{value: "lock", desc: "Prevent a worktree from being pruned"},
                    #{value: "move", desc: "Move a worktree"},
                    #{value: "prune", desc: "Prune worktree information"},
                    #{value: "remove", desc: "Remove a worktree"},
                    #{value: "repair", desc: "Repair worktree administrative files"},
                    #{value: "unlock", desc: "Unlock a worktree"},
                ]
            } else {
                switch args.filter(|a| !a.starts_with("-"))[0] {
                    "remove" | "lock" | "unlock" | "move" => if pos == 1 { worktrees(g) } else { () },
                    "add" => if pos == 1 { () } else { revisions(g, cur) },
                    _ => (),
                }
            }
        }
        "bisect" => {
            if pos == 0 {
                ["start", "bad", "good", "new", "old", "terms", "skip", "next", "reset", "visualize", "replay", "log", "run"]
            } else {
                revisions(g, cur)
            }
        }
        "help" => commands(g),
        _ => {
            // An alias completes as the command it stands for.
            let a = if depth == 0 { alias_of(g, cmd) } else { () };
            if a == () { () } else { arguments(g, a, args, cur, depth + 1) }
        }
    }
}

// The command an alias stands for, or the command itself.
fn resolve(g, cmd) {
    alias_of(g, cmd) ?? cmd
}

fn complete(words, i) {
    let cur = words[i];
    // git's own options, which come before the command, and those of them
    // that choose the repository.
    let g = "command git";
    let j = 1;
    while j < i {
        let w = words[j];
        if takes_value(w) {
            if j + 1 == i {
                // The option's value.
                return if w in ["-C", "--git-dir", "--work-tree", "--exec-path"] { () } else { [] };
            }
            g += " " + sh::quote(w) + " " + sh::quote(if w == "-c" { words[j + 1] } else { tilde(words[j + 1]) });
            j += 2;
        } else if w.starts_with("-") {
            if w.starts_with("--git-dir=") || w.starts_with("--work-tree=") {
                let eq = w.index_of('=');
                g += " " + sh::quote(w.sub_string(0, eq + 1) + tilde(w.sub_string(eq + 1)));
            } else if w.starts_with("--namespace=") || w == "--bare" {
                g += " " + sh::quote(w);
            }
            j += 1;
        } else {
            break;
        }
    }
    if j == i {
        if cur.starts_with("-") {
            return [
                "--version", "--help", "-C", "-c", "--exec-path", "--html-path", "--man-path", "--info-path",
                "--paginate", "--no-pager", "--no-replace-objects", "--bare", "--literal-pathspecs",
                "--glob-pathspecs", "--noglob-pathspecs", "--icase-pathspecs", "--no-optional-locks",
                #{value: "--git-dir=", suffix: ""}, #{value: "--work-tree=", suffix: ""},
                #{value: "--namespace=", suffix: ""}, #{value: "--config-env=", suffix: ""},
            ];
        }
        return commands(g);
    }
    let cmd = words[j];
    let args = words.extract(j + 1, i - j - 1);
    if cur.starts_with("-") && args.index_of("--") < 0 {
        if cur.contains("=") {
            return option_value(cur);
        }
        let real = resolve(g, cmd);
        let extra = if real in ["log", "show", "whatchanged", "shortlog"] { log_options() } else { [] };
        return options(g, real, cur, extra);
    }
    arguments(g, cmd, args, cur, 0)
}

sh::completer("git", |words, i| complete(words, i));
