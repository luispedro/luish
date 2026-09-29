# Performance

luish is meant to be as fast as dash, the fastest of the common POSIX shells,
for scripts and `sh -c`.

bash and zsh are up to six times slower on the same scripts. What luish adds
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

The script and startup measurements on this page were made on 2026-09-29, with the release build of luish at git
revision `9c469a3` (`pixi run release`), on a virtual machine with 4 cores running Ubuntu 24.04. The other shells are
Ubuntu's dash 0.5.12, bash 5.2 (`--posix`), zsh 5.9 (`--emulate sh`) and BusyBox 1.36. The machine is noisy:
differences under about 10% between two shells are within what separates two runs of the same one.

Seconds (and the ratio to dash; lower is faster), mean of 10 runs:

| Benchmark | dash | luish | bash | zsh | busybox |
|---|---|---|---|---|---|
| arith | 0.425 (1.00) | 0.308 (0.73) | 1.209 (2.85) | 0.810 (1.91) | 0.712 (1.68) |
| functions | 0.240 (1.00) | 0.226 (0.94) | 1.325 (5.53) | 1.502 (6.27) | 0.369 (1.54) |
| strings | 0.369 (1.00) | 0.415 (1.13) | 1.241 (3.37) | 1.083 (2.94) | 0.570 (1.55) |
| textproc | 0.280 (1.00) | 0.289 (1.04) | 0.766 (2.74) | 1.144 (4.09) | 0.487 (1.74) |
| configure | 1.651 (1.00) | 1.825 (1.11) | 2.160 (1.31) | 2.050 (1.24) | 1.819 (1.10) |
| build | 1.474 (1.00) | 1.632 (1.11) | 1.741 (1.18) | 1.894 (1.29) | 1.622 (1.10) |

The first four run mostly inside the shell, where luish is as fast as dash, or faster. The next two spend most of
their time starting programs, which luish does with the same system calls as dash; they take about 10% longer, mostly
because luish itself starts more slowly (below), and `build` starts a new shell for each file it compiles.

`arrays` uses what zsh and bash scripts use beyond POSIX (arrays, associative arrays, `[[ ... ]]`,
`${x//pattern/replacement}`, `${x:offset:length}`), so dash and BusyBox can't run it. Its ratios are to luish:

| Benchmark | luish | bash | zsh |
|---|---|---|---|
| arrays | 0.216 (1.00) | 1.134 (5.24) | 1.308 (6.05) |

## Startup and single commands

luish against dash, mean of 1000 runs for the first two rows and of 10 for the others:

| Benchmark | luish | dash |
|---|---|---|
| `sh -c true` | 2.1 ms | 1.7 ms |
| `sh -c /bin/true` | 3.5 ms | 3.2 ms |
| `while` loop, 100,000 iterations of `$((i+1))` | 0.10 s | 0.17 s |
| Loop running `/bin/true` 3000 times | 4.52 s | 4.24 s |
| Loop running `x=$(echo hi)` 3000 times | 1.19 s | 1.21 s |

Starting luish takes about 0.5 ms longer than dash, almost all of it in the dynamic loader, since luish is a larger
program (4.5 MB, with the plugin support built in). Starting a program, a pipeline or a command substitution costs
about the same.

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

luish parses large files more slowly than dash: `nvm.sh` (144 KB) takes about 6 ms to parse in luish and 2 ms in
dash, which is most of the difference in sourcing it without the cache.

## Running the benchmarks

The benchmarks need [pixi](https://pixi.sh), and
[hyperfine](https://github.com/sharkdp/hyperfine) if it is installed. In a
checkout of luish:

```sh
pixi run bench                     # release build, then all benchmarks with every shell installed
bench/run.sh -n 4 -r 10 arith      # one benchmark, 4 times the work, 10 runs per shell
```

`bench/README.md` describes the options and the scripts. Absolute times depend
on the machine and its load, so compare shells within one run.

