# The vcs module: git repositories found from the current directory, the
# branch, the operation in progress, and the state of the work tree.
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
g() { git "$@" >/dev/null 2>&1; }
cat > vcs.rhai <<'P'
let home = sh::getvar("HOME");
fn short(p, home) { if type_of(p) == "string" && p.starts_with(home) { "~" + p.sub_string(home.len()) } else { p } }
fn show(i, home) {
    if i == () { return "no repository"; }
    let head = if i.head == () { "none" } else if i.head == sh::getvar("HEAD") { "HEAD" } else { i.head };
    `${i.vcs} root=${short(i.root, home)} name=${i.name} subdir=${i.subdir} git_dir=${short(i.git_dir, home)} branch=${i.branch ?? "none"} head=${head} action=${i.action ?? "none"} step=${i.step ?? "-"}/${i.steps ?? "-"} stashes=${i.stashes}`
}
fn status(s) {
    if s == () { return "no status"; }
    `upstream=${s.upstream ?? "none"} +${s.ahead} -${s.behind} staged=${s.staged} unstaged=${s.unstaged} untracked=${s.untracked} conflicts=${s.conflicts} clean=${s.clean}`
}
sh::hook("chpwd", |from, to| {
    sh::run("HEAD=$(git rev-parse -q --verify HEAD 2>/dev/null)");
    print(`${short(to, home)}: ${show(vcs::info(), home)}`);
    print(`  ${status(vcs::status())}`);
});
P
__luish_internal plugin load ./vcs.rhai
cd .
g init -b main r
cd r
echo 1 > f && g add f && g commit -m one
mkdir -p a/b
cd a/b
cd "$HOME/r"
echo "packed refs:"
g pack-refs --all
cd .
echo "changes:"
echo 2 > f && echo n > new && echo s > staged && g add staged
cd .
g stash -u
g commit --allow-empty -m two
cd .
echo "detached:"
g checkout --detach HEAD~
cd .
g checkout main
echo "upstream:"
g clone "$HOME/r" "$HOME/clone"
cd "$HOME/clone"
g commit --allow-empty -m three
cd .
cd "$HOME/r"
echo "merge conflict:"
g checkout -b side && echo side > f && g commit -am side
g checkout main && echo main > f && g commit -am main
g merge side
cd .
g merge --abort
echo "rebase:"
g checkout side
g rebase main
cd .
g rebase --abort
echo "cherry-pick:"
g checkout main
g cherry-pick side
cd .
g cherry-pick --abort
echo "bisect:"
g bisect start
cd .
g bisect reset
echo "worktree:"
g worktree add "$HOME/wt" -b other
cd "$HOME/wt"
echo "explicit directories:"
cat > dirs.rhai <<'P'
print(vcs::info("../r/a").subdir);
print(vcs::info(sh::getvar("HOME")) ?? "none");
print(vcs::status("/") ?? "none");
print(vcs::info("/") ?? "none");
P
__luish_internal plugin load ./dirs.rhai
