# A chpwd hook that runs cd doesn't trigger itself.
mkdir -p a/b
cat > r.rhai <<'P'
sh::hook("chpwd", |from, to| {
    if to.ends_with("/a") { sh::run("cd b"); }
    print(`hook ran in .../${sh::cwd().split("/").pop()}`);
});
P
__luish_internal plugin load ./r.rhai
cd a
echo "${PWD#"$HOME"}"
