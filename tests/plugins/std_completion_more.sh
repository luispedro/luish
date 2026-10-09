# More of the completion plugin of luish-std-plugins (std/completion):
# shells, find, subcommands (systemctl, apt, pip ...), operations (pacman
# -S), programs read from their -h (cargo, pixi), Cobra programs (and
# complete-cobra), nix and openssl. Programs that the completers run are
# stand-ins in ~/bin, and files such as Cargo.toml are made here, so that
# nothing depends on what is installed.
__luish_internal plugin load "$STD_PLUGINS/completion"
echo "load $?"
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
mkdir -p bin src/bin examples
PATH=$HOME/bin:$PATH
touch notes.txt a.gz b.txt.gz x.deb y.rpm z.pkg.tar.zst src/main.rs src/bin/tool.rs examples/demo.rs
w() {
    printf '%s\n' '#!/bin/sh' "$2" > "bin/$1"
    chmod +x "bin/$1"
}

echo "=== shells"
c 'luish --no-'
c 'luish --s'
c 'luish -o pipe'
c 'luish -o glob.'
c 'luish -c cmd '
c 'sh -o nou'
c 'bash -O extg'

echo "=== single-dash options (find, gcc)"
c 'find . -ty'
c 'find . -type '
c 'find . -user roo'
c 'find -maxdepth 2 s'
c 'find . -newer n'
c 'gcc -std=c1'
c 'gcc -fsanitize=th'
c 'gcc -O'

echo "=== a value after = only where the table has it (vim)"
c 'vim --cm'
c 'vim --startuptime n'

echo "=== the values of options by the names of their arguments (curl)"
c 'curl -X P'
c 'curl -o n'
c 'curl --header '
c 'curl --cacert n'

echo "=== options of two letters or more (wget)"
c 'wget -n'
c 'wget -nv'

echo "=== files by extension"
c 'gunzip '
c 'gzip -d '
c 'gzip n'
c 'dpkg -i '
c 'rpm -qp '
c 'pacman -U '

echo "=== subcommands"
c 'systemctl sta'
c 'systemctl -t '
c 'systemctl --no-p'
c 'systemctl status --no-p'
c 'loginctl lock-s'
c 'apt fu'
c 'apt list --up'
c 'apt-get install --no-install-r'
c 'pip '
c 'pip install -r n'
c 'pip install --prog'
c 'tmux ne'
c 'tmux new -c s'
c 'ip a'
c 'ip addr sh'
c 'ip -f in'
c 'go mod ti'
c 'dig +sh'
c 'ssh-keygen -t ed'

echo "=== aliases of subcommands"
c 'tmux attach -'
c 'dnf in -'

echo "=== operations (pacman)"
c 'pacman -S'
c 'pacman -Sy'
c 'pacman -S --need'
c 'pacman -R'

echo "=== programs read from their -h (cargo)"
w cargo 'case "$*" in
--list) printf "%s\n" "Installed Commands:" "    b                    alias: build" \
    "    build                Compile a local package" "    fmt                  Formats all bin and lib files" ;;
"build -h") printf "%s\n" "Compile a local package" "" "Options:" \
    "  -r, --release                 Build artifacts in release mode. With optimizations" \
    "  -p, --package [<SPEC>]        Package to build [default: all]" \
    "      --bin [<NAME>]            Build only the specified binary" \
    "      --example [<NAME>]        Build only the specified example" \
    "  -F, --features <FEATURES>     Space or comma separated list of features to activate" \
    "      --profile <PROFILE-NAME>  Build artifacts with the specified profile" \
    "      --manifest-path <PATH>    Path to Cargo.toml" \
    "      --message-format <FMT>    Error format" \
    "      --config <KEY=VALUE>      Override a configuration value" \
    "      --color <WHEN>            Coloring [possible values: auto, always, never]" ;;
esac'
w rustup 'case "$*" in
"toolchain list") printf "%s\n" "stable-x86_64-unknown-linux-gnu (active, default)" "1.80.0" ;;
esac'
cat > Cargo.toml <<'E'
[package]
name = "proj"

[features]
default = ["fast"]
fast = []
simd = []

[dependencies]
serde = "1"

[[bin]]
name = "extra"
path = "extra.rs"

[profile.bench-fast]
inherits = "release"
E
c 'cargo '
c 'cargo -'
c 'cargo --color '
c 'cargo +'
c 'cargo +stable b'
c 'cargo -C src b'
c 'cargo build --rel'
c 'cargo b --rel'
c 'cargo build --bin '
c 'cargo build --example '
c 'cargo build -F fast,'
c 'cargo build --profile '
c 'cargo build -p '
c 'cargo build --manifest-path C'
c 'cargo build --config '
c 'cargo build --color '
# The words of the line are not run as commands.
c "cargo 'x;touch pwned' --"
[ -e pwned ] && echo "pwned"

echo "=== pixi's tasks and environments"
w pixi 'case "$*" in
-h) printf "%s\n" "Usage: pixi [OPTIONS] [COMMAND]" "" "Commands:" \
    "  run          Runs task in the pixi environment [aliases: r]" "  remove       Removes dependencies [aliases: rm]" ;;
"run -h") printf "%s\n" "Options:" "  -e, --environment <ENVIRONMENT>  The environment to run the task in" ;;
esac'
cat > pixi.toml <<'E'
[workspace]
name = "px"

