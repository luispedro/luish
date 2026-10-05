# Performance

luish is meant to be as fast as dash, the fastest of the common POSIX shells,
for scripts and `sh -c`.

bash and zsh are up to five times slower on the same scripts. What luish adds
to dash (plugins, the interactive features, luish's own options) is opt-in, and
costs scripts nothing.

## Scripts

These are the script benchmarks in the repository's `bench/` directory:
scripts of a few hundred lines, each mixing parsing, expansion, function calls,
built-ins and forks, rather than microbenchmarks. All the shells that run a
benchmark print the same output for it.

| Benchmark | What it does |
|---|---|
| arith | Mandelbrot set in fixed point, sieve of Eratosthenes, Collatz lengths |
| functions | Recursion (Hanoi, Fibonacci, Ackermann), wrappers, `getopts` in functions, a stack and a queue |
| strings | Pure-shell string processing: upper-casing, replacing, JSON and URL escaping, word frequencies |
| textproc | Generating and parsing an access log and an INI file with `while read` |
| configure | An autoconf-style `configure` script: many small forks, here-documents, `sed` |
| build | A make-like build that runs `$SH -c` for each object: fork, exec and shell startup |
| arrays | Not POSIX: sorting, a sieve, word counts, grouping, matrices, a search and slices, in indexed and associative arrays |

The script and startup measurements on this page were made on 2026-10-05, with the release build of luish at git
revision `9dc9b48` (`pixi run release`), on a laptop with an Intel Core i7-1260P (4 performance cores of 2 threads
each, and 8 slower efficiency cores) running Ubuntu 26.04. Every measurement ran pinned to the same performance core
(`taskset -c 3`), one at a time, with the desktop running. The other shells are Ubuntu's dash 0.5.12, bash 5.3
(`--posix`) and BusyBox 1.37.0 (`busybox sh`, from the `busybox-static` package), and zsh 5.9 (`--emulate sh`) from
conda-forge, as pinned in luish's `pixi.toml`. Two complete runs of the script benchmarks gave times within 6% of each
other for dash and luish (luish's ratios within 0.06), and within 9% for the other shells.

Seconds (and the ratio to dash; lower is faster), mean of 40 runs (two runs of `bench/run.sh -r 20`):

| Benchmark | dash | luish | bash | zsh | busybox |
|---|---|---|---|---|---|
| arith | 0.267 (1.00) | 0.172 (0.65) | 0.709 (2.66) | 0.432 (1.62) | 0.352 (1.32) |
| functions | 0.158 (1.00) | 0.142 (0.90) | 0.838 (5.30) | 0.609 (3.85) | 0.195 (1.23) |
| strings | 0.206 (1.00) | 0.194 (0.94) | 0.681 (3.31) | 0.536 (2.60) | 0.303 (1.47) |
| textproc | 0.164 (1.00) | 0.165 (1.01) | 0.418 (2.56) | 0.483 (2.96) | 0.238 (1.46) |
| configure | 0.844 (1.00) | 0.869 (1.03) | 1.069 (1.27) | 1.008 (1.19) | 0.369 (0.44)\* |
| build | 1.157 (1.00) | 1.212 (1.05) | 1.286 (1.11) | 1.276 (1.10) | 0.167 (0.14)\* |

The first four run mostly inside the shell, where luish is as fast as dash, or faster. The next two spend most of
their time starting programs, which luish does with the same system calls as dash; they take about 5% longer, mostly
because luish itself starts more slowly and a command substitution costs more (below), and `build` starts a new shell
for each file it compiles. Ubuntu 26.04's `cat`, `basename` and other basic utilities are those of the Rust
coreutils, which start in about 1.3 ms (to 0.4 ms for `/bin/true`), so these two take about twice as long for every
shell as they did under Ubuntu 24.04, and the shell's own share of their time is smaller.

\* BusyBox's times for `configure` and `build` don't measure the same work: the static BusyBox runs its own versions
of `sed`, `cat`, `basename`, `dirname`, `rm` and other utilities in the shell's process instead of running the
system's programs (`build` runs 204 programs under it, to 834 under dash), and as a static program it starts faster.
This counts for more with the slower utilities of Ubuntu 26.04.

`arrays` uses what zsh and bash scripts use beyond POSIX (arrays, associative arrays, `[[ ... ]]`,
`${x//pattern/replacement}`, `${x:offset:length}`), so dash and BusyBox can't run it. Its ratios are to luish:

| Benchmark | luish | bash | zsh |
|---|---|---|---|
| arrays | 0.142 (1.00) | 0.637 (4.47) | 0.663 (4.65) |

## Startup and single commands

Mean of 1000 runs for the first two rows and of 20 for the others, each shell run with a cleared environment
(only `PATH`, `HOME` and `LC_ALL=C`):

| Benchmark | dash | luish | bash | zsh |
|---|--:|--:|--:|--:|
| `sh -c true` | 0.34 ms | 0.51 ms | 0.55 ms | 0.74 ms |
| `sh -c /bin/true` | 0.66 ms | 0.80 ms | 0.85 ms | 1.03 ms |
| `while` loop, 100,000 iterations of `$((i+1))` | 86 ms | 45 ms | 193 ms | 209 ms |
| Loop running `/bin/true` 3000 times | 1.14 s | 1.15 s | 1.50 s | 1.67 s |
| Loop running `x=$(echo hi)` 3000 times | 0.30 s | 0.35 s | 0.62 s | 0.49 s |

Starting luish takes about 0.2 ms longer than dash, as luish is a larger program (5.5 MB, with the plugin support
built in): it loads more libraries and touches more memory (170 page faults to dash's 100). Starting a program costs
about the same as in dash. A command substitution takes about 1.2 times as long as in dash: it makes the same system
calls, but forking a larger process costs more.

## Interactive startup

With [cached startup files](usage.md#cached-startup-files), a new interactive shell doesn't run its startup files:
it restores the state they left, as shell commands saved in a file. Here the startup files load
[nvm](https://github.com/nvm-sh/nvm) 0.39.7 (`. "$NVM_DIR/nvm.sh"` in `rc.d`), and the saved state is 153 KB, with
110 functions. Mean of 100 runs, each started through `env -i` (which adds about 1 ms), measured on 2026-09-27 with
luish 0.1.0 (git revision `76b9576`), on a similar machine:

| Run | luish | dash | bash | zsh `-f` |
|---|---|---|---|---|
| `-c true` (for comparison) | 3.4 ms | 3.0 ms | 3.1 ms | 3.1 ms |
| Sourcing `nvm.sh`, without the cache | 20.7 ms | 14.6 ms | 27.9 ms | 106 ms |
| Parsing the saved state (`-n`) | 8.5 ms | 5.2 ms | 9.8 ms | 11.8 ms |
| Running the saved state | 8.9 ms | 5.9 ms | 11.2 ms | 11.2 ms |
| An interactive shell (`-i -c true`), from the cache | 8.2 ms | | | |

So an interactive shell with nvm set up starts in about 8 ms, however long the startup files take to run. nvm here
has no version of Node installed; with a default version, `nvm.sh` also runs `nvm use`, and takes much longer, as
do conda's initialization and other scripts that run programs, while the cached state stays the same size.

luish parses large files more slowly than dash: `nvm.sh` (144 KB) takes about 4 ms to parse in luish and 2 ms in
dash, which is most of the difference in sourcing it without the cache.

## Commands in Rhai

A plugin can add commands written in Rhai ([`sh::builtin`](extensions.md#commands-written-in-rhai-shbuiltin)).
`bench/extensions/` has four commands written both ways, as luish shell functions (`tasks.lsh`, using `local`,
arrays and associative arrays) and as an extension's built-ins (`ext.rhai`), which must print the same output:

| Task | What it does |
|---|---|
| collatz | One call: the number below 12,000 with the longest Collatz sequence (about a million loop iterations) |
| wordfreq | One call: counts the words of a 20,000-line file and finds the five most frequent (the shell reads it with `while read`, Rhai with `fs::read_file`) |
| urlencode | 8,000 calls from a shell loop, each percent-encoding a 60-character string into `REPLY` (the shell function with a `case` table, not a `$(printf)` per character) |
| calls | 300,000 calls from a shell loop of a command that adds two numbers into `REPLY`: the cost of a call |

The fastest of 10 runs of `bench/extensions/run.sh -r 10`, on 2026-09-29, on a machine with 16 cores:

| Task | Shell functions (ms) | Rhai built-ins (ms) | Shell / Rhai |
|---|--:|--:|--:|
| collatz | 834 | 380 | 2.2 |
| wordfreq | 550 | 189 | 2.9 |
| urlencode | 673 | 332 | 2.0 |
| calls | 472 | 758 | 0.6 |

Work inside a command (loops, arithmetic, strings, maps) runs two to three times as fast in Rhai. Rhai evaluates its
syntax tree much as luish evaluates shell code, so the gain is not larger. A command that does almost nothing is
faster as a shell function: calling into Rhai costs no more than calling a shell function (40,000 calls of an empty
command added about 5 ms either way to a loop that took 47 ms), but each Rhai operation costs more than the shell
expansion it replaces (`parse_int` twice, a string template and `sh::setvar`, against `REPLY=$(( $1 + $2 ))`).
The first extension creates Rhai's engine, once, which takes about 0.6 ms and 1 MB of memory (`-c` with `plugin load`
of an empty extension, against `true`); compiling and running `ext.rhai` takes about 0.6 ms more.

## Running the benchmarks

The benchmarks need [pixi](https://pixi.sh), and
[hyperfine](https://github.com/sharkdp/hyperfine) if it is installed. In a
checkout of luish:

```sh
pixi run bench                     # release build, then all benchmarks with every shell installed
bench/run.sh -n 4 -r 10 arith      # one benchmark, 4 times the work, 10 runs per shell
```

`bench/README.md` describes the options and the scripts. Absolute times depend
on the machine and its load, so compare shells within one run. On a machine whose cores differ in speed (such as
Intel's performance and efficiency cores), pin the run to one core, such as `taskset -c 3 bench/run.sh`, so that
every shell runs on the same one; and run one benchmark at a time. `bench/extensions/run.sh` runs the comparison of
commands in Rhai with shell functions.

