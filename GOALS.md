# Goals for luish

The long-term aim is for luish to replace zsh as my default shell. The core is
a POSIX `sh` that is as fast as dash. Extensions that go beyond POSIX cost
nothing when unused. Those that change behaviour are opt-in; those that only
give a meaning to what was an error may be always on.

Goals are grouped into stages. Each stage builds on the one before, and work
on a later stage does not start until the earlier one is usable. When goals
conflict, the earlier stage wins.

## Stage 1: Reproduce existing functionality (current focus)

- **POSIX conformance.** Implement the POSIX Shell Command Language and its
  required built-ins, plus `local` as in dash. Where the spec is ambiguous,
  match dash.
- **As fast as dash** for scripts that only use POSIX features, both at
  startup and during execution. Every later feature must be pay-for-what-you-use.
- **Usable interactively as a daily driver**: line editing, history,
  completion and job control, at least as good as a basic zsh setup.

## Stage 2: Beyond POSIX

- **Plugins**, loaded lazily so that they add no cost unless used.
- **Modern shell features**: arrays, associative arrays and process
  substitution (behind an opt-in where they would change behaviour).
- **Modern terminal features**: correct Unicode width handling in the line
  editor, true color in prompts, semantic prompt markers (OSC 133), working
  directory reporting (OSC 7) and bracketed paste.
- **Better scripting**: error messages with file, line and function stack,
  `pipefail`, and a predictable strict mode. Also debugging support.
- **Better interactive use**: richer tab completion, and history shared
  across sessions with metadata (working directory, exit status, duration).

## Stage 3: New capabilities

- **Caching of login scripts.** The final goal is to cache the *effects* of
  login scripts: environment variables, functions, aliases and options.
  Caching parsed scripts is a first step toward this. With a warm cache, a new
  shell should start almost instantly. Since this changes semantics, it must
  be explicit, and the cache must be invalidated correctly when its inputs change.
- **Built-in SSH support.** A client/server mode where the line editor runs on
  the client, so input is predicted locally. History could be managed on the
  client and shared across hosts. The line editor should be kept separate from
  the executor from the start so that this stays possible.
  - Initially, this runs over standard SSH. The only requirement is that the
    server-side component can be started on the remote host, so no extra
    daemon or network ports are needed.
  - Full-screen programs such as (neo)vim fall back to how SSH works today:
    the remote program drives the terminal directly.

## Non-goals

These are not a focus of the project. That does not mean outside
contributions for them will be rejected.

- Platforms other than Linux.
