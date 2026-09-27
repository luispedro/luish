# Performance

luish is meant to be as fast as dash, the fastest of the common POSIX shells, for scripts and `sh -c`, and it is:
work inside the shell is as fast as in dash or faster, and scripts that start many programs take about as long. bash
and zsh are up to five times slower on the same scripts. What luish adds to dash (plugins, the interactive
features, luish's own options) is opt-in, and costs scripts nothing.

## Scripts

These are the script benchmarks in the repository's `bench/` directory: realistic scripts of a few hundred lines, each
mixing parsing, expansion, function calls, built-ins and forks, rather than microbenchmarks. All shells print the same
output for each of them.

| Benchmark | What it does |
|---|---|
| arith | Mandelbrot set in fixed point, sieve of Eratosthenes, Collatz lengths |
| functions | Recursion (Hanoi, Fibonacci, Ackermann), wrappers, `getopts` in functions, a stack and a queue |
| strings | Pure-shell string processing: upper-casing, replacing, JSON and URL escaping, word frequencies |
| textproc | Generating and parsing an access log and an INI file with `while read` |
| configure | An autoconf-style `configure` script: many small forks, here-documents, `sed` |
| build | A make-like build that runs `$SH -c` for each object: fork, exec and shell startup |

Seconds (and the ratio to dash; lower is faster), mean of 10 runs, release build of luish, on a 4-core machine,
2026-09-27. The other shells are Ubuntu's dash 0.5.12, bash 5.2 (`--posix`), zsh 5.9 (`--emulate sh`) and BusyBox 1.36:

| Benchmark | dash | luish | bash | zsh | busybox |
|---|---|---|---|---|---|
| arith | 0.320 (1.00) | 0.196 (0.61) | 0.929 (2.90) | 0.553 (1.72) | 0.549 (1.71) |
| functions | 0.205 (1.00) | 0.158 (0.77) | 0.919 (4.49) | 0.992 (4.84) | 0.271 (1.32) |
| strings | 0.260 (1.00) | 0.254 (0.98) | 0.893 (3.43) | 0.747 (2.87) | 0.427 (1.64) |
| textproc | 0.203 (1.00) | 0.207 (1.02) | 0.513 (2.52) | 0.728 (3.58) | 0.320 (1.57) |
| configure | 1.149 (1.00) | 1.162 (1.01) | 1.453 (1.27) | 1.375 (1.20) | 1.265 (1.10) |
| build | 1.039 (1.00) | 1.086 (1.04) | 1.228 (1.18) | 1.236 (1.19) | 1.050 (1.01) |

The first four run mostly inside the shell, where luish is as fast as dash or faster. The last two spend most of
their time starting programs, which luish does with the same system calls as dash; the small difference is luish's
own startup (below).

## Startup and single commands

luish against dash, release build, 2026-09-26, on a loaded machine (so compare only within the table):

| Benchmark | luish | dash |
|---|---|---|
| `sh -c true` (average of 200 runs) | 1.19 ms | 0.90 ms |
| `sh -c /bin/true` (average of 200 runs) | 1.88 ms | 1.78 ms |
| `while` loop, 100,000 iterations of `$((i+1))` | 0.09 s | 0.09 s |
| Loop running `/bin/true` 3000 times | 1.80 s | 1.81 s |
| Loop running `x=$(echo hi)` 3000 times | 0.92 s | 0.90 s |

Starting luish takes about 0.3 ms longer than dash, almost all of it in the dynamic loader, since luish is a larger
program (with the plugin support built in). Starting a program, a pipeline or a command substitution costs the same.

## Interactive startup

With [cached startup files](usage.md#cached-startup-files), a new interactive shell doesn't run its startup files:
it restores the state they left, as shell commands saved in a file. On the author's setup, that file is 164 KB, with
120 functions (114 of them from nvm). Over 100 runs:

| Run | luish | dash | zsh `-f` |
|---|---|---|---|
| `-c true` (for comparison) | 3.1 ms | 2.2 ms | 6.2 ms |
| Parsing the cache file (`-n`) | 12.1 ms | 6.4 ms | 39 ms |
| Running the cache file | 14.2 ms | | 50 ms |

So the whole setup, nvm included, is in place in about 14 ms, however long the original files take to run (nvm's
and conda's initialization scripts alone commonly take hundreds of milliseconds). zsh takes more than three times as
long just to read the same definitions.

## Running the benchmarks

The benchmarks need [pixi](https://pixi.sh), and [hyperfine](https://github.com/sharkdp/hyperfine) if it is installed.
In a checkout of luish:

```sh
pixi run bench                     # release build, then all benchmarks with every shell installed
bench/run.sh -n 4 -r 10 arith      # one benchmark, 4 times the work, 10 runs per shell
```

`bench/README.md` describes the options and the scripts. Absolute times depend on the machine and its load, so
compare shells within one run.
