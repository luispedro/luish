#!/bin/sh
# Runs the Oils spec tests under luish, dash, bash and zsh, and prints the
# summaries (see README.md).
#
# Usage: scripts/oils-spec/run.sh [OILS_DIR [OUT_DIR]]
#
# OILS_DIR (default target/oils) is cloned if it doesn't exist; the results go
# to OUT_DIR (default target/oils-spec). $LUISH is the shell under test
# (default target/release/luish, from `pixi run release`), $ZSH the zsh (default
# pixi's).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
oils=${1:-$root/target/oils}
out=${2:-$root/target/oils-spec}
luish=${LUISH:-$root/target/release/luish}
zsh=${ZSH:-$(cd "$root" && pixi run which zsh)}

[ -x "$luish" ] || { echo "$luish: not found (pixi run release)" >&2; exit 1; }
[ -d "$oils" ] || git clone --depth 1 https://github.com/oils-for-unix/oils "$oils"
mkdir -p "$out"

shells="dash=/usr/bin/dash bash=/usr/bin/bash zsh=$zsh"
files=$(cd "$oils/spec" && grep -l '^## compare_shells:' -- *.test.sh)
# shellcheck disable=SC2086
python3 -I "$here/run_oils.py" "$oils" "$out/all.json" luish="$luish" $shells -- $files

# Brace expansion and **/ are options in luish (off by default).
printf '#!/bin/sh\nexec "%s" -o expand.braces -o glob.star "$@"\n' "$luish" >"$out/luish-opts"
chmod +x "$out/luish-opts"
# shellcheck disable=SC2086
python3 -I "$here/run_oils.py" "$oils" "$out/opts.json" luish="$out/luish-opts" $shells -- \
    brace-expansion.test.sh globstar.test.sh glob-bash.test.sh

echo "== Files that list dash"
python3 -I "$here/analyse.py" "$out/all.json" dash
python3 -I "$here/classify.py" "$out/all.json" >"$out/classify.txt"
grep -v '^ ' "$out/classify.txt"
echo "== Other files"
python3 -I "$here/analyse.py" "$out/all.json" other | head -1
echo "== Brace expansion and globstar, with the options on"
python3 -I "$here/analyse.py" "$out/opts.json" other | head -1
echo "Details: $out/classify.txt; analyse.py -v and show.py on $out/all.json"
