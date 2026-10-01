# Benchmarks

Script-sized workloads for comparing luish with dash and other shells. Each one is a realistic script of a few hundred
lines, not a microbenchmark: `-c true` measures start-up, and a loop of `$((i+1))` measures one path through the
executor, but real scripts mix parsing, expansion, function calls, built-ins and forks.

```sh
pixi run release                  # the luish under test: target/release/luish
bench/run.sh                      # all benchmarks, default shells
bench/run.sh -c                   # only check that the outputs agree (shows diffs)
bench/run.sh -n 4 -r 10 arith     # one benchmark, 4x the work, 10 runs per shell
bench/run.sh -s dash=dash -s old=/tmp/luish-old -s new=target/release/luish   # compare builds
pixi run bench                    # release build, then bench/run.sh (arguments are passed on)
```

`run.sh` uses [hyperfine](https://github.com/sharkdp/hyperfine) when it is installed (`apt install hyperfine`) and a
plain timing loop otherwise. It first runs every script once per shell and compares stdout and the exit status with
the first shell's (dash by default): a shell whose output differs is flagged in the results, since its time isn't
comparable, and `run.sh` exits with status 1. So the benchmarks double as a coarse conformance test. It ends with a
Markdown table of mean times and ratios to the first shell (`-o FILE` saves it).

The default shells are dash, luish, `bash --posix`, `zsh --emulate sh` and `busybox sh`, whichever are installed.
mksh also runs every script with the same output, but isn't a default because it is very slow on `textproc` (about
40 times dash); add it with `-s mksh=mksh`. A static BusyBox (Debian's and Ubuntu's `busybox-static`) runs its own
`sed`, `cat`, `basename` and so on in the shell's process instead of the system's programs, so its times for
`configure` and `build` don't measure the same work.
Scripts run with a cleared environment (`PATH`, `HOME`, `LC_ALL=C`) and `$SH` set to the shell's command, and each
works in its own `mktemp -d` directory. Absolute times vary with the machine and its load, so compare shells within
one run, and run one benchmark at a time. On a machine whose cores differ in speed (Intel's performance and efficiency
cores), pin the whole run to one core so that every shell gets the same one: `taskset -c 3 bench/run.sh`.

## The scripts

Each script takes a scale (default 1) as its first argument; the work grows linearly with it. At scale 1 each takes
0.2 to 1.5 s under dash. Except `arrays.sh`, they use only POSIX features plus `local`, keep arithmetic below 2^31 up
to scale 50 (mksh has 32-bit integers), and use `printf` rather than `echo`.

| Script | What it does | Mostly exercises |
|---|---|---|
| `arith.sh` | Mandelbrot set in fixed point, sieve of Eratosthenes over eval'd variables, Collatz lengths | `$((...))`, `[`, `case`, loops, a large variable table |
| `strings.sh` | Pure-shell upper-casing, reversal, replace-all, JSON and URL escaping, word frequencies | `${x#pat}`/`${x%pat}` with quoted patterns, string building, `read`, `printf` |
| `textproc.sh` | Generates and parses an access log and an INI file, aggregating counters | `while read`, field splitting with `IFS`, `set --`, eval'd counters |
| `functions.sh` | Recursion (Hanoi, Fibonacci, Ackermann), logging wrappers, `getopts` in a function, a stack and a queue | function calls, `local`, `"$@"`, `shift`, `return` statuses |
| `configure.sh` | An autoconf-style `configure`: option parsing, cached header/function checks, `config.status` | here-documents, `$(... \| sed)`, `eval`, fd redirections: many small forks |
| `build.sh` | A make-like build: dependency scanning, one `$SH -c` recipe per object, an incremental rebuild | fork and exec, shell start-up, pipelines, `test -nt`, globbing |
| `arrays.sh` | Not POSIX: quicksort, a sieve, word counts, grouped records, matrix products, a BFS and sliding windows in arrays | indexed and associative arrays, `a+=(x)`, `$(( a[i] ))`, slices, `${x//pat/rep}`, `[[ ]]` |

`arrays.sh` uses what zsh and bash scripts use beyond POSIX, in the subset on which luish, `bash --posix` and
`zsh --emulate sh` agree (its comment lists what it avoids). dash and BusyBox have no arrays and mksh's differ, so
its line `# skip: dash busybox mksh` makes `run.sh` leave them out: its reference, for the output and the ratios, is
then the first shell that runs it (luish by default), and the skipped shells show `-` in the table.

To add a benchmark, add `scripts/NAME.sh`; its output must be deterministic and the same under every shell that
runs it (check with `bench/run.sh -c NAME`), and it should clean up after itself.

## Commands in Rhai: `extensions/`

`extensions/run.sh` compares a plugin's commands written in Rhai (`sh::builtin`) with the same commands written as
luish shell functions: `tasks.lsh` has the shell functions, `ext.rhai` the extension, and `driver.sh IMPL TASK` runs a
task with either. It checks that both print the same output, then prints a Markdown table of the times (the fastest
and the mean of `-r` runs, 5 by default). At scale 1 each task takes about a second with the shell functions.

```sh
pixi run release
bench/extensions/run.sh                  # all tasks
bench/extensions/run.sh -r 10 collatz    # one task, 10 runs
```

The tasks and the results are in `docs/performance.md` (Commands in Rhai). `gen-words.awk` writes the text that
`wordfreq` reads.