[tasks]
build = "cargo build"
test = { cmd = "cargo test", depends-on = ["build"] }

[tasks.lint]
cmd = "ruff check"

[feature.docs.tasks]
docs = "sphinx-build docs out"

[feature.docs.dependencies]
sphinx = "*"

[dependencies]
python = "3.12"

[environments]
docs = ["docs"]
E
c 'pixi '
c 'pixi r '
c 'pixi run -e '
c 'pixi rm '

echo "=== npm, yarn and pnpm"
cat > package.json <<'E'
{"name": "x", "scripts": {"serve": "vite", "check": "tsc"},
 "dependencies": {"left-pad": "1"}, "devDependencies": {"vitest": "1"}}
E
c 'npm run '
c 'npm rm '
c 'npm i -D'
c 'yarn se'
c 'pnpm ch'

echo "=== conda"
mkdir -p conda/bin conda/envs/py3 conda/envs/.hidden conda/envs/py3/conda-meta
touch conda/envs/py3/conda-meta/numpy-2.1.0-py312h1.json conda/envs/py3/conda-meta/python-dateutil-2.9-pyhd.json
CONDA_EXE=$HOME/conda/bin/conda
c 'conda activate '
c 'conda install -n p'
c 'conda remove -n py3 '
c 'mamba env '

echo "=== Cobra programs"
w gh '[ "$1" = __complete ] || exit 1
shift
case "$*" in
" "|"") printf "pr\tManage pull requests\nrepo\tManage repositories\n:4\n" ;;
"pr list --state="*|"pr list --state "*) printf "open\nclosed\nmerged\n:4\n" ;;
"pr checkout "*) printf ":0\n" ;;
*) printf ":1\n" ;;
esac'
c 'gh '
c 'gh pr list --state='
c 'gh pr list --state '
c 'gh pr checkout n'
c 'gh nosuch '
w frob '[ "$1" = __complete ] && printf "up\tStart\ndown\tStop\n:4\n"'
c 'frob n'
complete-cobra frob
echo "complete-cobra $?"
c 'frob '
complete-cobra | grep -x -e gh -e frob
complete-cobra a/b 2>/dev/null
echo "complete-cobra $?"

echo "=== Click programs"
w clk '[ "$_CLK_COMPLETE" = fish_complete ] || { echo "Usage: clk [OPTIONS]"; exit 0; }
W=$(printf "%s" "$COMP_WORDS" | sed "s/\x27\([A-Za-z0-9_=.\/-]*\)\x27/\1/g")
case "$W|$COMP_CWORD" in
"clk |") printf "plain,run\tDo it.\nplain,other\tOther one.\n" ;;
"clk ru|ru") printf "plain,run\tDo it.\n" ;;
"clk run --|--") printf "plain,--mode\tLine one\\\\nline two\nplain,--nohelp\nplain,--in-file\nplain,--outdir\n" ;;
"clk run --mode |") printf "plain,fast\nplain,slow mode\n" ;;
"clk run --mode=s|--mode=s") printf "plain,slow mode\n" ;;
"clk run -i n|n") printf "file,n\n" ;;
"clk run --in-file=n|--in-file=n") printf "file,n\n" ;;
"clk run --outdir |") printf "dir,\n" ;;
"clk run --outdir=s|--outdir=s") printf "dir,\n" ;;
"clk run "*) printf "plain,%s\nplain,[%s]\n" "$W" "$COMP_CWORD" ;;
*) ;;
esac'
w ign 'echo "Usage: ign [OPTIONS]"; echo "Options:, none"'
mkdir -p sub
complete-click clk
c 'clk '
c 'clk ru'
c 'clk run --'
c 'clk run --mode '
c 'clk run --mode=s'
c 'clk run -i n'
c 'clk run --in-file=n'
c 'clk run --outdir '
c 'clk run --outdir=s'
c "clk run 'a b' "
c "clk run '\$(touch pwned)'"
c "clk run x '\$(touch pwned)"
test -e pwned && echo INJECTED || echo "no injection"
c 'clk nosuch '
complete-click ign
echo "complete-click $?"
c 'ign n'
complete-click | grep -x -e black -e ign
complete-click a/b 2>/dev/null
echo "complete-click $?"

echo "=== nix"
w nix 'case "$NIX_GET_COMPLETIONS:$*" in
"1:bu") printf "normal\nbuild\t\nbundle\t\n" ;;
"2:build --f") printf "normal\n--file\tInterpret [*installables*](@docroot@/command-ref/new-cli/nix.md#installables) as\n" ;;
"2:build nixpkgs#he") printf "attrs\nnixpkgs#hello\t\nnixpkgs#hello-go\t\n" ;;
"2:build ./"*) printf "filenames\n" ;;
esac'
c 'nix bu'
c 'nix build --f'
c 'nix build nixpkgs#he'
c 'nix build ./n'

echo "=== openssl, from the -help of its commands"
w openssl 'case "$*" in
"x509 -help") printf "%s\n" "Usage: x509 [options]" "General options:" " -in infile   Input file" \
    " -inform format   Input format" " -text   Print the certificate in text form" ;;
esac'
c 'openssl x5'
c 'openssl x509 -in'
c 'openssl x509 -in n'
