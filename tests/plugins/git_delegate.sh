# std's git completer hands a command that is a program of its own (`git zz`
# runs `git-zz`, in PATH) over to that program's completer, with the words an
# alias for it adds, in the directory that `-C` chooses. A program without a
# completer gets filenames.
export GIT_CONFIG_NOSYSTEM=1
__luish_internal plugin load "$STD_PLUGINS/completion"
mkdir bin r
printf '#!/bin/sh\n' > bin/git-zz
cp bin/git-zz bin/git-ww
chmod +x bin/git-zz bin/git-ww
PATH=$PWD/bin:$PATH
cat > zz.rhai <<'R'
sh::completer("git-zz", |words, i| {
    let cur = words[i];
    if cur.starts_with("--to=") {
        return #{prefix: "--to=", candidates: ["origin"]};
    }
    if cur == "here" {
        return [`here:${sh::cwd().split("/")[-1]}`];
    }
    [`zz:${words}:${i}`]
});
R
__luish_internal plugin load ./zz.rhai
git init -q r
mkdir -p r/sub/deeper
touch r/sub/deeper/file r/afile
git -C r config alias.zg 'zz get'
git -C r config alias.st status
cd r
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
c 'git z'
c 'git zz '
c 'git zz a b'
c 'git zz a --to=o'
c 'git zz a b --'
c 'git zg '
c 'git -C sub zz here'
c 'git -C sub -C deeper zz here'
c 'git --no-pager -C sub zz here'
c 'git ww '
c 'git ww s'
c 'git st --sh'
