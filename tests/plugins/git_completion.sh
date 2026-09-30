# Completion for git (std/completion) in a repository: revisions, ranges and
# the files that go with them.
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
__luish_internal plugin load "$STD_PLUGINS/completion"
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
g() { git "$@" >/dev/null 2>&1; }
g init -b main r
cd r
mkdir sub
echo 1 > tracked && echo 1 > sub/deep && g add tracked sub && g commit -m one
g tag v1
echo 2 > tracked && g commit -am two
echo 3 > sub/deep && echo 3 > tracked && g add tracked && echo 4 > tracked
echo u > untracked
echo "=== revisions and modified files"
c 'git diff '
c 'git diff HEAD'
c 'git diff --cached '
echo "=== a range"
c 'git diff HEAD^..'
c 'git diff HEAD^...v'
c 'git log main..'
c 'git reset HEAD^..'
echo "=== files after a revision or a range"
c 'git diff HEAD^.. '
c 'git diff HEAD^.. s'
c 'git diff v1 '
c 'git diff v1 main '
c 'git diff v1 -- '
