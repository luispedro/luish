# Performance

luish is meant to be as fast as dash, the fastest of the common POSIX shells, for scripts and `sh -c`, and it is:
work inside the shell is as fast as in dash or faster, and scripts that start many programs take at most 10% longer.
bash and zsh are up to five times slower on the same scripts. What luish adds to dash (plugins, the interactive
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

The measurements on this page were made on 2026-09-27, with the release build of luish 0.1.0 at git revision
`76b9576` (`pixi run release`), on a virtual machine with 4 cores running Ubuntu 24.04. The other shells are Ubuntu's
dash 0.5.12, bash 5.2 (`--posix`), zsh 5.9 (`--emulate sh`) and BusyBox 1.36. The machine is noisy: differences
under about 10% between two shells are within what separates two runs of the same one.

Seconds (and the ratio to dash; lower is faster), mean of 10 runs:

| Benchmark | dash | luish | bash | zsh | busybox |
|---|---|---|---|---|---|
| arith | 0.339 (1.00) | 0.215 (0.64) | 1.000 (2.95) | 0.598 (1.77) | 0.572 (1.69) |
| functions | 0.207 (1.00) | 0.184 (0.89) | 1.004 (4.86) | 1.133 (5.48) | 0.298 (1.44) |
| strings | 0.289 (1.00) | 0.292 (1.01) | 0.984 (3.40) | 0.808 (2.79) | 0.445 (1.54) |
| textproc | 0.203 (1.00) | 0.192 (0.95) | 0.543 (2.67) | 0.783 (3.85) | 0.310 (1.52) |
| configure | 1.463 (1.00) | 1.540 (1.05) | 1.854 (1.27) | 1.771 (1.21) | 1.563 (1.07) |
| build | 1.342 (1.00) | 1.459 (1.09) | 1.667 (1.24) | 1.658 (1.24) | 1.462 (1.09) |

The first four run mostly inside the shell, where luish is as fast as dash or faster. The last two spend most of
their time starting programs, which luish does with the same system calls as dash; they take 5% to 10% longer, mostly
because luish itself starts more slowly (below), and `build` starts a new shell for each file it compiles.

## Startup and single commands

luish against dash, mean of 1000 runs for the first two rows and of 10 or 20 for the others:

| Benchmark | luish | dash |
|---|---|---|
| `sh -c true` | 2.0 ms | 1.5 ms |
| `sh -c /bin/true` | 3.4 ms | 3.0 ms |
| `while` loop, 100,000 iterations of `$((i+1))` | 0.07 s | 0.13 s |
| Loop running `/bin/true` 3000 times | 4.08 s | 4.02 s |
| Loop running `x=$(echo hi)` 3000 times | 0.94 s | 0.83 s |

Starting luish takes about 0.5 ms longer than dash, almost all of it in the dynamic loader, since luish is a larger
program (4 MB, with the plugin support built in). Starting a program, a pipeline or a command substitution costs
about the same.

## Interactive startup

With [cached startup files](usage.md#cached-startup-files), a new interactive shell doesn't run its startup files:
it restores the state they left, as shell commands saved in a file. Here the startup files load
[nvm](https://github.com/nvm-sh/nvm) 0.39.7 (`. "$NVM_DIR/nvm.sh"` in `rc.d`), and the saved state is 153 KB, with
110 functions. Mean of 100 runs, each started through `env -i` (which adds about 1 ms):

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

The benchmarks need [pixi](https://pixi.sh), and [hyperfine](https://github.com/sharkdp/hyperfine) if it is installed.
In a checkout of luish:

```sh
pixi run bench                     # release build, then all benchmarks with every shell installed
bench/run.sh -n 4 -r 10 arith      # one benchmark, 4 times the work, 10 runs per shell
```

`bench/README.md` describes the options and the scripts. Absolute times depend on the machine and its load, so
compare shells within one run.
