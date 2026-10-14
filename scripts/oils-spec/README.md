# Oils spec tests

Scripts that run the spec tests of [Oils](https://github.com/oils-for-unix/oils) (`spec/*.test.sh`) under luish,
dash, bash and zsh, and compare the results. They aren't part of `pixi run check`: the tests are Oils', and many
are about features of bash that luish doesn't have. The results of the last run, and what the differences are,
are under Conformance in `DEVELOPING.md`.

## Running

```sh
pixi run release
scripts/oils-spec/run.sh            # clones Oils into target/oils, writes target/oils-spec/
```

`run.sh [OILS_DIR [OUT_DIR]]` runs every file with a `## compare_shells:` line, then the brace-expansion and globstar
files again with `-o expand.braces -o glob.star`, and prints the summaries. `$LUISH` and `$ZSH` choose the shells
(by default `target/release/luish` and pixi's zsh); dash and bash are the system's (`/usr/bin`). A run of the
2800 cases takes about half a minute.

## The scripts

- `run_oils.py OILS_DIR OUT.json luish=PATH dash=PATH bash=PATH zsh=PATH [-- FILE...]` runs the cases. Each file is
  split on `#### `; lines starting with `## ` are expectations (`## stdout:`, `## status:`, `## STDOUT:` ...
  `## END` blocks, and the same after `OK`, `BUG` or `N-I` and shell names). Each case runs as Oils' own runner runs
  it: on standard input, in a fresh directory that is also `$TMP`, with `spec/bin` first in `PATH`,
  `LC_ALL=C.UTF-8`, `REPO_ROOT` and `SH` (the full path, so `case $SH in dash)` in a case doesn't match), and a
  timeout of 5 s. The helpers in `spec/bin` want `python2`, so it is `python3` here, which prints the same for
  ASCII arguments. The JSON has, for each case, its file, number, description, code, the expected output, and each
  shell's stdout, stderr and status.
- `analyse.py RESULTS.json dash|other [-v]` counts the cases where luish differs, by file. With `dash`, the files
  whose `compare_shells` list dash, compared with dash's stdout and status. With `other`, the rest: a case is fine
  if luish gives bash's or zsh's output, or the expected one. `-v` prints each case that differs, with every
  shell's output.
- `classify.py RESULTS.json` sorts the cases where luish differs from dash in the files that list dash: `ext`
  (dash fails, luish does as bash or zsh: an extension), `B-change` (dash succeeds, luish does as bash or zsh: a
  deviation, which should be in `docs/compatibility.md`), `C-none` (none of the three: look at these first).
- `show.py RESULTS.json FILE#N...` prints cases side by side, e.g. `show.py target/oils-spec/all.json
  alias.test.sh#26`.

## Reading the results

Only stdout and the status are compared. Some cases race (background jobs, `SIGPIPE`), so a case or two can
change between runs; run `show.py` on a case before believing it. A few cases write outside their directory
(`builtin-dirs` makes `/tmp/oils-spec`). A bug found here gets a differential case in `tests/cases/` like any other.
