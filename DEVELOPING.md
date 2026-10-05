# Developing luish

This is the developer (and agent) documentation: how luish is built and tested, why it is built that way, what is
implemented and which tests cover it, and what is known about dash that the code depends on. User-facing behaviour is
documented in `docs/` (with the differences from dash and the known limitations in `docs/compatibility.md`); what is
still to be built is in `PLAN.md`, and the goals are in `GOALS.md`. `CLAUDE.md` has the commands, the test
conventions and an overview of the architecture; this file doesn't repeat them.

**Keep this file current**: update it (and the user docs) in the same commit as any change in behaviour. Every
behaviour listed here must stay covered by the tests named with it.

## Design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Target platform | Linux only | Linux-specific APIs (`/proc`, `clone(CLONE_VFORK)` through `posix_spawn`, ...) may be used wherever they help. No portability layer |
| Process creation | Raw `fork` + `execve`, and `posix_spawn` where possible | Subshells need a fork *without* an exec, which `std::process::Command` can't do |
| String type | `Vec<u8>` throughout | POSIX data is bytes |
| Execution model | Tree-walking interpreter over an AST | Simple, and fast enough for a shell |
| Line editing | rustyline, kept inside `interactive/` | Mature, with vi and emacs modes. The completer and highlighter get a plain-data snapshot (`Names`) instead of `Shell`, which keeps the editor separable for the SSH mode of Stage 3 |
| Plugins | Rhai, behind the `plugins` cargo feature, with the engine created on the first extension loaded | Pure Rust, so no system dependency. No threads, global state or signal handlers, so it is safe across `fork`. Scripts can be interrupted and resource-limited |
| Reference behaviour | dash (upstream for intent), then zsh beyond POSIX | See `docs/compatibility.md` |
| Arrays | Always on (associative ones too, made with `typeset -A`), with ksh/bash syntax and zsh's sh-emulation semantics (from 0, `$a` is `${a[0]}`) | Their syntax is an error in dash (`a=(`, `${a[i]}`), so no POSIX script changes, except that `a[i]=x` and `x+=y` stop being command names. zsh's native forms (`$a[1]`, 1-based) would change the meaning of POSIX scripts. `zsh --emulate sh` is then a reference for tests |

**Why Rhai.** The first plan used Python (through PyO3), dropped because linking libpython makes the dynamic loader
map it at every startup and makes the login shell depend on the system Python; an initialized interpreter
complicates forking without exec (the GIL, at-fork hooks, threads started by extensions) and signal handling; and a
runaway or crashing extension can hang or kill the shell. Rhai's costs are strings that can only hold UTF-8 (see
[Plugins](#plugins)), no library ecosystem (the `sh`, `fs` and `vcs` modules provide what extensions need), and a
language few people know. Lua (through `mlua`) was the main alternative: 8-bit clean strings and faster, but it
brings C code into the build.

**Why `toml-span`** for `config.toml`: it is small (no serde; its only dependency is `smallvec`) and keeps the
positions of keys and values, which give the lines for errors and the order of the keys (its tables are sorted maps,
so `config.rs` sorts entries by position; the order matters for `plugins.enabled`, which loads in the file's
order). TOML's bare keys can't contain `/`, so `plugins.enabled` takes `SOURCE.NAME = "*"` (a table, told apart from
an inline source by having none of `gh`, `git` and `path`; `SOURCE.SUB.NAME` nests, `Reader::nested`) as well as the
quoted `"SOURCE/NAME" = "*"`. `/` is the canonical form (plugins' names, `plugin load`, `@` imports), so names can
keep their dots.

**Why git plugins are fetched by running `git`**, not with libgit2 or gitoxide: no dependency, and git's own
configuration applies (credentials, SSH keys, proxies, `insteadOf`). Only `plugin sync`, `plugin update` and `plugin check` run it;
startup reads `plugins.lock` and looks for the extracted commits, so it never touches the network.

**Grouped settings.** luish's own options are bits in `Options` (so nothing on a hot path changes), named
`group.name` in `EXTENDED` (`options.rs`), with their earlier names and zsh's in `ALIASES`. Settings with a value
(`VALUES`) are backed by the variables they have always been (`history.file` is `$HISTFILE`), since `HISTFILE` and
`HISTSIZE` are POSIX variables and zsh shares the file. Names are only looked up when `setopt` runs. `-p` rather than
`-g` for the group, because `-g` means "global" to zsh's `typeset`.

## Repository layout

```
src/
├── main.rs             # C `main` (#![no_main]): command line, mode selection
├── shell.rs            # `Shell`: all interpreter state
├── sys.rs              # syscall wrappers (retry on EINTR)
├── stack.rs            # the guard against running out of stack, and the function depth limit
├── input.rs            # input sources: string, file, stdin, line editor
├── lexer.rs, parser.rs # one `Parser` struct: tokens, quoting, here-docs, aliases, recursive descent
├── ast.rs              # AST types
├── cmdtext.rs          # job text from the AST (dash's cmdtxt)
├── unparse.rs          # AST back to source text that re-parses exactly
├── prompt.rs           # zsh's % sequences (setopt prompt.percent)
├── style.rs            # styles and colour schemes (the `style` built-in is builtins/style.rs)
├── state.rs            # the shell's state as commands (savestate), and differences of states
├── startcache.rs       # cached rc.d / login.d
├── config.rs           # config.toml
├── vartrace.rs         # variable tracing (setopt vars.trace) and the `where` built-in
├── expand/             # mod.rs (driver, parameters, command substitution), arith.rs, split.rs, pattern.rs, glob.rs,
│                       # qual.rs
├── exec/               # mod.rs (lists, pipelines, compound commands), simple.rs (commands, lookup), fork.rs,
│                       # redirect.rs, cond.rs (`[[ ... ]]`), procsubst.rs (`<(...)`, `>(...)`), not_found.rs
│                       # (hints for commands of other shells)
├── vars.rs, options.rs, jobs.rs, signals.rs, path.rs
├── hash.rs             # the hash for the shell's tables (not SipHash)
├── builtins/           # mod.rs (table, special vs regular), one file per built-in or small group; help.rs
├── interactive/        # mod.rs (REPL, rustyline helper), history.rs, histfile.rs, bang.rs (history expansion),
│                       # complete.rs, menu.rs, keys.rs, highlight.rs, rprompt.rs (RPROMPT), firstrun.rs (the first-run menu),
│                       # jobmenu.rs (jobs -i)
└── plugins/            # mod.rs (the `plugin` built-in), package.rs (config.toml's [plugins], plugin.toml,
                        # plugins.lock), fetch.rs (git), add.rs (`plugin add`), rhai.rs, fs.rs, vcs.rs, bytes.rs
tests/
├── cases/              # differential cases (*.sh, with .expected/.status/.stdin where needed)
├── plugins/            # plugin cases (*.sh with .expected, .stderr)
├── compare.rs          # the differential harness
└── interactive.rs      # pty tests
fuzz/                   # cargo-fuzz targets (see Fuzzing)
bench/                  # script benchmarks, and extensions/ for Rhai commands (see bench/README.md)
scripts/                # dist.sh (release packages), test-install.sh (tests install.sh with them)
install.sh              # the `curl | sh` installer, which downloads a release
flake.nix               # the Nix package and dev shell (see Releases)
docs/                   # user documentation; docs/builtins/ is compiled into `help`; docs/examples/ has example plugins
luish-std-plugins/      # a collection of plugins (completion, bash-completion, notify), see its README.md
```

## Implementation notes by area

### Command line (`main.rs`)

- `-i` forces interactive mode; with `-c` or a script it runs that (it reads stdin only without them). As in dash,
  `+c` works like `-c`, `-l` (or `+l`) makes a login shell, and `-o interactive` and `-o stdin` work like `-i` and
  `-s`. `-o NAME` takes any option named as for `setopt`. Long options are listed in `docs/usage.md`. Tests:
  `options/interactive_c.sh`, `options/command_line.sh`, `tests/plugins/no-plugins.sh`.
- A missing script prints `cannot open X: No such file` and exits with 127. A script without `#!` (ENOEXEC) is
  re-run with this executable.

### Parsing (`lexer.rs`, `parser.rs`)

- Line continuation works inside `$` expansions (`$\<newline>?`), as in dash. Backquotes are unescaped, then parsed
  separately. `$((` falls back to `$( (...) )` (a deviation). As the `$((` inside are then read again, while one is
  read (or a subscript, below) `subst_memo` keeps what each `$((` and `$(` read as, or reading took time exponential
  in their nesting. Tests: `parse/*`, `parse/arith_fallback.sh`.
- Here-docs: several on one line, inside `$(...)`; bodies over 64 KiB go through an unlinked temporary file. One in
  `$(...)` with no body before the `)` is empty, as in dash (`read_subst_list` gives it the tree of an empty body,
  for `unparse`). Test: `parse/heredoc_in_cmdsubst.sh`.
- Aliases: a value ending in a blank makes the next word eligible wherever it is (also a `for` variable, `in`, or a
  `case` word, as in dash). Test: `parse/alias_blank_compound.sh`.
- `AliasMap` holds regular and global aliases in one table (a name is one or the other, as in zsh) and suffix aliases
  in another. Regular and suffix aliases are expanded by the parser in command position (`maybe_expand_alias`);
  global ones by `peek` for every token, only while `AliasMap::has_globals` (a count), except for a here-document
  delimiter (`next_raw`). All three splice text into `src` (`splice_alias`). A suffix alias being expanded is
  recorded in `active_aliases` as NUL plus its suffix, so it can't clash with a name. New parsers share one empty
  table (`NO_ALIASES`), so an `eval` doesn't allocate one. The completer's `Scan` mirrors these rules. Tests:
  `parse/alias_global.sh` (zsh), `parse/alias_suffix.sh` (zsh), unit tests in `complete.rs`.
- Function bodies may be any command (`f() echo hi`), as in dash.
- Process substitution: `read_word` reads `<(` and `>(` anywhere in a word (a syntax error in dash, so it is always
  on, and only `<` and `>` pay for the check) as a `WordPart::ProcSubst`, as bash and zsh do (`--input=<(cmd)`, and
  `2<(cmd)` is one word, not an fd number); `lex_token` sends a token that starts with one to `lex_word`. Not in
  `[[ x =~ ... ]]` (`regex_word`). The list is parsed as for `$(` (`read_subst_list`). Tests: `expand/procsubst*.sh`,
  unit tests in `unparse.rs`.
- As in dash, a bad `${...}` (such as `${x^^}`) is an error only when expanded, and `$(` in a here-doc delimiter
  is a syntax error. Test: `parse/dash_lenient.sh`. `ParamOp::Bad` keeps the text up to the `}` as it was
  (`read_bad_param`): it is read as a word only to find the `}`. A subscript with no `]` before the `}`, or one
  that doesn't parse (`${a['"'}`, where the subscript is read as in double quotes), makes a bad substitution, as
  does one that doesn't end where it ends read in the context of the `${` (as in zsh, `'` and `\` are read
  differently: else whether it is a subscript would depend on the text after the `}`), so
  `read_index` is followed by that second reading of the text. For nested ones (`${a[${a[...`) that took time
  exponential in the depth, so while a subscript is read, `param_memo` records where each `${...}` read ends, and
  the second reading (`skim`) skips them, and `subst_memo` has what each `$(` read as (as one with a here-document
  with no end reads to the end of the input). A `$(...)` that doesn't parse leaves the parser as it was (the
  here-documents waiting for their bodies), as reading goes on. Tests: `parse/bad_subscript.sh`,
  `parse/bad_subscript_cmdsubst.sh`, `parse/bad_subscript_heredoc.sh`.
- Arrays: `split_assignment_with` also takes `NAME+=` and `NAME[index]=` (the index up to the matching unquoted `]`,
  across parts), and `parse_simple` reads `(...)` right after an assignment's `=` (`array_follows` compares the
  token positions, so `a= (x)` stays an error) into a lone `WordPart::Array`. After `local`, `export`, `readonly`,
  `typeset` and `declare` (also after `command` or `builtin`), a `name=` argument followed by `(` gets the array as
  its last part (`is_declaration`): this is decided when parsing, as in bash. Under `glob.bare_qualifiers`, `(` after
  `name=` is not a qualifier. `${a[index]}` is `ParamExp::index`. An element of `(...)` is an `ast::ArrayItem`: one
  that starts with unquoted `[` and has `]=` (`split_subscript`, shared with `NAME[index]=`) has a key, which stays a
  word until the assignment decides whether it is an index or a key. Tests: unit tests in `parser.rs` and
  `unparse.rs`.
- `__luish_cache [env=(NAME...)] [files=(WORD...)] { list; }` (`parse_cache_block`, `ast::CacheBlock`): a reserved
  word, so only unquoted at the start of a command (`__luish_cache {` on a line of its own runs a command in dash,
  but the `__luish_` prefix is luish's). The options are arrays as in `local a=(x y)` (`array_follows`), each at
  most once; `env=` takes names, and `files=` words without command or process substitution (`forks`), since they
  are expanded at every start. The body must be `{ ... }`. Outside the startup files of `rc.d`, `login.d`, `$ENV` and
  `luishrc` (see Startup files), the body runs as a brace group and the options are ignored; the highlighter paints
  the options as arrays (`After::Cache`). Tests: `parse/cache_block.sh`, unit tests in `parser.rs`, `unparse.rs` and
  `highlight.rs`.
- `Parser::started` tells a buffer of blank lines apart from a real incomplete command. The lexer reads a trailing
  `(...)` as `WordPart::GlobQual` only under `glob.bare_qualifiers`: the only place it depends on an option.
- Unit tests in `parser.rs`, and the `parse` and `unparse` fuzz targets (see Fuzzing).

### Expansion (`expand/`)

- A double-quoted part is always a field, even when empty (`"$u"`, `"${u+x}"`), except a lone `"$@"` with no
  parameters. In command words `$@`, `$*` and `"$@"` give separate fields even when IFS is empty; elsewhere they are
  joined with the first character of IFS, as in dash.
- `$@` and `$*` always count as set for `${@-x}`/`${@+x}`, and are null for `${@:-x}` when their joined length is
  zero (counting separators by dash's rules, `varvalue`); `${#@}` is the joined length. Test:
  `expand/positional_ifs.sh`.
- `${x:offset:length}` and `${x/pat/rep}` (`ParamOp::Substring`, `ParamOp::Replace`), as in zsh's sh emulation.
  The lexer reads the offset up to a `:` at the top level, and the pattern up to an unquoted `/`, in a fresh quoting
  context as for `%` and `#`. `pattern::replace` tries, at each position, only the lengths the pattern can match,
  longest first. For `$@` and `$*` they apply to the list (`push_list`, shared with `$@`) and to each element, except
  in `"${*/...}"`, which replaces in the joined string, as zsh does. Tests: `expand/substring.sh` (zsh),
  `expand/substring_error.sh` (zsh), `expand/replace.sh` (zsh), `expand/substring_bad.sh`.
- Arrays (`vars::Value::Array`, boxed so that `Var` stays 32 bytes): `Vars::get` gives an array's first element, so
  everything that reads variables sees `$a` (an empty array is unset); `Value::elements` treats a string as one element. `a=x` sets element
  0. `expand_array` handles `${a[@]}` and `${a[*]}` with every operator, through `push_list` (shared with `$@`).
  It copies the elements for the operators in `array_op`, but not for a plain `${a[@]}`, `${#a[@]}` or a slice
  `${a[@]:i:n}` (which copies only the slice, after evaluating the offset and length): with the copy, `${#a[@]}` or a
  slice in a loop over the array was quadratic. `element` reads `${a[i]}`, which then goes through the scalar path. Assignments expand to `exec::simple::Assignment`
  (the index evaluated, the elements expanded as command words), made by `Shell::assign`; temporary ones before a
  command are saved and restored whole. A declaration command gets an array argument as `name=`, a NUL, and each
  element followed by a NUL (`builtins::vars::split_arg`), since an argument can't otherwise hold a NUL. Arithmetic
  reads `a[i]`, and evaluates the index of `a[i] = v` only once it has seen the assignment operator. `unset 'a[i]'`
  empties the element (`unset_element`). `quote_value` writes arrays for `set`, `-p` listings and `savestate`.
  Elements `[i]=v` of a list (`Shell::assign_items`) are evaluated as arithmetic when the list is assigned.
  Arrays have no holes, so an assignment past the end adds the elements before it; an index past `vars::MAX_INDEX`
  (64 Mi elements) is a bad subscript rather than an allocation that fails.
  Tests: `expand/arrays.sh` (zsh), `expand/arrays_errors.sh`, `expand/arrays_luish.sh`.
- Slices, `${a[i..j]}` (`Index::Slice`): `lexer::slice_index` splits a subscript at its first `..` in an unquoted
  literal part, so the ends are words of their own. `Shell::slice` evaluates them, then copies only the slice, which
  `array_op` expands as `${a[@]}` (`is_list` counts slices as lists, so `"${a[1..1]}"` gives no word). For an
  associative array, `slice_key` puts the key back together. Tests: `expand/array_slices.sh`, `subscripts` in
  `cmdtext.rs`.
- Associative arrays (`vars::Value::Assoc`, boxed): `vars::Assoc` keeps the keys in insertion order, with a hash map
  from key to position (removal is `swap_remove`). Whether a subscript is a key or an index is decided when it is
  expanded, from the variable's type (`Shell::subscript`, giving a `vars::Subscript`): a key is expanded as a string,
  an index as arithmetic; in arithmetic (`Arith::subscript`), a key is the text between the brackets, after the
  `$((...))` text was expanded. `Value::elements` gives the values, so `${h[@]}` and its operators need nothing more,
  and `Vars::get` gives the value at key `0`, as bash does. `Shell::assign_items` makes `h=(...)`, `read -A` and
  declaration arguments: pairs, or `[key]=value` (declaration arguments encode a key as `[key` and a NUL before the
  element's `=value`). In `${h[...]}`, `read_param_word_to` makes `\]` an escaped `]`, as in `h[...]=v` (the rest is
  lexed as in double quotes). `savestate` writes `typeset -gA name` before the value. Tests: `builtins/assoc.sh` (zsh),
  `builtins/assoc_luish.sh`.
- `${!a[@]}` and `${!a[*]}` (bash's keys) are `ParamOp::Keys`, lexed only as `${!name[@]}` or `${!name[*]}` directly
  followed by `}`. `${!prefix@}` and `${!prefix*}` (names) are `ParamOp::Names`, with the prefix as the name and the
  index `At` or `Star`. `expand_array` pushes either list with `push_list`. Any other `${!` followed by a name or a
  digit is an indirection (bash): the name becomes `ParamName::Indirect`, and the index and operator are read as usual
  (an `[@]` or `[*]` index with an operator is a bad substitution). A `${!` followed by anything else is still `$!`
  (`${!}`, `${!-x}`, `${!#}`). `expand_indirect` reads the value (of the element, with an index), parses it with
  `parse_reference` (a name, `name[index]` with the index as a literal word, digits, or a special character), and
  expands a `ParamExp` for it with the same operator (cloned; only indirections pay for it). `is_list` counts an
  indirection as a list, so that a lone `"${!x}"` gives no field when `x` is `@` or `a[@]` with nothing in it;
  `expand_indirect` sets `cur_exists` for other targets. Tests: `expand/array_keys.sh`, `expand/indirect.sh`.
- zsh's parameter flags: `${(` starts them (`Parser::read_flags`, into `ast::Flags`, boxed in `ParamExp::flags`,
  which keeps its text for `unparse` and `cmdtext`); the name, index and operator follow as usual, but not `#` or
  `!`. An unknown flag, a separator reaching `}`, or no name, make the whole a bad substitution. `expand_param` checks
  `flags` only after the `$x` fast path. `expand_flagged` expands the parameter and its operator as in double quotes
  into a list of words (`array_op`, split out of `expand_array`, takes the keys or pairs for `k`), turning `$*` and
  `[*]` into lists (or `$@` and `[@]` into joined words) following zsh's rules, then applies `j`, `s`, the case, `u`
  and the order, and pushes the words: quoted, or unquoted with `push_list` (split again), or for `s` with
  `push_literal` (not split, but globbed). The numeric order (`compare_words`) is zsh's: the first number that
  differs decides, then the bytes. A lone quoted flagged expansion counts as a list in `DoubleQuoted` (it sets
  `cur_exists`: a string gives one word, even if empty). Tests: `expand/param_flags.sh`, `expand/param_flags_luish.sh`.
- Arithmetic: a variable holding only blanks is 0. Quotes and backslashes inside `$((...))` are kept, so they are
  errors, as in dash. Test: `expand/arith_quotes.sh`.
- Command substitution drops NUL bytes (so does `read`) and sets `$?` only for commands of assignments alone (so
  `echo $(exit 3)$?` prints 0). Test: `expand/nul_bytes.sh`.
- Globbing: byte order, `.*` matches `.` and `..` (as in dash), `^` is not negation. A lone `[` without a matching
  `]` is not a pattern: treating it as one made every `[ ... ]` call `readdir` and loops 7× slower.
- `expand.braces` (`brace.rs`): done in `expand_word_into` on the parsed word, not in the lexer, so that the option
  takes effect in functions already defined (as bash's `set -B` does), and with it off the only cost is a test of the
  option per word. Unquoted literal bytes are atoms that can be `{`, `,`, `}` or `.`; every other part is one atom,
  kept whole, so quotes, `\{` and `${x}` don't count. A leading `Tilde` is turned back into text (the lexer took
  `~{,/tmp}`'s `{,` for the user name) and each word gets its tilde prefix again (`mark_leading_tilde`). As in bash,
  the first `{` whose match has a comma (or is a sequence) is expanded, with the items and the rest expanded
  recursively; a `{` that isn't one is skipped, and the search goes on after it (`{x{a,b}}` is `{xa} {xb}`). The ends
  of a sequence that have expansions are expanded once with `expand_word_str` (zsh's `{1..$n}`); if they don't give a
  sequence, their text, quoted, replaces them, so that they aren't expanded twice. The words of a sequence are
  `SingleQuoted` (`{Z..a}` has `[` and `\`). A redirection's target is expanded too, and more than one word is
  bash's `ambiguous redirect` (status 1). Tests: `expand/braces.sh` (zsh `-o noignorebraces`),
  `expand/braces_luish.sh`, `expand/braces_off.sh` (dash), unit tests in `expand/brace.rs`.
- `glob.star` (`glob.rs`): no hidden directories and no links (`***/` follows them, stopping at a link to a directory
  it is already in; zsh loops). Types come from `d_type` where available. Tests: `expand/globstar.sh` (zsh),
  `expand/globstar_off.sh`, `expand/globstar_loop.sh`.
- Glob qualifiers (`qual.rs`): kept as text and recognized when a field ends in an unquoted `(...)` at glob time, so
  that, as in zsh's `sh` emulation, they can come from an expansion. Supported: file type, permission, owner,
  device, link count, size and time tests, `^ - , N D n`, `o`/`O` (`n L l a m c d N`), subscripts (from 0), `M`,
  `T`, and the modifiers of `modify.rs`. Errors have status 1. `savestate` sets the option before the functions, and wraps a function
  with a qualifier in `set -o`/`+o` when it is off. Tests: `expand/glob_qualifiers.sh`,
  `expand/glob_qualifier_errors.sh`, `builtins/internal_savestate_globqual.sh`, unit tests in `qual.rs` and
  `parser.rs`.
- zsh's modifiers (`modify.rs`): `Modifier::read` reads one (a letter, and a count after `h` or `t`) and `apply`
  applies it, with zsh's results (`remtpath`, `remlpath`, `chabspath`, `chrealpath` in zsh's `hist.c`): `h` and `t`
  ignore trailing slashes and count runs of slashes as one, `h` keeps a leading `//` (but not `///`), `a` is relative
  to `getcwd` (not `PWD`, as in zsh) and canonicalized from the text (`cd::canonicalize`, so `/..` is `/` where zsh
  gives `//`), and `A` is `a`, then `realpath` of the longest prefix that exists, with the rest appended (found by a
  binary search: the prefixes that resolve are the first few, and trying each from the end took quadratic time). The lexer
  reads `${name:X...}` with a letter `X` as `ParamOp::Modify` if every `:` is followed by a modifier up to the `}`
  (anything else is a bad substitution, as before, so `${x:h-y}` is too); `expand_slice_op` and `array_op` apply it
  as they apply `Replace` (to each element, or to the joined string in `"${*:t}"` and `"${a[*]:t}"`). Glob qualifiers
  and history expansion (`a` and `A` only; its other modifiers are bash's) use the same functions. Tests:
  `expand/modifiers.sh` (zsh), `expand/modifiers_luish.sh`, `expand/glob_qualifiers.sh`, unit tests in `modify.rs`
  and `bang.rs`.
- The call stack (`frames.rs`) is `Shell::frames`, innermost last: `main` pushes a `FrameKind::Script` frame for
  the script before `run_input`, `misc::run_file` a `Source` frame for `.`, `source` and plugins' `.lsh` files (the
  path as found), `interactive::run_file` one for a startup file, and `call_function` a `Function` frame with the
  function's name and file (`Function::file`, the innermost frame's when it was defined), so a call costs two
  reference counts and a push; it also restores `LINENO` on return. Each frame has the line it was called from
  (`call_line`: `LINENO` then, 0 for the script and startup files) and whether its lines are its file's
  (`lines_in_file`). bash's `BASH_SOURCE`, `FUNCNAME` and `BASH_LINENO` are specials computed from it
  (`stack_elements`): innermost first, unset without frames, and `FUNCNAME` only while a function runs, with
  `main` and `source` for the other frames; a frame outside a file has an empty file (bash's `main` or
  `environment`). `caller` (`misc::caller`, `Shell::caller`) is bash's, `NULL` included. As restoring a saved state
  defines the functions again, `state.rs` writes `__luish_internal function-file NAME FILE LINES DIR` after a
  function with a file, so that the startup cache and `savestate` keep it. Its text is written anew, so `LINES` keeps
  the original line numbers of its body (`unparse::body_lines`, as differences: `12,1,0,3`). They are kept as text
  (`Function::pending_lines`) until the function's first call (`Shell::give_lines`, from `call_function`), as most
  functions in a startup cache are never called: applying them to nvm's 110 functions at startup cost 7% more
  instructions for a cached interactive start, and keeping them as text costs 3% (lexing the longer text).
  `unparse::set_body_lines` then gives them to a copy of the body read again: `unparse::walk_lines` visits every line number in an
  order that only depends on the tree's structure, which is the same for the printed text (checked by the round-trip
  tests and the `unparse` fuzz target). If they don't fit, or for a function whose lines weren't its file's (defined
  by `eval`), `lines_in_file` is false. `DIR` is the directory of a relative file (`SourceFile::dir`). Walking a
  tree to read it copies it, and here-document bodies (shared, in a `RefCell`, with the tree that may be running) are
  only borrowed for reading then (`LineVisitor::WRITES`). Tests: `misc/bash_source.sh`, `misc/bash_source_startup.sh`, `misc/call_stack.sh`,
  `builtins/internal_savestate_aliases.sh`, `exec/error_stack.sh`, `state::tests::lines`.
- Error messages (`Shell::error`, which `berr` and syntax errors go through) start with `error_location`: the file
  of the innermost frame and `LINENO` (no line if `lines_in_file` is false), else `$0` as in dash. `stack_trace` then
  adds a line for each frame but the script and startup files, innermost first, with where it was called (`file:line`
  from the frame below, `line N` in `-c` and on standard input, nothing at an interactive prompt) and, below it, that
  line's text; consecutive equal lines are counted, and more than `MAX_STACK_LINES` lose their middle. Between the
  two, `error_line_text` shows the failing line. Both use `line_text`, which reads the frame's file again (once per
  message, `TextCache`) (`SourceFile` keeps the directory of a relative path, so this works
  after `cd`) or, for `-c`, from `/proc/self/cmdline` (`Shell::command_arg` is the argument's index), so nothing is
  kept for it. `run_string` (`eval`, traps, `fc`, plugins' shell code) counts `Shell::in_string`, which a new frame
  saves (`Frame::saved_in_string`) and clears (`push_frame`, `pop_frame`): its lines go on from the current line,
  so no text is shown for a line there, nor for a call made there, and a function defined there has `lines_in_file`
  false. All of this is only computed for an error. The lines that
  luish adds start with two spaces, so cases compared with dash that capture stderr drop them (`sed '/^  /d'`, as in
  `builtins/test_parse.sh`, `expand/arith_quotes.sh` and `exec/recursion_limit.sh`). With `interactive::links` (an
  interactive shell, stderr a terminal the editor supports, not `terminal.no_integration`), the names of files are
  OSC 8 links to them (`SourceFile::shown`, `file://HOST/PATH` as for OSC 7; none if the path isn't absolute). Tests:
  `exec/error_stack.sh`, `exec/stack_guard.sh`, `error_links` in `tests/interactive.rs`.
- zsh's special parameters (`RANDOM`, `SECONDS`, `EPOCH*`, `UID`/`EUID`/`GID`/`EGID`, `HISTCMD`, `pipestatus`
  and bash's `PIPESTATUS`, and the constants `LUISH_VERSION`, `LUISH_PATCHLEVEL` (`GIT_REV` from `build.rs`),
  `MACHTYPE`, `HOSTTYPE` and `OSTYPE`, in `vars.rs`) are not in the variable map, so plain lookups and assignments of other names cost only a check of the
  first byte. They are computed on a miss (`Shell::special_value`, also in arithmetic), and a bit per special
  records whether it is set (`unset` clears it, assigning `RANDOM` or `SECONDS` sets it; assigning another makes it
  an ordinary variable). `RANDOM` is libc's `rand() & 0x7fff`, as in zsh, seeded on first use, and again in a
  forked child unless it was assigned. `SHLVL` is an ordinary variable incremented in `main` (`bump_shlvl`).
  `pipestatus` is the array `Shell::pipestatus`, read through `Shell::special_elements` where an array is expanded
  (`$pipestatus` is its first element). `run_pipeline` sets it for a single command (not an assignment or `[[`, as
  in zsh); for several, `wait_foreground` (without job control) or `wait_job` does. `local` and temporary
  assignments save a variable with its special bit (`Vars::save`, `Shell::restore_saved`), so that a special made
  ordinary there is special again afterwards. Tests: `expand/special_vars.sh` (zsh), `expand/special_vars_luish.sh`,
  `expand/version_vars_luish.sh`,
  `expand/pipestatus.sh`, `misc/shlvl.sh`, `histcmd_shlvl` and `pipefail_job_control` in `tests/interactive.rs`.
- zsh's `path` is a special too, read as `PATH` split at colons (`Shell::tied_elements`). Only array assignments
  tie it, as they are errors in dash: `Shell::set_var_value`, `append_elements` and `set_element` turn them into an
  assignment of `PATH` while it is special (`assign_tied`, which also checks that `path` isn't read-only), while a
  string assignment (`Vars::set`) makes it ordinary, as for `UID`, so that dash scripts can use the name. `local`
  makes it ordinary (and unset, as in dash), `typeset -a path` leaves it alone, `path=(...) cmd` also saves and
  exports `PATH` (`with_temp_assigns`), and `remember_command` doesn't look up a command with it. `from_env` takes
  it from the environment as an ordinary variable. Tests: `expand/path_ordinary.sh` (dash), `expand/path_tied.sh`.
- zsh's `dirstack` is tied to `Shell::dirstack` in the same way (`Special::is_tied` covers both): an array
  assignment replaces the stack without checking the directories (as zsh: `popd` reports a missing one and drops
  it), and `dirstack=(...) cmd` saves and restores the stack (`with_temp_assigns`). Tests:
  `expand/dirstack_ordinary.sh` (dash), `expand/dirstack_tied.sh`.
- Unit tests in `split.rs`, `pattern.rs` and `arith.rs`; cases in `expand/*`.

### Execution (`exec/`)

- Simple commands follow dash's `evalcommand` order: expand words (one at a time until the command is known, for
  declaration built-ins), make the redirections in the shell (for every command kind; a forked external command
  inherits them), then expand the assignments, so `x=$(cat) <<EOF` reads the here-doc. The `set -x` trace goes to the
  stderr from before the redirections. `RedirError::Open` (status 2; exits for a special built-in) vs
  `RedirError::Flow` (fatal expansion error). Test: `exec/assign_redirect_order.sh`.
- Here-strings (`<<< word`): the lexer's `Op::TLess` (`<<<`, a syntax error in dash, so always on), a
  `RedirKind::HereString` with an ordinary word target. `expand_here_string` expands it without splitting or globbing
  and joins the fields of `"$@"` with spaces (bash, zsh); with a newline added, it goes through `heredoc_fd` like a
  here-document's body. Tests: `exec/here_string.sh` (zsh), `parse/here_string_error.sh`.
- Assignments before regular built-ins, functions and external commands are made in the shell (as in dash), so a
  read-only variable is an error of the shell. Tests: `exec/readonly_assign.sh`.
- Command cache: (file, index in `PATH`), trusted without a stat; `with_command_path` retries later `PATH` entries on
  ENOENT (dash's `shellexec`). `cd` drops entries from relative directories. A search makes one `stat` per directory
  and an `access` only for a regular file. The shell looks commands up before forking, so the cache lasts and a
  missing command costs no fork; in a pipeline, each simple command whose name is a literal word is looked up (dash
  remembers them through `vfork`). Interactive shells `stat` the `PATH` directories after each line and clear the
  cache if one changed (device, inode or mtime). Tests: `exec/path_cache.sh`, `exec/hash_stale.sh`,
  `exec/hash_temp_path.sh`, `builtins/hash_pipeline.sh`, `path_cache` in `tests/interactive.rs`.
- `posix_spawn` (glibc uses `clone(CLONE_VM|CLONE_VFORK)`) for simple foreground external commands when
  `can_spawn()`: not interactive, no job control. `vfork` itself can't be called safely from Rust (the child would
  share the parent's stack). glibc's `posix_spawn` doesn't fall back to `/bin/sh` on ENOEXEC; `spawn_argv` retries
  with this shell. A child under job control must call `tcsetpgrp` itself (else it can get SIGTTIN first); glibc
  2.35+ has `posix_spawn_file_actions_addtcsetpgrp_np` if interactive shells ever spawn. Test: `exec/spawn.sh`.
- Process substitution (`exec/procsubst.rs`): `process_subst` makes a pipe and forks (`NoJob`, like `$(...)`); the
  child puts its end on fd 1 (`<(`) or 0 (`>(`) after closing the parent's ends of earlier substitutions (a `>(...)`
  would never see EOF otherwise), and the parent moves its end to fd 64 or higher (`high_fd`, without close-on-exec,
  so `exec 3< <(cmd)` can't close its own descriptor when it releases the pipe; bash uses 63) and records it in
  `Shell::procsubs`. `run_command` remembers the length of that list and calls `end_procsubs` when the command is
  over, error or not: it closes the descriptors, waits for `>(...)` processes, and leaves `<(...)` ones to be reaped
  without blocking (`procsub_orphans`, since a process that ignores SIGPIPE, or that has more to do, mustn't hold the
  shell up). `exec` without a command (`exec 3< <(cmd)`) uses `detach_procsubs`, which doesn't wait. `run_external`
  doesn't `exec` in place while the list is not empty, or nothing would close and wait. Tests: `expand/procsubst*.sh`.
- The `exit` flag (`run_list_exit`, dash's `EV_EXIT`) is passed to the last element of lists and and-or lists, to
  `if`/`case` bodies and brace groups without redirections, but not to loops, negated pipelines, or compound
  commands with redirections (as in dash). So `$!` is the command itself. Tests: `exec/exec_last.sh`,
  `exec/c_exec_last.sh`, `exec/async_pid.sh`.
- Signals are blocked across `fork` while a signal is trapped or ignored, or the shell is interactive or doing job
  control, until the child has reset its dispositions: a signal sent right after `fork` (`sleep &` then `kill %2`)
  used to be lost.
- Redirections: `n>&n` does nothing even if `n` is closed; `>&word` with a word that isn't a number or `-` is a fatal
  syntax error, as in dash. Tests: `exec/redirect_dup.sh`, `exec/redirect_big_fd.sh`.
- A command that isn't found is reported by `report_not_found` (`exec/not_found.rs`), from all three places that
  find out (`look_up_before_fork`, and `exec_error` after `posix_spawn` or `execve`), so the hint is the same whether
  the shell spawns, forks or execs. After `NAME: not found` it looks `NAME` up in `FALLBACKS`, whose functions get the
  command's words and may return a hint (`shopt` suggests `setopt`, translating the options luish knows). This is the
  place for further suggestions (such as similar command names); it runs only for a missing command, so it costs
  nothing otherwise. A name with `/` gets no hint. Test: `exec/not_found_hint.sh`.
- Exec errors other than EACCES give 127, as in Debian's dash (e.g. `ELOOP`). `wait_for` prints a message for deaths
  by signal except INT and PIPE.
- `set -e`: as in dash, only simple commands, subshells and pipelines (and a compound command whose redirection
  fails) exit on their own status, so `{ false && true; }` doesn't exit. Inside `$(...)` the suppression is reset.
  Tests: `errexit/compound.sh`, `errexit/cmdsubst_condition.sh`, `errexit/*`.
- A function can't be named after a special built-in ("Bad function name").
- `function` is a reserved word (`parse_function_keyword`), so `FunctionDef` holds a list of names (zsh's
  `function f g`). Names are unquoted literal words: any but those with `/` or of special built-ins, so
  `unparse.rs` writes a name that isn't a valid variable name, or is a reserved word, or several names, after
  `function` (`f()` wouldn't read back). A second name that opens a compound command (`if`, `for`, ...) starts a
  bash-style body instead; zsh would take it as a name. Tests: `parse/function_keyword.sh` (zsh),
  `parse/function_keyword_bash.sh` (`.expected`; zsh's sh emulation rejects these bodies).
- `[[ ... ]]` (`CompoundCommand::Cond`, parsed by `parse_cond_or` and evaluated in `exec/cond.rs`): `[[` and `]]`
  are reserved words, and inside the lexer's tokens are used as they are: `<`, `>`, `(`, `)`, `&&` and `||` are
  operators, and a word is an operator only as an unquoted literal, so no new token kinds are needed. Operators are
  recognized by position, as in zsh: after a unary operator comes its operand, unless that is a binary operator with
  a word after it (`-n = x`). The right side of `=~` is lexed with `Parser::regex_word`, which makes `(` and `|` word
  characters and keeps anything inside parentheses in the word (bash's rule); `read_word` checks it only at a
  delimiter, so other words don't pay for it. A lone word is kept as `-n word`, which is how `unparse.rs` and
  `cmdtext.rs` write it (`CondExpr::write`, which adds the parentheses that precedence needs; `=` is written `==`).
  Words are expanded as `case` expands them (`expand_word_str`, and `expand_pattern` for the right side of `=`),
  only when evaluated; the `set -x` trace is built during evaluation, so it shows only those parts. File tests reuse
  `builtins/test.rs`, so with `-nt` and `-ot` a file that exists is newer than one that doesn't, as in bash (zsh: both
  must exist). `=~` uses `regcomp`/`regexec` (`REG_EXTENDED`), without `setlocale`, so it matches bytes, as
  patterns do. The number of groups is `re_nsub`, which the `libc` crate keeps private, so `re_nsub` in `cond.rs`
  reads it at its offset (glibc or musl, unit test `exec::cond::tests::groups`); `match`, `mbegin` and `mend` are set
  only if there are groups, and `BASH_REMATCH` always, all as ordinary arrays. An error in an arithmetic operand is a shell error (status 2, as for `$((...))`; zsh uses 1). Like
  a simple command, `[[` exits under `set -e` on its own status (`run_pipeline`). The highlighter paints the
  expression's operators, and `]]` as a keyword (`After::Cond`). Tests: `parse/cond.sh` (zsh),
  `parse/cond_regex_match.sh` (zsh), `parse/cond_regex_rematch.sh` (`zsh -o bashrematch`), `parse/cond_regex_bash.sh`, `parse/cond_newer.sh` and `parse/cond_xtrace.sh` (`.expected`), unit tests `parser::tests::cond` and
  `unparse::tests::layout`.
- Recursion (`stack.rs`): as in Debian's dash (its patch 0009, for Debian bug 579815), a function call when 1000 are
  running is a shell error, `Maximum function recursion depth (1000) reached`; unlike dash, `func_depth` also goes
  down when the error unwinds. Other deep nesting would overflow the stack, which kills the shell with SIGSEGV
  (there is no overflow handler, and an alternate signal stack would cost startup syscalls). `stack::ok()` compares
  the address of a local with one recorded in `main`, and reads `RLIMIT_STACK` only past 1 MB of stack (every
  time, since `ulimit -s` can change it), keeping 256 KB spare. It is checked where nesting recurses:
  `run_list_exit` (functions, `eval`, `.`, traps, compound commands), `expand_parts` and `arith_text` (nested
  words), `arith.rs`'s `expr` and `unary`, the parser's `parse_command`, and the lexer's `read_dollar` (nested
  `$(`, `${` and `$((`, which don't go through `parse_command`). The error is `nested too deeply`, status 2. It
  costs a comparison per list, word and `$`, which the benchmarks don't show. A release build parses about 4000
  levels of `( ... )` with 8 MB of stack, a debug build about 600, and a debug build runs out before 1000 function
  calls (so `exec/recursion_limit.sh` raises `ulimit -s`). Tests: `exec/recursion_limit.sh`,
  `exec/stack_guard.sh`.

### Jobs (`jobs.rs`, `builtins/jobs.rs`)

- Without job control only background jobs are recorded, with no command text (as in dash); `JobTable::reclaim`
  imitates dash's `makejob`: making a new job frees the first finished job that `wait` has reported. `fork_child`
  calls it for the first process of each job.
- `wait_foreground`: without job control it waits for each pid directly; with job control it records a job and uses
  `wait_job`, which reaps any child (`waitpid(-1)`) until the job stops or ends.
- Under job control: `setpgid` in parent and child, `WUNTRACED`, a job killed by SIGINT makes the shell act as though
  it got the SIGINT (as dash). After a job exits normally the terminal modes are kept (so `stty` works); after it
  stops or dies from a signal they are restored (as bash; dash doesn't).
- Job text follows dash's `cmdtxt` (`$x` becomes `${x}`, single quotes become double quotes, `$(...)` elided,
  assignments dropped). The stopped-jobs warning only lets you out with an *immediately* repeated `exit`.
- `wait` uses dash's statuses (127 for an unknown pid, 2 for an unknown job; only a pipeline's last pid names it).
  `kill` is a port of dash's; signal names follow dash's table (any case, no `SIG`, `RTMIN+n`/`RTMAX-n`, no name for
  16), also for `trap`, which takes no options. Tests: `builtins/jobs.sh`, `builtins/kill_job.sh`,
  `builtins/kill_trap_signals.sh`, `tests/interactive.rs`, unit tests in `cmdtext.rs`.
- **`jobs -i`** (`interactive/jobmenu.rs`), an extension (`-i` is an illegal option in dash): a menu of the jobs, in raw
  mode on fd 0 and drawn on stderr below the command as the first run's is, erased when left. It needs both to be a
  terminal that can move the cursor (`TERM` not `dumb`), and otherwise fails with status 2, as dash does. The loop
  polls the terminal (every 500 ms, 100 ms while a `KILL` is pending) and reaps children in between, so the rows
  follow the jobs; no job is freed while it is open (`show_job` isn't used), so a job that changed is reported at the
  next prompt. Rows are in job-number order, so the selection is stable. `f`, `b` and `s` act at once; signals that
  end a job take two keys (`K`, then one of `SIGNALS`; any other key cancels). `Shell::signal_job` signals the process
  group, or without job control only the processes that haven't been reaped (whose pids may have been reused). A
  stopped job is continued (`restart_job`) after any signal but `KILL`, as bash and zsh do, so that it acts on it.
  `e` sends `TERM` and records a `Kill`; `KILL` follows after `ESCALATE` (5 s). Nothing could send it once the line
  editor has the terminal, so leaving waits until every such job has ended (Ctrl-C leaves without), which also lets
  the prompt report it. `Menu` (keys and drawing) doesn't see `Shell`. Tests: unit tests in `jobmenu.rs`, `jobs_menu`
  in `tests/interactive.rs`, `builtins/jobs_menu.sh` (without a terminal).

### Built-ins (`builtins/`)

- Ported from dash nearly verbatim, so compare with dash's source before "fixing": `test`'s parser, `getopts`,
  `umask`, `ulimit`, `kill`, `describe_command` (`command -v`/`-V`, `type`), `single_quote` (output of `set`,
  `export -p`, `alias`, `trap`), `number()` (strtoimax, 0..INT_MAX, for `exit`, `return`, `shift`, `kill`, `wait`).
  Tests: `builtins/test_parse.sh` (every expression of up to four arguments from a set of tokens),
  `builtins/test_newer.sh` (`-nt` and `-ot` with a missing file),
  `builtins/getopts_dash.sh`, `builtins/umask_modes.sh`, `builtins/ulimit_dash.sh`, `builtins/command_describe.sh`,
  `builtins/quoting_output.sh`.
- `echo`: `-n` only, XSI escapes always, dash's `\0nnn` and `\nnn`, and Debian's `\e`. `printf`: numeric conversions
  through libc `snprintf`, unsigned ones through `strtoull` (`-1` wraps), `strerror(ERANGE)` for out-of-range, status
  2 for an invalid directive, no options. Test: `builtins/printf_escapes.sh`.
- `getopts`: `OPTIND` moves past an argument as soon as its first letter is read, `OPTARG` is left alone at the end,
  the position is reset by assigning `OPTIND`, `set --` and `shift`, and saved across function calls; `OPTIND` must
  be a number.
- A built-in whose output can't be written prints `name: I/O error` and its status gets bit 1 (dash's `evalbltin`).
  Test: `builtins/write_errors.sh`.
- `set -x` doesn't trace commands run while `PS4` is expanded (`in_ps4`, dash's `inps4`), which used to loop forever.
  Test: `options/xtrace_ps4_subst.sh`.
- `export`, `readonly`, `local`, `typeset`, `declare` and `setopt` expand assignment-like arguments as assignments, as dash 0.5.12 does,
  also through `command` and when the name comes from an expansion (`declaration_command` in `expand/mod.rs`).
  `name=value` without a tilde to expand or an array is expanded in place (`expand_plain_declaration`); the rest go
  through `split_assignment_with`, which copies the word's parts. Tests: `builtins/declaration_args.sh`,
  `builtins/local_forms.sh`.
- `cd` and `pwd` use the logical directory (dash's `curdir`); a valid `$PWD` at startup is used without `getcwd`. Tests:
  `builtins/cd_logical.sh`, `builtins/chdir.sh`, `builtins/cd_e.sh`.
- `cd.auto` (`autocd_target` in `exec/simple.rs`) costs nothing unless the option is on. Test: `builtins/autocd.sh`.
- Directory stack (`dirstack.rs`, `Shell::dirstack`): `pushd` goes through `cd`'s code (`CDPATH`, `PWD`, `OLDPWD`,
  `chpwd`), but `-P` wins over `-L` as in zsh; only `-q`, `-L` and `-P` are options. Not implemented: zsh's
  `PUSHD_MINUS`, `PUSHD_TO_HOME`, `DIRSTACKSIZE`, `cd +n` without `pushd.auto`, the `dirstack` array. Tests:
  `builtins/dirstack.sh`, `builtins/dirstack_interactive.sh`, `builtins/auto_pushd.sh`, `builtins/pushd_silent.sh`,
  `builtins/popd_dir.sh`.
- `unset` of a bad name is an error; `set -` turns off `-x` and `-v`; `.` of a directory reads nothing. Test:
  `builtins/special_misc.sh`. `source`: `builtins/source.sh`, `builtins/source_missing.sh`.
- `builtin` (`misc::builtin`) is a regular built-in that calls the one named, passing its errors through, so a special
  one's errors still exit (as in zsh), but assignments before it are temporary (as in bash). Its arguments aren't
  expanded as assignments (`declaration_command` doesn't skip it), as in zsh and bash. Test: `builtins/builtin.sh`
  (zsh).
- `let` (`misc::let_`) evaluates each argument with `arith::eval`, as zsh does: status 1 if the last value is zero,
  or on an error, which stops at that argument (with the `$((...))` message, but no exit). No arguments is an error
  (status 1) and a leading `--` is skipped, as in zsh. Test: `builtins/let.sh` (zsh).
- `typeset` and `declare` (`vars::typeset`) share `vars::declare` with `local`, which differs in keeping the value
  (dash) where `typeset` starts a local unset (zsh and bash), in rejecting `-g`, and in being special. Outside a
  function, `typeset x` puts a `Var` without a value in the map, so `typeset -p` finds it. A local made by `typeset`
  hides a read-only variable, as in zsh (bash refuses). `-p` prints `typeset -aArx name=value` with `quote_value`;
  a special that is still special (`Shell::special_var`) is printed with its value, as an array for `pipestatus`,
  `path` and `dirstack` (which reads back as an array assignment, so `path` stays tied), but a listing without
  names includes only the specials that have attributes (as `set` doesn't list them).
  `-a` and `-A` convert a string, and refuse to convert one kind of array to the other (status 1, as in bash, and
  the other names are still declared). Not implemented: zsh's other options, and `-F` (zsh's floats, bash's names of
  functions). Tests: `builtins/typeset.sh` (zsh), `builtins/typeset_luish.sh`, `builtins/typeset_special.sh`.
- `typeset -f` (`vars::print_functions`) prints definitions with `unparse::function`, as `savestate` does, so they
  read back (the layout differs from zsh's and bash's, which differ from each other); `+f` prints the names, as in
  zsh. A name that isn't a function gives status 1 without a message (zsh and bash). With `-f`, variable attributes
  are an error (status 2) and `name=value` too (status 1, bash's error; zsh ignores both); `local` has no `-f`, as in
  zsh. Tests: `builtins/typeset_functions.sh` (zsh: listings, statuses, reading back),
  `builtins/typeset_functions_luish.sh` (layout, errors).
- `typeset -i`, `-l`, `-u` and `-U` set `Var::transform` (a `vars::Transform`), and `Vars::transforms` records that
  some variable ever had one, so that `Vars::transform` costs a flag test when no script uses them.
  `Shell::try_set_var` (hence `set_var`, `read`, `for`), `set_var_value`, `set_element`, `assign_items` and
  `append_elements` convert the value (each element) with `Shell::convert`: `-i` evaluates it with `arith::eval`
  and stores it in decimal, `-l` and `-u` change the case of ASCII letters (as `${x:l}` does). `x+=v`
  (`exec::assign`) and `a[i]+=v` add for `-i`, and convert the joined string otherwise. Arithmetic assignments
  (`$((x=1))`) store numbers already, through `Vars::set`. `-U` removes repeated elements (`vars::dedupe`) after
  each array assignment, and from `PATH` when it is set as a string (zsh does it for its other colon-separated
  specials too). On a tied `path`, `-U` dedupes before `assign_tied` sets `PATH`, which then applies `PATH`'s own
  attributes, so `-U` on `PATH` applies to `path` assignments too (zsh keeps the two apart). As in zsh and bash,
  `local` (which keeps the value, as in dash) and temporary assignments before a command (`with_temp_assigns`) drop
  the attributes, and `unset` removes them. Setting one converts the current value (zsh). Values are converted when
  assigned, as bash does, where zsh converts `-l` and `-u` ones when they are read (so after `typeset +l` zsh shows
  the original); that costs nothing on reads. Tests: `builtins/typeset_integer.sh` (zsh),
  `builtins/typeset_integer_luish.sh` (arrays, `typeset -p`, decimal output), `builtins/typeset_case.sh` (zsh),
  `builtins/typeset_unique.sh` (zsh), `builtins/typeset_case_luish.sh` (arrays, `+l`, `typeset -p`, `path`).
- `read -A` (zsh) and `read -a NAME` (bash) split the line with the same `next_field` as `read` uses for all names
  but the last, so there is no empty element after a trailing delimiter (bash; zsh has one). Tests:
  `builtins/read_array.sh` (zsh), `builtins/read_array_luish.sh`.
- `__luish_internal` (`internal.rs`) holds luish's own commands, so they don't take names from the command
  namespace; a missing or unknown subcommand is status 2. `print-git-rev` is set at compile time by `build.rs`
  (`-dirty` if `src/`, `build.rs`, `Cargo.toml` or `Cargo.lock` differ). Test: `builtins/internal_git_rev.sh`.
- `complete LINE` (`interactive::completions`) builds a `ShellHelper` as `read_line` does (the `Names` snapshot, and
  `ask` through the same `SHELL` pointer) and prints the matches Tab offers for the last word, one per line: the
  replacement for the word (with the suffix of a single match), a tab and the description. It works in any shell,
  so plugin cases can test completers. Status 1 if there are none or a completer failed. Test:
  `tests/plugins/complete.sh`.
- `savestate` (`state.rs`): not `PPID`, `LINENO`, `SHLVL`, or the options `-i -s -m -n`. Functions are printed by
  `unparse.rs`, which keeps all quoting (unlike `cmdtext.rs`), and escapes a `$` before backquotes, which it writes as
  `$(...)` (`unparse::tests::dollar_before_backquotes`); `${x/pat}` is written without the last `/` (`${x//}` would be `//`,
  `unparse::tests::words`), and a command named like a reserved word keeps its redirections first (`>f for`); a
  backslash at the end of the input is read as `\\` (so it stays itself in `$(...)`); words in function bodies that would be expanded as
  aliases (command names that are aliases of any kind, other words that are global aliases) are quoted, and a
  function named like an alias is preceded by `unalias`. Loaded plugins are printed as
  `__luish_internal plugin restore NAME PATH`, after aliases and before options. When there are aliases, the commands
  from them on are grouped in `{ }` (`join`), which is parsed before any of it runs, so that a global alias doesn't
  change the words after it (`set -o NAME` ...). Tests: `builtins/internal_savestate.sh` (a new shell reading the
  state prints the same state), `builtins/internal_savestate_aliases.sh`, unit tests in `unparse.rs`.
- `help` (`help.rs`) shows the Markdown in `docs/builtins/` (compiled in with `include_str!`). It is a built-in only
  in shells started with `-i` (which `set` can't change, so also their subshells). Unit tests check that every
  built-in has a page, that pages fit 80 columns and that `docs/builtins.md` includes them all. On a terminal
  (`builtins::style::stdout_sgr`, as for `where`), it uses the colour scheme's highlighting roles: `keyword` for
  headings, `string` for code spans (then shown without backticks), `command.builtin` for the synopsis's command
  names, bold for terms and `**`; `sh` code blocks go through the line editor's highlighter (`classify`), with
  command names taken as built-ins or else external commands, whatever the shell has defined. Tests:
  `builtins/help_noninteractive.sh`, `builtins/internal_help.sh`, `help_builtin` in `tests/interactive.rs`, unit
  tests in `help.rs`.
  When adding a page: every name, aliases such as `declare` included, needs an entry in `TOPICS` (kept sorted); the
  summary (the first paragraph after the synopsis) is at most 60 characters and rendered lines at most 79; the page
  must not contain relative links (Sphinx rejects them). After a failing `pixi run docs`, `rm -rf docs/_build`
  before rebuilding, or the cached build hides the warnings.
- `clipcopy` (`clipcopy.rs`), a built-in only in shells started with `-i` (and `__luish_internal clipcopy`), opens
  `/dev/tty` first (so nothing is read without a terminal), reads its file or stdin to the end, and writes
  `ESC ]52;c;BASE64 BEL` there in one write. Its own base64 encoder (no crate for 20 lines). Tests:
  `builtins/internal_clipcopy.sh` (errors: the cases have no terminal), `clipcopy` in `tests/interactive.rs`, unit
  tests in `clipcopy.rs`.
- `print` (`print.rs`) is zsh's, a built-in only in shells started with `-i`, as `help` is, and
  `__luish_internal print` anywhere. Its options are parsed as zsh parses them (`options`), not with
  `builtins::options`: an option's argument is the rest of its word or the next word, a word such as `-1` ends the
  options, and after `-R` only words of `e` and `n` are options. Its escapes are zsh's `getkeystring` (`key`), not
  `echo`'s. Each argument goes through the escapes, then `prompt::expand` (`-P`), then `dirstack::abbreviate`
  (`-D`), in that order, as in zsh; `\c` drops the arguments after its own. `-f` is `printf::format`, `-m` uses
  `name_pattern` (as `alias -m`), `-c`/`-C` lay out columns as zsh's `bin_print` does. `-s` adds a history entry
  after the command's own (`ShellHistory::add_entry`), which is then no longer current, so `fc` doesn't replace it;
  `-z` pushes onto a stack (`PUSHED` in `interactive/mod.rs`) that `read_line` pops to start a command line, after
  a `history.verify` refill. Tests: `builtins/print.sh` (zsh, through `$SH -i +m -c`, as the zsh reference runs
  natively there), `builtins/print_luish.sh`, `print_builtin` in `tests/interactive.rs`, unit tests in `print.rs`.
- `local x` keeps the current value, as in dash (also an array's, and with `-a`).

### Options (`options.rs`)

- dash's table order; `$-` lists letters in reverse table order. luish's own options have no letter and aren't in
  `set -o` or `$-`. An unknown option, or `interactive` and `stdin`, is status 1 (the other names are still set).
- `pipefail` (POSIX 2024) is in `OPTIONS`, without a letter, next to `hashall` where dash has `debug`. Its setting
  when a pipeline starts decides the status: `wait_foreground` reads it (nothing can change it while the shell
  waits), and a job records it in `Job::pipefail` for `Job::status`, which `wait_job` and `wait` use. The status of
  a stopped job is still that of its last process when that one stopped. `jobs` shows the last process's status, as
  bash does. Tests: `options/pipefail.sh` (zsh), `options/pipefail_async.sh`, `pipefail_job_control` in
  `tests/interactive.rs`.
- `setopt -p GROUP`: an unknown group is status 1 (nothing set), a missing one or another option letter status 2. The
  completer completes group names after `-p`, and the group's names after `-p GROUP`.
- Tests: `options/setopt.sh` (zsh), `options/setopt_list.sh`, `options/setopt_values.sh`, `options/setopt_group.sh`,
  `options/*`.

### Variable tracing (`vartrace.rs`)

- `setopt vars.trace` (or `vars.trace_history`) creates `Shell::vartrace`, and turning both off drops it; options go
  through `Shell::set_option` (`setopt`, `set -o`, `config.toml`) so that it follows them. Command-line options set
  the bits directly, and `main` calls `update_var_trace(true)` after `bump_shlvl`, so that variables that didn't come
  from the environment show as set by luish when it started. When tracing starts, each variable that is set gets a
  first record: `Environment` if it is exported with the value the environment had, else `Startup` or `Before`.
- Zero cost while off: `vartrace` is an `Option<Box>`, and each place that changes a variable tests it and calls a
  `#[cold]` function: `Shell::after_assign` (which `try_set_var` now goes through, so every assignment through
  `Shell` is covered), `Shell::unset_var`, `Shell::save_var` and `Shell::restore_saved` (`local`, temporary
  assignments, typeset's local unset), arithmetic assignments (`arith.rs`, which change `Vars` directly), and
  `unset 'h[k]'`. `with_plugin_vars` and `bump_shlvl` aren't traced (the first is undone, the second runs before
  tracing starts). The interleaved benchmarks show no difference with tracing off; with it on, `functions.sh` takes
  about 20% longer (`vars.trace`) and 35% (`vars.trace_history`).
- A record (`Event`) has where the code was (`Location`: the innermost frame's file, `LINENO` if `lines_in_file`, the
  innermost function frame, else the prompt, `-c` or standard input), and, for a variable whose history is kept
  (`vars.trace_history`, or one of `HISTORY_VARS`), the value as shown. Otherwise only the last record is kept,
  without its value, which is the current value. At most `HISTORY_LIMIT` (100) are kept per variable, with a count
  of those dropped.
- `local` and temporary assignments: `save_var` pushes the variable's last record on a stack per name
  (`VarTrace::saved`), which `restore_saved` pops, so that a `Restored` record has the record that set the value put
  back (`origin`, followed through earlier restores), which plain `where` shows. Saves and restores pair up per name
  in LIFO order; one made before tracing started has no origin.
- The startup caches aren't used while tracing (`Run::lookup` finds nothing): entries are rebuilt, so the files run
  and what they set is recorded at their lines. The prompt's `prompt-vars` run with `vartrace` taken out, since
  their changes are put back after the prompt (`restore_var` isn't traced).
- `where` is in `builtins::INTERACTIVE`, and `Shell::builtin` also finds it while tracing (only after the main table
  missed, so lookups of other commands pay a test of `vartrace` only on that path). `__luish_internal where` is the
  same everywhere. The old plugin case that named a plugin built-in `where` now uses `plugindir`.
- `where` prints each change as a `Line` (what was done, the value, where), on one line or, if that is wider than the
  terminal (80 columns when stdout isn't one), with the value and where on indented lines of their own; `-a` decides
  once per variable, so its numbered entries line up. Colours come from highlighting roles (`var`, `string`, `path`,
  `command.function`, `comment`) through `builtins::style::stdout_sgr`, which `plugin`'s `Ui` also uses: none unless
  stdout is a terminal and `$NO_COLOR` is unset. Widths leave out escapes (`Text`); unit tests in `vartrace.rs`.
- Tests: `misc/vartrace.sh`, `misc/vartrace_restore.sh`, `misc/vartrace_history.sh`, `misc/vartrace_startup.sh`,
  `tests/plugins/vartrace_prompt.sh`.

### Interactive mode (`interactive/`)

- **History** (`history.rs`) implements rustyline's `History` trait (its `FileHistory` can't remove an entry or keep
  stable numbers). `add_current` marks the newest entry as the running command, also when it was a duplicate and not
  added, so `fc` leaves it out; `fc -s`/`-e` call `remove_current` and add what they run. `interactive::with_history`
  borrows the editor's `RefCell`: never run commands inside its closure. `fc` re-running `fc` is limited to 4 levels
  (dash's `MAXHISTLOOPS`). The reference for `fc` is upstream dash's `histedit.c` (`histcmd`, `str_to_event`) plus
  POSIX; bash was used to check how the `fc` entry itself is treated.
- **History file** (`histfile.rs`): zsh's extended format (metafied; `\` before embedded newlines and a space after a
  final `\`), and luish's old `#V2` is still read. Appends under an `fcntl` lock (mode 0600, creating the directory);
  trims through a temporary file past 20% over `SAVEHIST`. With `history.share`, one `stat` before each prompt when
  nothing changed: the shell remembers the file's size and last entry seen, which it looks for if the file was
  replaced. It is read after the startup files, so they can set `HISTFILE`/`HISTSIZE`. Tests: `history_file` and
  `share_history` in `tests/interactive.rs`, unit tests in `histfile.rs` (including lines written by zsh),
  `builtins/fc_noninteractive.sh`, `fc_history` in `tests/interactive.rs`.
- **History expansion** (`bang.rs`, `history.expand`): runs on each line the editor returns, in `run_incremental`
  through `Input::expand_history` (only for `Input::Editor`, so scripts, `-c` and piped input never reach it), before
  the line joins the pending text; a line without `!` (or a leading `^`) costs one `contains`. The line's quoting
  context is found by scanning the pending text of the command first (`Scanner`: quotes, `$'...'`, `$(...)`,
  `$((...))`, backquotes, comments and here-document bodies). `!` is literal where bash's
  `bash_history_inhibit_expansion` makes it so (`$!`, `${!`, `[!`) and before `"`, `=`, `(`, blanks and operators.
  An expansion longer than 1 MiB (`MAX_LEN`) is an error, as each `!#` doubles the line and a `:gs` repeats its
  replacement for each match (`bang::tests::too_long`).
  Results: the line (echoed to stderr, as bash and zsh do), `Again` (`history.verify`: `REFILL` starts the next
  `read_line` with it, unless rustyline doesn't support the terminal, where it would be lost), or `Drop` (an error,
  or `:p`), which clears the pending text. `bang::Memory` keeps the last substitution and `?str?` across lines. The
  highlighter takes command names with `!` (or a leading `^`) as known when the option is on
  (`Names::history_expand`). Behaviour was checked against bash 5.2 and zsh 5.9 on a pty; where they differ, luish
  follows zsh, except where zsh's reading collides with sh syntax (`!"`, `[!`) or zsh fails where bash gives a result
  (`:h`, `:r`, `:e`), where it follows bash, as it does for comments (`docs/compatibility.md`). Tests: unit
  tests in `bang.rs`, `history_expansion` in `tests/interactive.rs`.
- **Prompts** (`prompt.rs`): the expansion keeps escape sequences apart from the text, and gives rustyline both (its
  `(raw, styled)` prompt), so the cursor position doesn't count them. Nothing is done unless the option is on and the
  prompt has a `%`. `%NG` counts as `N` spaces in the raw prompt, so `N` is limited to 65536 (`MAX_GLITCH`).
  `%[style:NAME]` and `%[style_off]` have only long names (internal letters `STYLE` and `STYLE_OFF`, control bytes
  that the short form ignores). `Expander::sgr` follows the attributes and colours set by the sequences (`%B`, `%F`
  and the like, not `%{...%}`); a style is added to them (`Style::add`) and pushed, and `%[style_off]` pops; both
  write the whole state (`ESC[0;...m`), as an SGR can't be undone otherwise. Both branches of `%(...)` are expanded,
  so each starts from the state before it, and the taken one's state is kept. The scheme in use is resolved once per
  expansion, when first needed; with `$NO_COLOR` both do nothing. Tests: `misc/prompt_percent.sh` (checked against zsh while written; zsh can't be the reference
  because its interactive mode writes more than the prompts), `misc/prompt_percent_long.sh`, `misc/prompt_style.sh`, `prompt_percent` in
  `tests/interactive.rs`, unit tests in `prompt.rs`.
- **The right prompt** (`rprompt.rs`): rustyline has none, so the highlighter appends it to the line between `ESC 7`
  and `ESC 8`, which leave the cursor where rustyline's layout (computed from the plain text) expects it. rustyline
  clears and redraws the line's rows on every refresh, so it is erased with them; `highlight_char` returns true
  while one is set, so that rustyline doesn't take its shortcut of writing a key typed at the end of the line alone,
  over the right prompt. The hinter notes the autosuggestion's width (rustyline gets the hint before the highlighted
  line), and `highlight_char`'s `ForcedRefresh` is rustyline's last refresh of an accepted line, where
  `prompt.transient_rprompt` leaves it out. It is expanded in `plugins::prompt`, with the prompt variables of
  plugins. Test: `right_prompt` in `tests/interactive.rs`, unit tests in `rprompt.rs`.
- **Key bindings** (`keys.rs`): luish's keymap (zsh's widget names and emacs bindings) comes before rustyline's. Keys
  are decoded as rustyline decodes xterm's sequences, so `^[OA` and `^[[A` are the same key. rustyline overwrites the
  count of any `Move`/`Kill` a handler returns with the numeric argument (`cmd.redo(Some(n))`), so only count-1
  searches work; `word_cmd` picks a rustyline motion that lands in the right place (so kills go to the kill ring),
  else edits through `Completer::update` (no kill ring). Widgets that need the history go through the completer
  (`Pending`) or the hinter (which records `history_index`). `history.rs::starts_with` implements the prefix search,
  sharing `Search` with the key handlers. Bindings changed with `bindkey` are part of the saved state. Keys given
  by name (`named`: `Up`, `Ctrl-X Ctrl-E`) are turned into the bytes xterm sends and decoded as the others; an
  argument is read as names only if every space-separated word is a name, has a modifier, or is one character (and
  some word isn't just a character), and `show` writes the first character in octal when its output would read as
  names (`\125p` for U, p), so listings and saved state read back as the same keys. Tests:
  `line_editor_keys` in `tests/interactive.rs`, `builtins/internal_bindkey.sh`, `builtins/bindkey.sh`, unit tests in
  `keys.rs` and `history.rs`.
- **Completion** (`complete.rs`): a rough tokenizer finds command position (quotes, operators, redirections,
  assignments, `$(`, backquotes, reserved words, commands such as `sudo` that take a command) and records where each
  unquoted byte ends in the line, so a non-prefix match can replace the text from where it stops matching. `PATH`
  executables are cached until `PATH` or one of its directories changes. rustyline's own listing is never used:
  `Completer::complete` returns 0 or 1 candidates (in `CompletionType::List`, one candidate is put in the line with
  `update`).
  After `${name[` (`subscript`), the word is the subscript, unquoted as the lexer's `read_index` does, and
  `Quote::Subscript` escapes `$`, backquote, `"`, `\`, `]` and `}` in what is inserted; the candidates come from the
  `subscripts` callback in `interactive/mod.rs` (through the `SHELL` pointer, as `ask`), called only then, so the
  `Names` snapshot taken before each prompt doesn't copy arrays. `items` lists subscripts that are numbers first, in
  numeric order (as zsh does for indices), then the others. Tests: `builtins/internal_complete_subscript.sh`,
  `subscripts` in `complete.rs`.
- **Expansion on Tab** (zsh's `expand-or-complete`, `ShellHelper::expansion`): a word ending at the cursor, outside
  quotes, with `*?[$` or a backquote in it, is parsed as the argument of `:` and expanded by the shell (the `expand`
  callback in `interactive/mod.rs`, through the same `SHELL` pointer as `ask`, so options such as bare glob
  qualifiers apply and substitutions run). The fields replace the word, quoted, with a space after them when there
  are several (as zsh). An empty result or the word itself unquoted (a glob without matches, `\*`) falls through to
  completion, as does the cursor right after `$`. `Scan::raw_start` keeps the start of the word across `$(...)`,
  which `Scan::start` forgets. Tests: `builtins/internal_complete_expand.sh`, `expand_or_complete` in `complete.rs`.
- **Completion menu** (`menu.rs`): drawn as rustyline's **hint** (a multi-line string starting with `\n`; rustyline
  includes it in its layout, skips SGR escapes when measuring, and erases it on accept), so rustyline isn't patched.
  `Hint::completion()` returns None so Right doesn't insert it. Menu keys are `ConditionalEventHandler`s that set
  `Menu::pending` and return `Cmd::Complete`; `Menu::step` makes the move, and the new text is the only candidate, so
  every change goes through rustyline's completion and undo. The menu is "open" only while `(line, pos)` equals what
  it last put there, so any other edit closes it. Handlers must be `Send + Sync`, hence `Arc<Mutex<Menu>>`; don't
  hold the lock while a plugin completer runs. Column widths are capped at the 90th percentile or a third of the
  screen. rustyline's default `keyseq_timeout` is None (a lone Esc waits for the next key); luish sets 400 ms in
  emacs mode (zsh's `KEYTIMEOUT`) and 100 ms in vi mode.
- **Highlighting** (`highlight.rs`): `classify` gives each byte a `Cell`, a role (`style::Role`, the position of a
  name in `style::ROLES`, made by the const fn `Role::of`, so a misspelt role doesn't compile) and a bitset of
  `MODIFIERS` (`error`, `path` and `path.prefix` are set so far). It needs no `Shell`: what it knows comes in `Facts`
  (the kind of a command name, whether a variable is set, `expand.braces`, `set -f`, and the path check with
  `highlight.paths`). `ShellHelper::command_kind` finds a command's
  `CommandKind` in the order the shell would (alias, special built-in, function, built-in or extension's built-in,
  then `PATH`; autocd directories last), cached until the next prompt; a known name in `PRECOMMANDS` gets
  `command.precommand` instead. Words in argument position, and array elements, get `arg` (`arg.option` if they start
  with `-`) after a pass over their bytes that have no role yet (so are unquoted and literal) for tildes (also after
  `=` and `:` in assignments), brace expansions (one pass, with a stack) and glob characters (also in `case`
  patterns). `<(` and `>(` start a word's process substitution anywhere in it, as in the lexer (so `2<(ls)` is not a
  redirection). `Colors` holds the resolved style and SGR parameters of every role (`interactive::colors` resolves
  them before each prompt only if `Styles::generation` or the scheme in use, which depends on `$LUISH_BACKGROUND`
  and `$COLORFGBG` (see The background), changed, and reports a scheme that isn't defined then); a cell with modifiers adds their styles
  to its role's (`Style::add`), cached per cell. `$NO_COLOR` and `editor.no_highlight` turn the line's colours off
  (the menu and suggestion keep theirs, or the old fixed ones with `$NO_COLOR`). `$NAME` and `${NAME}` get
  `var.unset` when `NAME` is not in `Names::vars`, unless an earlier word in the text is `NAME=...` (as an
  assignment or an argument, as for `export`) or a `for` name, or the cursor is on it. A variable that is set gets
  the role of its `VarKind` (`Names::vars` maps each name to one: read-only, else array, else exported, else
  plain, as in ble.sh), also in `${NAME[i]}`, `${#NAME}` and `${NAME:-x}` (which are never marked unset). Functions
  defined earlier in the text (outside subshells and substitutions) count as known, in place of an external or
  unknown command but not of a built-in, which might be special. Aliases defined earlier count only from the next
  complete command (a newline outside any compound command, `Scan::depth`), since that is when the shell takes them
  up; arguments of `alias` after an option (`-g`, `-s`) define nothing here. Syntax errors come from a dry parse
  (`syntax_error`): the real parser over the text (the `PS2` context and the line) and the newline that entering it
  adds, with `source_eof` false so that incomplete input isn't an error, and with the shell's aliases (the parser has
  no side effects: it only builds the tree). `Parser::error_span` gives where the last error was, in bytes of the
  original input: `unexpected` notes the token's span, `err` the peeked token's or the position, errors in
  backquotes and here-document bodies (parsed by a parser of their own) the whole of them; `splice_delta` maps back,
  roughly inside alias text. `error_start` decides where `error` starts: not at all while the cursor is at the end of
  the token with the error (a word being typed), nor for an error at the newline (`ls >`) while the cursor is after
  the last non-blank, otherwise marking the operators before the newline. The result is kept for the last text
  (`State::parsed`, reset before each prompt), as moving the cursor redraws the same text; texts over `MAX_PARSE`
  (64 KiB, about 3 ms to parse in release) aren't parsed. Nesting deep enough to need it stops at `stack::ok`, as the
  editor runs on the main thread. With `highlight.paths`, `Scan::path` asks `Facts::path` about each argument, array
  element and redirection target (also given `expand.tilde`) that has no expansion, glob or brace characters and isn't
  an option, with its quotes removed and a lone leading `~` replaced by `$HOME`. `ShellHelper::path_mods` gives
  `path` if `lstat` finds it (so a dangling symlink counts), else, for the word under the cursor, `path.prefix` if a
  name in its directory begins with its last component (the listing is sorted and binary-searched). Both are cached
  until the next prompt (`State::paths`, `State::dirs`), and each redraw may make `PATH_BUDGET` (16) uncached lookups;
  words past it stay unmarked until a later redraw. There is no thread, since subshells fork without exec. Unit tests
  compare one letter
  per byte for the top-level role (`classes`), or the runs with their full role (`roles`). Tests: `highlight.rs`
  (`command_roles`, `string_and_var_roles`, `var_roles`, `defined_on_the_line`, `expansion_roles`,
  `unset_variables`, `syntax_errors`, `syntax_errors_at_the_cursor`, `path_words`, `path_lookups` and others), the kinds in `candidates` in `complete.rs`, `syntax_highlighting` in
  `tests/interactive.rs`.
- **Styles** (`style.rs`, `builtins/style.rs`): a `Style` has optional colours, attributes on and off (bits by
  `ATTRS`), `plain` and raw SGR; `inherit` fills what one leaves out from its parent. `Styles` (on `Shell`, empty
  maps until used) holds the user's layer, plugins' defaults, the schemes defined, and the `Choice` (None for the
  built-in pair). The built-in schemes are const tables, made into a `Scheme` when looked up; changing one with `style
  -s` copies it first, but `-i` (which the saved state uses to start each scheme) defines an empty one, so a replayed
  state is exact. `Resolver::value` takes a name's value from the first layer that has it (user, then the scheme
  chain, then plugins), and `get` then walks the parents, so a scheme's specific name beats the user's general one.
  Names in `ROLES` are fixed; another name is free unless its first component is a role's (then it must be in
  `ROLES`) or it has no dot and is within edit distance 2 of a top-level role ("did you mean"). Undefined schemes
  are allowed in a choice from `config.toml` and in `inherits` (a plugin loaded later may define them), but not by
  `style -c`. Styles are saved state (`Kind::Style`, as `__luish_internal style` commands; `style::removal` undoes
  one). Tests: unit tests in `style.rs` and `highlight.rs` (`colors`), `builtins/internal_style.sh`,
  `misc/config_toml_style.sh`, `tests/plugins/manifest_style.sh`, `syntax_highlighting` in `tests/interactive.rs`.
- **The background** (`interactive::find_background`, `interactive/tty.rs`): before the first prompt with a dark/light
  pair chosen (the default) and colours on, unless `$LUISH_BACKGROUND` is `dark` or `light`, the shell takes it from
  `$COLORFGBG`, or else asks the terminal, if stdin and stderr are one and `TERM` is set and not one the editor
  doesn't support (`dumb`): in raw mode, it writes OSC 11 (`ESC ]11;? ESC \`) and DA1 (`ESC [c`) to stderr and reads
  until the DA1 answer, which every terminal gives, so a terminal that doesn't know OSC 11 costs no wait; the hard
  timeout (500 ms) is for one that answers nothing, whose late answers then reach the editor as keys. A colour is
  dark if its L* is below 50 (mid-grey is light). The result goes in `$LUISH_BACKGROUND` (not exported), and
  `interactive::detected_background` remembers where it came from for `style -c`. Bytes that aren't the answers
  were typed: those up to the first control character start the line (`push_buffer`), so Enter typed early runs
  nothing. It is asked once per shell (`BACKGROUND`); `style --detect` asks again. The pty tests set
  `LUISH_BACKGROUND` (`Pty::spawn_at`) except `background_detection`, which answers itself. Tests: unit tests in
  `tty.rs`, `background_detection` in `tests/interactive.rs`.
- **Underlines** (`style.rs`): `undercurl` and the other kinds (`UNDERLINES`) set the `underline` bit and
  `Style::under`, the subparameter of SGR 4 (`4:3`); `ul:COLOUR` is `Style::ul`, SGR 58 (`58;5;N` or `58;2;R;G;B`, as
  58 has no short form; 59 for `default`). A child inherits `under` only if it doesn't turn `underline` on itself, so
  `underline` under an `undercurl` parent is straight. Tests: unit tests in `style.rs` (`underlines`),
  `builtins/internal_style.sh`.
- **Terminal colours** (`interactive/termcolors.rs`): a `Scheme`'s `terminal` map (keys `style::TERMINAL_KEYS`:
  `foreground`, `background`, `cursor`, `palette`, values lists of RGB, one each but up to 16 for the palette) is
  resolved key by key through the chain (`Styles::terminal`); `style -s S terminal.KEY ...` sets one, `-r` removes
  it, and they are in the saved state as such commands. `terminal` and `terminal-colors(-colours)` are reserved as
  style names (`check_name`). `termcolors::update`, before each prompt (not continuation lines), after
  `find_background`, maps them to OSC keys (`(10|11|12, 0)`, `(4, N)`) and sends the sequences (`ESC ]11;#rrggbb
  ESC \`: `#rrggbb`, not the `rgb:R/G/B` that terminals answer with, which Konsole doesn't take) for what differs from `SET`, what the shell last set, putting back the keys no longer wanted. It looks again
  only when `Styles::generation`, the scheme in use or `wanted` (the `terminal-colors` setting, `$NO_COLOR`,
  `can_ask`) changed (`SEEN`). Before setting a key for the first time it asks the terminal for it (`tty::ask`, the
  same query as the background's, with one OSC `?` per key before DA1), keeping the answer if it is `rgb:...`, as an RGB (in
  `FOUND`); restoring sends that back (as `#rrggbb`, so at 8 bits per channel), or resets the key (OSC 110, 111, 112, `104;N`). Answering with the
  original rather than resetting is what makes a nested shell give back its parent's colours. `termcolors::restore`
  puts back everything: from `Shell::exit` (not in subshells), the `exec` built-in (not in subshells, before
  `exec_argv`), and `ask_background` (so `style --detect` and a later pair see the terminal's own background); it
  clears `SEEN`, so the next prompt sets them again. `terminal-colors = false` (`Styles::no_terminal_colors`) is in the
  saved state as `style --terminal-colors off`. Tests: unit tests in `style.rs` (`terminal_colors`), `tty.rs`
  (`color_answers`) and `termcolors.rs`, `terminal_colors` and `terminal_colors_exec` in `tests/interactive.rs`,
  `builtins/internal_style.sh`, `misc/config_toml_style.sh`, `tests/plugins/manifest_style.sh`.
- **Terminal integration** (`interactive/integration.rs`): with the line editor on a terminal it supports (`EDITS`)
  and fd 1 a terminal (checked each time), unless `terminal.no_integration` is set. Before a primary prompt,
  `before_prompt` sends OSC 7 (`ESC ]7;file://HOST/PATH BEL`, the path's bytes other than `[A-Za-z0-9/._~-]`
  percent-encoded) if `curdir` differs from the last one sent (`DIR`). The prompt given to rustyline is wrapped in
  `ESC ]133;A BEL` (`A;k=s` for `PS2`) and `ESC ]133;B BEL`, with the unwrapped text as the plain prompt that rustyline
  measures; rustyline redraws the whole prompt, so the marks are sent again on each redraw, which terminals take as
  the same prompt. `run_incremental` calls `Input::command_starts` (`ESC ]133;C BEL`) before each list read with the
  editor (after `preexec`) and `command_done` (`ESC ]133;D;STATUS BEL`, only if C was sent: `RUNNING`) after it, and
  does the same around a syntax error (status 2), so lines with several lists give one C/D pair each. All are written
  to fd 1, where rustyline writes. Other pty tests drop the marks from the transcript (`Pty::strip_marks`, unless
  `Pty::marks`). In vi mode, the cursor's shape (DECSCUSR, `ESC [N q`: 6 bar, 2 block, 4 underline, 0 the terminal's
  default) follows the input mode. rustyline tells the mode only to key handlers, as it was before the key, so
  `keys::Dispatch` (for the keys luish binds) and `keys::ViCursor` (bound to `Event::Any`, rustyline's fallback for
  the other keys; it returns `None`, so rustyline does what it would) call `integration::vi_key`, which predicts the
  mode after the key from rustyline's vi keys (`next_mode`); a wrong guess is corrected at the next key.
  `line_starts` (a bar: rustyline starts each line inserting) and `line_done` (back to 0, only if changed: `SHAPE`)
  are around `readline`. Tests: unit tests in `integration.rs`, `terminal_integration` and `vi_cursor_shape` in
  `tests/interactive.rs`.
- **Autosuggestions**: the hint while the cursor is at the end of a non-blank, non-continuation line and the menu
  isn't open; accepted with rustyline's `CompleteHint`. The search goes from the newest entry and stops at the first
  match. Test: `autosuggestions` in `tests/interactive.rs`.
- Tests: `tests/interactive.rs` and unit tests in `complete.rs`, `menu.rs`, `keys.rs`, `highlight.rs`, `rprompt.rs`,
  `history.rs` and `histfile.rs`.

### Startup files (`main.rs`, `startcache.rs`, `config.rs`)

- **First run** (`interactive/firstrun.rs`): before `config.toml` is read, an interactive shell reading from the line
  editor, with stderr a terminal (and without `--no-rcs` or `--no-plugins`), whose configuration directory is missing
  or empty (only `.` and `..`) shows a menu on stderr (`choose`): write `config.toml` with the recommended settings
  (`config_text`: `editor.autosuggest`, `prompt.percent`, `history.expand`, the default `history.file` as a comment, `std.completion`,
  plus `std.bash-completion` if bash-completion is where `bridge.bash` looks for it; the plugins only with the
  `plugins` feature), write the same text commented out (so the directory isn't empty next time), write a minimal
  file with only a personal plugin, or write nothing. The menu puts the terminal in raw mode (`Raw`, restored on drop;
  no `ISIG`, so Ctrl-C is a byte) and redraws its items in place (`ESC [ N A`, each line short enough not to wrap);
  `read_key` takes Up and Down (`ESC [ A`, `ESC O A`, also `k`/`j` and Ctrl-P/Ctrl-N), Enter, a digit (which
  chooses), and Ctrl-C, Ctrl-D, Esc (nothing after it for 50 ms) or `q` for nothing. With `TERM` unset or `dumb`, it
  lists the items and reads a number with `read_answer` instead. A plugin spec goes through
  `plugins::prepare_addition` (`add::prepare`, as `plugin add`, so a git source is fetched to see what it holds) and
  `add_to_config` (`add::add_to`) on an empty text; an error is printed and the menu comes back. The file is written
  once, and `plugin sync` runs unless it is all commented out; the normal startup then loads the plugins.
  The pty tests' `Pty::spawn_term` puts an empty `luishrc` in the configuration directory so that no other test sees
  the menu. Test: `first_run` in `tests/interactive.rs` (vt100 and dumb), and `config` in `firstrun.rs`.
  `__luish_internal default-config [--extra]` prints the recommended text (`firstrun::default_config`); `--extra` adds
  luish-extra as the source `extra`, with `extra.complete.all` and `extra.themes`, and a commented-out `[style]`
  colour scheme. `install.sh` writes that (it can't take the text from the first run, which needs a terminal).
  Test: `tests/plugins/default_config.sh` (a plugin case, as without plugins the text has none).
- Order: `config.toml`, `rc.d`, then the login files (`login.d`, or else `/etc/profile` and `~/.profile`), `$ENV`,
  `luishrc`. Login files run for login shells whether interactive or not, as in dash.
- A cache (`startcache.rs`) is a list of entries, each what one file or one `__luish_cache` block changed for one
  key: the difference between the state (`state.rs`) before and after it ran (`state::changes`, a list of
  `Change`s, so inherited variables that it doesn't touch aren't saved), with the fingerprints (device, inode, size,
  mtime) of the files it read with `.` (`Shell::sourced_files`, recorded while an entry is built). An entry's id is
  `config` (`config.toml` and the plugins it enables, keyed also on the fingerprint of `config.toml`, even when it
  doesn't exist), `file NAME`, `post-rc` (the plugins' `post-rc.lsh`) or `block HASH PATH`. The cache also records
  the build of luish (the git revision, plus a hash of the sources for a dirty build; see `build.rs`; another
  build's cache is discarded) and the directory. Written with a rename, mode 0600, only when an entry was built or
  dropped; the directory gets a `CACHEDIR.TAG` and a `README`. Every field is `TAG LEN`, a newline, then the bytes
  (`field`, `take_field`); a key is a list of items, each a tag and a length-prefixed value (`item`, `items`).
  Test: `misc/startup_cache.sh`.
- A file's key is its fingerprint, the values of `PATH` and `HOME` (`DEFAULT_ENV`, `v NAME=VALUE` or `u NAME`), and
  the chain: a hash of the id, key and changes of every entry before it in its directory (`Run::link`), so that a
  file runs again when one before it does something else, and an entry from before an earlier file was added is
  used again once it is removed. `rc.d` and `login.d` have separate chains, so `luish -l` and `luish -il` share
  `login.d`'s entries. `KEEP` (4) keys are kept per id, the oldest dropped first; for a file, storing an entry
  drops those for its other fingerprints. Entries that a startup doesn't use are dropped when their file is gone,
  or, for blocks, after `MAX_AGE` (30 days from when they were built). Test: `misc/startup_cache_blocks.sh`.
- A file with a block outside function bodies (`has_block`: a text search for `__luish_cache`, then a parse of the
  whole file with a walk of the trees) is "mixed": its entry only records that (`m FINGERPRINT`, no changes), and it
  runs every time with the `Run` in `Shell::startcache`, so that its blocks (`Command::Cache`, `run_block`) use the
  cache. A file is read and parsed only when no entry has its fingerprint. A block's id is the hash of its unparsed
  text (`unparse::cache_block`, so comments and layout don't count) and the startup file that runs it; its key is
  the values of `env=(...)` and the fingerprints of the expanded `files=(...)`, without the chain. Its status is
  saved (`R`) and returned when it is replayed; one that ends with `return`, `break` or an error isn't saved. The
  file in a block's id is that of the code running it (`Shell::current_file`: a plugin's file), or else the startup
  file. While an entry is replayed, or a block built, `Shell::startcache` is `None`, so a block inside just runs.
- While an entry is built (`cached`: `config`, a file's, `post-rc`), its `Run` is in `Shell::startcache` with
  `Run::nested` set, so that the blocks of the plugins it loads are entries of their own: looked up and stored as
  above, but not linked into the chain (the entry is, with what they changed among its changes). What they depend
  on is added to the entry's: the names of their `env=(...)` (`Nested::names`) become `v`/`u` items after the
  entry's key, with the values those variables had when the entry started (a snapshot of the variables, taken only
  when the entry is built), and `Cache::find_nested` accepts such items after the key if the variables still have
  those values; their `files=(...)` and the files they read with `.` (also when replayed) go to
  `Shell::sourced_files`, so become the entry's dependencies. A block that isn't saved (`return`, an error) keeps
  the entry from being saved (`Nested::unsaved`). `build` restores the `Var::assigned` marks of the enclosing entry
  after building a block (`Vars::assigned_names`, `Vars::mark_assigned`), so that the entry still saves what it
  assigned before the block. Test: `tests/plugins/startup_cache_nested.sh`.
- `_uncached.lsh` runs
  as a mixed file, after the cache is written (in case it exits); the cache is written again if its blocks built
  entries, or if unused entries were dropped, which is done last.
- Replay renders the changes (`render`, removals first, aliases and what follows grouped in braces by
  `state::join`) and runs them without expanding aliases (`Shell::run_text`): the text was written after expansion,
  and aliases from earlier entries mustn't apply. On the warm path: a stat per file and per `files=` path and
  sourced file, a directory read, and one file read per directory. Compared with one cache per directory, this costs
  about 2% more instructions for six files (callgrind).
- Besides the entries, the cache records when an entry was last built (`t`, seconds since the epoch), the options
  of the shell that built it (`m`: `-i`, `-l` or `-il`) and the environment it started with (an `e` field per
  `NAME=VALUE`; the shell never changes its own environment, so `std::env::vars_os` is the inherited one;
  `Cache::built_by`). They are for `__luish_internal check-cache` (`startcache::check`), which runs `luish MODE +m
  --internal-check-cache=NAME:TMP` in that environment, with `/dev/null` for 0 to 2. When that shell reaches the
  cache `NAME`, it rebuilds every entry (`Run::check`: lookups miss, except for the "mixed" records, which depend
  only on the text), and `check_child` writes the entries it used to `TMP` (next to the cache, mode 0600) and exits.
  `check` compares the build and directory, then each new entry with the old one of the same id and key (files and
  changes, `diff_changes`), or, failing that, with the newest of the same id (the items of the key that differ,
  except the chain, whose change is reported for its own entry), and reports files whose entries are gone. If
  nothing differs it touches the cache (`sys::touch`), otherwise it writes the old entries with the new ones stored
  over them. Recording the environment is what keeps `PATH=$HOME/bin:$PATH` from differing when the check runs in a
  shell whose `PATH` has it already. `+m` keeps the shell off the terminal. `_uncached.lsh` doesn't run in it (the
  `post-rc` hooks do), and a shell that doesn't reach the cache it is told to check (`rc.d` or `login.d` is gone, or
  the shell isn't interactive) reports so from `main` (`check_not_reached`), after `$ENV` and `luishrc` (so a
  `startup` check gets a chance to run first). Times are shown with `strftime("%c")` in local time, in the
  `LC_TIME` locale of the shell's variables (`sys::format_time`, which sets and restores the C library's locale
  around the call). Tests: `misc/startup_cache_check.sh`, `misc/startup_cache_blocks.sh`,
  `tests/plugins/startup_cache_check.sh`.
- Every variable an entry assigns is saved, even with the value it had: `Var::assigned` is set by `Vars::set` and
  `Vars::entry` (one store, no branch), cleared by `Vars::clear_assigned` when an entry starts being built, and
  makes `state::changes` include the variable (`Entry::assigned`; not `PWD`, which is the directory's). Otherwise
  `export CONDA_EXE=...` in a shell that inherited it isn't saved, and the variable is unset in a shell started
  elsewhere. Test: `misc/startup_cache_assigned.sh`.
- `$ENV` and `luishrc` (`interactive::startup`) aren't a directory and always run as `_uncached.lsh` does, but their
  `__luish_cache` blocks, and those of plugins loaded with `plugin load` while they run, are cached, in a cache of
  their own, `startup-HOST` (`startcache::begin_startup`, `finish_startup`): no file-level entries or chain, since
  neither file is cached as a whole, and no `_uncached.lsh`. `interactive::startup` wraps each file with
  `startcache::run_mixed` (shared with the "mixed" files of `rc.d` and `login.d`), so a plugin's `rc.lsh` dot-sourced
  while one runs sees `Shell::startcache` too, and its blocks are cached there (with the plugin's file in their id). `check-cache`
  takes `startup` as a third name, built the same way as `rc` and `login` (`Cache::built_by`'s `-i` branch, since
  neither file runs outside an interactive shell). Tests: `misc/startup_cache_blocks.sh`,
  `tests/plugins/startup_cache_blocks.sh`.
- Not yet done (see `PLAN.md`, Stage 3): `commands=(...)`, warning about variables a block reads but doesn't list,
  background revalidation, `flock` for many shells at once, and merging into running shells.
- `config.toml` is parsed with `toml-span`; errors are `luish: PATH: line N: ...`, in the file's order. A key directly
  under `[options]` is a setting by its `setopt` name. The `alias` table defines regular aliases, and its `global` and
  `suffix` tables the other kinds (so a string named `global` or `suffix` is a regular alias, and TOML won't have both
  in one file). The `bindkey` table goes through `keys::bind_widget`, as `bindkey KEY WIDGET` does. The `colorscheme`
  table's schemes replace earlier ones of the same name (`Styles::replace_scheme`), and a scheme's `terminal` table
  holds its terminal colours (`config::terminal_colors`: strings, the palette an array or a string); the `style` table's
  `colorscheme` is the `Choice`, `terminal-colors` (or `terminal-colours`, a boolean) the setting, and its other keys go
  to the user's layer. Tables under a style name are flattened (`[style.var] unset = ...` is `var.unset`), so unquoted
  dotted keys work. A directory plugin's `plugin.toml` shares the `options`, `alias`, `bindkey`, `colorscheme` and
  `style` tables (`config::load_plugin_manifest`, see Plugins); there, `style` sets plugins' defaults and can't have
  `colorscheme` or `terminal-colors`. Tests: `misc/config_toml.sh`, `misc/config_toml_style.sh`.
- `config.toml`'s `env`, `vars` and `path` tables (`config::environment`): `env` exports, `vars` doesn't (a variable
  inherited exported stays so), and a table named `interactive` in `env` is `env.interactive` (a string of that name is
  a variable, as for `alias.global`). Values are strings, with `tilde`, or integers. `path`'s directories are
  collected (`Dirs`) and applied after the whole file, so they add to a `PATH` set in `env` wherever the tables are;
  a directory `PATH` has already is skipped rather than moved, so that a shell started by `pixi shell` or
  `nix-shell` keeps the environment's directories first, and `PATH` is set (and exported) only if it changed. In an
  interactive shell they are part of the `config` cache entry (`PATH` and `HOME` are in its key already). A login
  shell that isn't interactive applies `env`, without `env.interactive`, and `path`, uncached
  (`config::load_login`, before the login files). Test: `misc/config_toml_env.sh`.
- The rc stage (`interactive::rc_d`, `startcache::run` with `config`) is: `config.toml`'s options, the plugins it
  enables (`plugins::load_enabled`), `rc.d`'s files, then every loaded plugin's `post-rc.lsh` (`post_rc_files`), all
  inside the cache; then, outside it and so in every shell, the `post-rc` hooks (`post_rc_hooks`), then
  `_uncached.lsh`. `Shell::in_rc` is set meanwhile, so that `plugin load` defers `post-rc.lsh` and hooks; outside
  it, a plugin runs them right after `rc.lsh`. With `config.toml` but no `rc.d`, the cache is still used (for the
  nonexistent directory). A startup with plugins that aren't installed or can't be resolved doesn't save the entry
  for `config.toml`, so the message repeats until they are. `--no-plugins` bypasses the caches (reading one would
  restore plugins' effects, and writing one would save a state without them). Tests: `tests/plugins/packages.sh`,
  `tests/plugins/post_rc.sh`.

### Plugins (`plugins/`)

- Until the first `plugin load`, the only state is `Shell::plugins` (`None`) and the only cost is the `None` check in
  `cd`. A plugin without an extension doesn't create the Rhai engine. Without the feature, `plugin load` fails with
  "luish was built without plugin support". CI also runs clippy and the tests with `--no-default-features`.
- `plugins/rhai.rs`: one `Engine` (with call-depth, expression-depth and size limits, and `eval` disabled: tests/plugins/noeval.sh), one AST per extension, so
  helpers with the same name in different extensions don't clash. `import` resolves relative to the file that
  imports (`Resolver`): each extension's AST and each module's AST and `Module` get the file's absolute path as
  their source and id, which Rhai passes to the resolver and gives the functions and closures defined there, so a
  module in a subdirectory imports its neighbours wherever its code is called from; code without a source falls back
  to the running plugin's directory (or the current directory, for `plugin run -c`). `import "@SOURCE/PATH/MODULE"` is `MODULE.rhai` in the directory of the loaded
  plugin called `SOURCE/PATH` (`other_plugin`), the longest name that fits, as MODULE can have a `/`; a plugin's path
  can't be the start of another's, as a directory is a plugin or a sub-collection, not both. Failing that, the
  plugin is `PATH` without the source, for plugins loaded under a name of their own (from the plugin directory, by
  path, an entry's own source): `@std/completion/lib` also finds std's completion loaded by path, but a plugin of
  one collection can't stand in for another's (`@x/completion/gui/kinds` isn't std's `completion` with the module
  `gui/kinds`, as it would be if `SOURCE` were ignored). An error that isn't `ErrorModuleNotFound` (which Rhai
  replaces with its own) says to add the dependency. Imported
  modules are cached by their lexically canonical path (`../`), until a plugin is loaded again. Tests:
  `tests/plugins/imports.sh`, `plugin_imports.sh`. SIGINT stops extension code (checked in
  `on_progress`), leaving the signal pending for the shell. Rhai installs no signal handlers and has no threads or
  buffered output, so nothing happens around `fork` (built without its `sync` feature). Release builds use
  `panic = "abort"`; a panic in Rhai is a Rhai bug to report, not something to `catch_unwind`.
- Rhai is built without default features (whose `runtime-rng` pulls in `libdl`) and with `only_i64`, which roughly
  halves its load-time relocations. Floats are kept, although they make the executable depend on `libm`.
- The `sh` functions reach `Shell` through a pointer set for the length of each call into Rhai. Calls are re-entrant
  (`sh::run`), so no `&mut Shell` borrow can be held across a call into Rhai or back into the shell.
- Completers run with `Shell::jobctl` taken out (so their commands aren't jobs and don't save the editor's raw modes)
  and `$?` kept; they are stopped after 2 s (checked every 1024 operations). An error is printed (after a newline,
  in interactive shells, to leave the command line) and the line redrawn by returning the word itself as the only
  candidate. `fc` sees no history while a completer runs (the editor is
  borrowed).
- `prompt-vars`: the variables the step changed are found by comparing with a snapshot of all variables
  (`Vars::changes_since`) and put back after the prompt is built; no snapshot is taken if nothing has a
  `prompt-vars` hook or file. `prompt-rewrite` hooks that take a parameter are found by looking up the function in
  the extension's AST when it is registered.
- **`precmd`, `preexec` and `exit` hooks**: `run_incremental` runs `precmd` next to `notify_jobs` (so before each
  prompt but `PS2`, also after an empty line or Ctrl-C) and `preexec` after `add_history`, both only in interactive
  shells; `Shell::exit` runs `exit` after the `EXIT` trap, unless `in_subshell`. Like the trap, `exit` hooks keep the
  shell from exec'ing its last command (`plugins::exit_hooks`, next to `has_traps`). `exit` in a hook exits the shell
  (from `exit`, with its status). Without a host, each costs one `Option` check. luish doesn't call shell functions
  with zsh's hook names, by design (PLAN, Phase 13). Tests: `tests/plugins/hooks.sh`.
- **`plugin run`** (`Host::run`) runs a file or `-c` code once, as id 0 (`CURRENT`), which no plugin has: nothing is
  added to `Host::plugins`, so no plugin is unloaded by name, and `register` refuses hooks, completers and built-ins
  (they would outlive their AST's owner). `argv` is pushed into the scope and the value comes from
  `eval_ast_with_scope`; an integer or boolean is the status (`return` and Rhai's `exit(n)` give a value too), any
  other value is 0, unlike a built-in, as a script's last statement often has a value by accident. Its imports go
  to an empty module cache that is dropped afterwards, so they are read again for each run and loaded plugins keep
  theirs, unless a plugin was loaded meanwhile (`next_id` changed), which emptied the cache for good. `-c` code has
  no source, so its imports resolve relative to the current directory (`import_dir`). The file is recorded in
  `sourced_files`, as `plugin load`'s. `--no-plugins` doesn't stop it. Tests: `tests/plugins/run.sh`,
  `run_errors.sh`.
- **`plugin load -c CODE NAME`** (`Host::load_code`) loads code without a file as a plugin like any other (`start`,
  shared with `Host::load`), with `Plugin::code` set and an empty `abs`. Its `dir` is the current directory at load
  time, so `plugin_dir()` and `import` in its hooks don't depend on later `cd`s. It replaces only a `-c` plugin of
  the same name: one with a file is refused (it could be lost by a typo), while `load_found` warns but goes on when a
  file replaces a `-c` plugin. `Host::loaded` leaves `-c` plugins out (it is about paths: `post-rc.lsh`,
  `list-available`, `restore`); `savestate` records them with their code (`Host::inline`). An `@SOURCE/PATH/MODULE`
  import never resolves to one. `NAME` can't be empty or contain a `/`. Tests: `tests/plugins/load_c.sh`.
- **Extension built-ins** (`sh::builtin`): `Host::builtins`, by name, looked up in `Shell::lookup_command` after
  functions and before `PATH` (`CommandKind::Extension`), so the only cost without them is the `Shell::plugins`
  check for external commands. `builtin`, `command`, `type` (`is a shell builtin from plugin NAME`, `Host::builtin_plugin`) and `hash` (skips them) know them;
  the editor gets their names in `Names::builtins` (highlighting, command completion; not `help`). They take
  temporary assignments as regular built-ins do. The function is called with `argv` as an array; the status comes
  from what it returns (`builtin_status`), a thrown string (an `ErrorRuntime` holding a string, also from `sh`
  functions) is printed as `NAME: message`, other errors with the file. Recursion through `sh::run` is stopped by
  the shell's stack check (each `FnPtr::call` starts Rhai's call depth afresh). `sh::read_line` reads fd 0 a byte at
  a time, as `read` does, and stops on SIGINT. The engine doesn't intern strings (`set_max_strings_interned(0)`):
  once Rhai's interner is full, each new short string scans it, which cost a built-in returning a new short string
  each call about 19% of its instructions. Tests: `tests/plugins/builtins.sh`, `interrupt.sh`; benchmark against
  shell functions: `bench/extensions/` (results in `docs/performance.md`).
- **`sh::capture`** takes the program and its arguments as an array, so that no word from the command line is parsed
  as shell code (`capture_argv`): the child is forked as for `$(...)`, then `exec_argv` runs the program, so a function
  or built-in of that name is never called, a script without `#!` runs with luish, and "not found" is reported to its
  standard error (status 127). Standard input is /dev/null (a completer must never read the terminal); standard error
  is discarded, inherited, merged (`2>&1`) or read through a second pipe, the two read together with `poll`
  (`read_pipes`) so that a program filling one doesn't block. There is no way to set variables other than running
  `env`. Shell code goes through `sh::capture_sh`, the earlier `sh::capture`; a string given to `sh::capture` is an
  error that names it. Test: `tests/plugins/capture.sh`.
- **`sh::which`** is `Shell::which` (`path.rs`): the `PATH` search that running a command does (`find_in_path`, so
  the `hash` table and an empty entry meaning `.` apply), without its fallback to a file that can't be executed, and,
  for a remembered command that is gone, the search again (as `with_command_path` tries the later directories); a name
  with a `/` is taken as it is. **`sh::commands`** lists `path::executables`, which also fills the editor's
  `PathCache`, and the highlighter uses `path::search`, so extensions, completion and highlighting agree on what
  a command is (a regular file with execute permission). Test: `tests/plugins/which.sh`.
- **`sh::matches`** is `expand::pattern::Pattern`, `case`'s matcher, with every byte of the pattern unquoted (so a
  backslash escapes, as in `case $s in $p)`). std uses it instead of matching patterns itself (ssh's `Include`, which
  also skips files that start with `.` unless the pattern does, as globbing does). Tests: `tests/plugins/matches.sh`,
  the ssh cases of `tests/plugins/std_completion.sh`.
- **`sh::expand_prompt`** is `prompt::expand`, as `print -P` uses it: `%` sequences only, whatever
  `prompt.percent` is (parameters are the extension's to expand). Test: `tests/plugins/expand_prompt.sh`.
- **`fs::realpath`** is `realpath(3)` (Rust's `canonicalize`): physical, so `..` after a symbolic link is the target's
  parent (unlike `cd`'s logical paths and `fs::find_up`), and relative to the process's directory, which `cd` keeps
  as the shell's. A path that doesn't exist gives `()`. Test: `tests/plugins/fs.sh`.
- Arrays: `sh::getvar` gives `$a` (a string), so existing extensions are unaffected; `sh::getarray` and `sh::getmap`
  read the elements and keys, and `sh::setvar` is overloaded on Rhai's `Array` and `Map`, going through
  `Shell::try_set_var_value` (the variable's attributes, the local scope and the `path` tie apply, and errors are
  thrown, not printed). A map's keys come out sorted (Rhai's maps are ordered). Test: `tests/plugins/arrays.sh`.
- **Bytes** (`bytes.rs`), as Python's `surrogateescape` (PEP 383), but Rust strings can't hold lone surrogates, so
  each byte `b` of an invalid UTF-8 sequence becomes U+10FF00 + `b` (bytes 0x80–0xFF map to U+10FF80–U+10FFFF), and
  back. Values round-trip exactly, except that real U+10FF80–U+10FFFF characters in shell data become raw bytes; that
  range is effectively unused (Nerd Fonts use the BMP private-use area and plane 15). Displayed text shows escaped
  bytes as U+FFFD; completion candidates with escaped bytes are dropped; strings with NUL can't be set as variables.
- `vcs.rs` reads `.git` without forking (`HEAD`, loose and packed refs, `commondir` for worktrees, `vcs_info`'s action
  names, the stash log), running git only for the reftable format and for `vcs::status` (`git --no-optional-locks
  status --porcelain=v2 --branch -z`). Not supported: bare repositories, `GIT_DIR`, `GIT_CEILING_DIRECTORIES`.
- Examples: `docs/examples/cobra.rhai` (programs built with Cobra). Plugins for use, in the collection
  `luish-std-plugins/` (to become a repository of its own; the source `std`): `completion/git.rhai`
  (lists commands from `LC_ALL=C git help -a` without the low-level and guide sections, options from
  `git CMD --git-completion-helper`, files from `ls-files`/`diff --cached`, collapsed to the next directory; `git diff`
  offers all tracked files once a revision or range is given, and only revisions within a range word; tested in
  `tests/plugins/git_completion.sh`) and
  `bash-completion/` (a default completer that runs bash-completion in bash through `bridge.bash`; about 50 ms per
  Tab, since bash sources `bash_completion` each time). Outside bash's own completion, compgen doesn't undo
  the quoting bash-completion gives the word (`~` as `\~`), so the bridge replaces the quoting functions; its
  `-o` options are in `copts`, since completion functions have a local `opts`.
- `luish-std-plugins/notify.rhai`: `preexec` notes the time and the line, and `precmd` sends `ESC ]777;notify;TITLE;BODY
  BEL` to fd 1 (if it is a terminal: `[ -t 1 ]`) when `.elapsed` reaches `$LUISH_NOTIFY_AFTER` (10 s), with control
  characters taken out of the line. Rhai's backtick strings don't take `\x` escapes, so the sequence is built from
  quoted strings. Not in the shell itself, since only the terminal knows whether its window has focus, and focus
  reports (`?1004`) would reach the commands' input. Test: `notify_plugin` in `tests/interactive.rs`.
- `luish-std-plugins/completion/` completes about 230 common commands from specs, one module per group (`specs.rhai`
  for coreutils, grep, tar, make, ssh ...; `shells.rhai`, `tools.rhai`, `system.rhai`, `net.rhai`, `dev.rhai`,
  `langs.rhai` for the package managers of languages, `packages.rhai` for those of systems): an option table written as
  in `--help` (`-a, --all  DESC`, `--name=ARG`, `--name[=ARG]`, `-n ARG`, `--name <ARG>`), the values of options, the
  kinds of the arguments that aren't options (`kinds.rhai`: `dirs`, `users`, `mode`, `hosts` from `~/.ssh/config` with
  `Include` and `/etc/hosts`, `targets` from the makefile, `members` from `tar -tf`, `commands` from `PATH` ...;
  `MODULE:NAME` is `MODULE::kind(NAME, cur, words)`, for the kinds of one module, such as `system:units`), and
  subcommands (`commands`, a table of names and aliases, `subs`, their specs, and `common`, the options valid on both
  sides), each of which gets the whole command line for its kinds (`line`). Other fields: `modes` (an option that picks
  an operation, as `pacman -S`, whose spec replaces the command's; the other operations stay valid but hidden, and a
  mode is also found in the word being completed, as in `-Sy`), `single_dash` (`find -name`, `gcc -std=`: no bundles,
  and `=` only where the table has it), `strict_eq` (the same for `--` options, for vim and cmake, which don't take
  `--opt=VALUE`), `guess_values` (for big tables, such as curl's: an option whose argument is named like a file or a
  directory completes to those, others to nothing), `skip` (words such as cargo's `+toolchain`, with their kinds) and
  `sub_spec` (a module function that makes a subcommand's spec when needed). `help_spec` makes a spec from a program's
  `-h` (clap or GNU style: options, the commands of a section whose heading has `command` in it, `[possible values:
  ...]` and `[aliases: ...]`), run with `COLUMNS=400` so that clap doesn't wrap; cargo's subcommands, rustup, uv and
  pixi are read this way (each takes about 10 ms), and openssl's commands from their `-help`, but pip, conda and npm
  are written out, as they take 60 to 300 ms to start. `kinds::toml` reads enough TOML for manifests (`Cargo.toml`,
  `pixi.toml`, `pyproject.toml`), and `kinds::mount_table` the file systems of `/proc/mounts`, with all four of the
  kernel's escapes (`\040`, `\011`, `\012`, `\134`, the last decoded last; test: `tests/plugins/std_mounts.sh`).
  `lib.rhai` scans the words before the cursor for options, values, `--` and the subcommand, and handles
  `--opt=VALUE`, `-o VALUE`, `-oVALUE` and bundles (`-la` offers the flags that can follow);
  a word such as `-nv` that is an option (wget) is completed as one. A word `-` offers each option once (its short name
  if it has one), `--` the long names. Package managers list the installable packages only for a word with a letter
  (apt has about 90,000), and not at all for dnf, yum and zypper. `bridges.rhai` asks programs that complete
  themselves: Cobra's (`PROG __complete ARGS... WORD`, whose last line `:N` has flags: 2 no space, 4 no filenames, 8
  extensions, 16 directories; the values of `--flag=` come without the prefix, which the bridge adds back; the
  extension's `complete-cobra PROG...` built-in registers more of them, and lists them without arguments) and nix's
  (`NIX_GET_COMPLETIONS=N`, whose first line is `normal`, `filenames` or `attrs`, the last with no space after, and
  whose descriptions are Markdown), and Click's (Python: `_PROG_COMPLETE=fish_complete COMP_WORDS=LINE COMP_CWORD=WORD`
  prints `TYPE,VALUE` and a tab and a description, TYPE being `plain`, or `file` or `dir`, which the bridge completes
  itself; fish's format because its lines are one candidate each, while zsh's three lines break on a help of several
  lines and bash's has no descriptions. As fish does, COMP_WORDS ends before the word to complete when it is empty,
  or Click takes `''` for an argument. A program that doesn't know the protocol just runs, so `complete-click PROG...`
  is only for programs built with Click, and what doesn't look like candidates is ignored). The extension only registers the commands (a map from command to module, which the
  closures share); a completer imports its module, so each is compiled on its first Tab (5 to 10 ms; later ones take
  about 1 ms, plus the programs they run: 15 ms for `systemctl stop`, 30 ms for `cargo build --`) instead of at every
  start (loading the plugin takes about 0.4 ms). xargs gets no plugin completer, as luish's own completer skips it as a
  precommand. Rhai details it works around: a closure made in a `for` loop sees the loop variable's last value (the
  completer uses `words[0]`, which is the name it was registered for); a module's constants aren't visible to its
  functions (shared tables are functions); arrays are passed to functions by value; keywords (`export`, `module`,
  `switch`, `go` ...) can't be map keys or function names without quotes; `replace` and `trim` change the string in
  place and return `()`; a closure that captures a map it is iterating is a data race.
  Other plugins use the engine through `import "@std/completion/lib"`, and name their own kinds and `sub_spec` as
  `@SOURCE/PATH/MODULE:NAME`, which `kinds.rhai` and `lib.rhai` import as they do std's `MODULE:NAME`. Kinds aren't
  closures because Rhai (1.26) links a closure to its function only through the caller's `global.lib[0]` (or by name
  while a function of its module runs): a closure made in a module fails with `Function not found: anon$...` when
  `lib.rhai` calls it. The spec format is public (`docs/extensions.md`), so changes to it must stay backward-compatible.
  Test: `tests/plugins/std_completion_extern.sh`.
  `src/options.rs` checks that `shells.rhai` lists all the options `luish -o` takes.
- `luish-std-plugins/completion/builtins.rhai`: specs for luish's built-ins, with the options only where luish
  completes the other arguments itself (`ARGS` in `complete.rs`): a completer's `()` (the kind `files`) falls back to
  luish's completion, so `cd s` still gets `CDPATH` and `unset -f` functions. `style` and `__luish_internal` have
  completers of their own: names, values and schemes come from `__luish_internal style` (which, unlike `style`,
  exists in non-interactive shells too) run with `sh::capture_sh`, so a subshell has the shell's styles and
  schemes. `print` is a Rhai keyword, so it can't name a function. Test:
  `tests/plugins/std_completion_builtins.sh`.
- **Packages** (`package.rs`, `fetch.rs`): `read_config` turns `[plugins]` into owned `Config` (sources in
  `plugins.available`, plus the built-in `std`, at the tag `vVERSION` of the running luish (`std_ref`); entries in
  `plugins.enabled`), and `manifest` a directory plugin's `plugin.toml` into entries of the same kind. `Resolver` resolves entries depth-first, dependencies before
  dependents, identifying plugins by absolute path: the same plugin twice is loaded once, two plugins with one name
  and a cycle (found on the stack) are errors, and a failed dependency fails its dependents. A plain `NAME` in a
  manifest is looked for in the collection the plugin came from (`Scope::Collection`). Git sources resolve through
  pins (URL and ref to commit): pins already used in this run, then (unless updating) `plugins.lock`, then (only for
  `sync`/`update`) `fetch::resolve`. `plugin sync` also resolves every plugin of each `plugins.available` source (each
  separately, so unrelated name clashes don't matter, `resolve_all`), so their pins are locked too; problems there
  are reported but don't stop the lock being written, while problems with enabled plugins do. Unless `-q`, the
  resolver prints `Fetching`/`Installing` as it goes (`Resolver::verbose`), and `sync` a line for each source that is
  new or moved (all of them for `update`), with the enabled plugins from it, and a summary after the lock is
  written. `plugin check` resolves the same way without fetching (`Fetching::No`), then asks for each pin's ref with
  `git ls-remote` (`fetch::remote_commit`, preferring the peeled `^{}` line of an annotated tag, since pins hold
  commits), so it touches neither the cache nor the data directory, and prints a line for each source (`-q`: only
  those with updates, and those not installed). A GitHub source that moved also gets a `compare` link
  (`compare_url`).
- **`plugin` output** (`plugins/ui.rs`). `Ui::new` resolves the `plugin.*` styles (roles in `style::ROLES`, defaults in
  `default-dark`) to SGR once per command, only when stdout is a terminal and `$NO_COLOR` is empty (so the startup path
  and pipes pay nothing, and scripts read plain text); `Ui::plain` otherwise. Errors still go through `Shell::berr`,
  uncoloured. `load`, `unload` and `load -c` confirm (`Loaded`, `Reloaded`, `Unloaded`) only when `confirms`: an
  interactive shell, no frames (not in a function or a startup file) and a terminal on stdout; `plugin list-loaded`
  and `list-available` colour names and say when there are none only on a terminal. `plugin add` always says what it
  added to `config.toml` (it was silent with `-y`). Tests: `plugins/git_packages.sh`, `add.sh` (plain text),
  `plugin_feedback` in `tests/interactive.rs` (colours, `$NO_COLOR`, silent in functions), `ui::tests`. The lock is written only if its text
  changed (so the rc cache, which fingerprints it, stays valid). Messages about manifests of git plugins show
  `SOURCE:PATH/plugin.toml` rather than the data directory.
- **Names.** A plugin of a named collection is loaded as `SOURCE/PATH` (`std/completion`, `extra/complete/all`), the
  name it is written with, so short names such as `all` don't clash. Others are named after the entry (a source
  that is one plugin, `NAME = { gh = ... }`), or after their file or directory (the plugin directory, a path, and
  the dependencies of plugins of a source without a name): `Coll::name_of`, with `Coll::src` the source's name or
  `None`. The resolver checks that names are unique (`add`). A collection's directories that aren't plugins and
  hold no `NAME.rhai` or `NAME.lsh` beside them are sub-collections (`is_collection`), to a depth of 8:
  `collection_names` lists their plugins as paths (for `plugin list-available`, `plugin sync`, `plugin add`'s
  `holds` and a source's only plugin), skipping hidden directories, and `find_path` follows a path only through
  them, so `std/completion/lib` isn't `lib.rhai` inside the plugin `completion`; `Resolver::find` reports a
  collection named as a plugin. A plain `NAME` in a manifest is a sibling (`Coll::dir`, the plugin's parent),
  `"/PATH"` is from the top of the same source (`Target::InSource`, only in manifests), and `SOURCE/PATH` from the
  top of a named one. Tests: `tests/plugins/packages.sh`, `nested.sh`.
- `plugin list-available` leaves out the loaded plugins by absolute path, and `plugin unload ARG`, if no plugin is
  loaded under the name ARG, unloads the one at the path that `plugin load ARG` would load (`package::location`,
  else `find`). Test: `tests/plugins/packages.sh`.
- A **library** is a directory plugin whose `plugin.toml` has `library = true` (`is_library`, which reads the file
  only for the key, so it also works without the `plugins` feature, for Tab). The only difference is that it is
  hidden: `plugin list-available` (unless `-a`) and Tab after `plugin load` use `visible_names`, and `main_names`
  (`plugin add`'s `holds`, the resolver's `pick` of a source's only plugin) leaves libraries out unless there is
  nothing else. `plugin.toml` itself counts as an entry point (`ENTRY_POINTS`), so a library of Rhai modules needs
  no other file; the resolver's `manifest` reports a `library` that isn't a boolean. Test:
  `tests/plugins/library.sh`.
- `plugin.toml`'s `options`, `alias`, `bindkey`, `colorscheme` and `style` tables are applied by `load_found` (with `config.rs`'s code), in
  interactive shells, after the extension loads and before `rc.lsh`; its options override `config.toml`'s, by design
  (a plugin can package a set of options). The file is parsed again there (the resolver only keeps the
  dependencies), and syntax errors are left to the resolver, so they are reported once. `load_found` records the
  file for the rc cache, also for plugins that `rc.d` loads with `plugin load`. Test: `tests/plugins/manifest.sh`.
- **Plugin options.** A manifest declares them in `[plugin-options]` (`Reader::declarations`, into `Decl`s: a type,
  and a default or `required`; not `[options]`, which is the shell's settings, shared with `config.toml`). An entry
  of `plugins.enabled` or `dependencies` gives them as `{ version = "*", options = { ... } }` (`is_entry`, which also
  takes an empty table as an entry, since an empty `SOURCE.SUB` level would name nothing; `entry_table`; beside `gh`/`git`/`path` too, where `source` skips the two keys), and `plugin load` as `OPTION=VALUE`
  after each plugin (`option_arg`; an argument is an option if what precedes its `=` is a valid option name), kept as
  text (`Given::Text`) until the declarations convert it. `Resolver::add` checks them (`apply`: undeclared, wrong
  type, missing required, and the defaults filled in) after reading the manifest, and keeps the declarations, the
  options and who gave them in `Resolved`; meeting the same plugin again (by absolute path) compares the options
  after `apply`, so giving none equals giving the defaults, and reports the difference (`differences`) at the second
  entry. `load_resolved` also compares with the options of plugins already loaded (`Host::options`), before loading
  anything. `plugin sync`'s pass over every plugin of the available sources skips the checks
  (`Resolver::check_options`): options are for loading. The options travel in `Loading` to the host's `Plugin`, for
  `sh::plugin_options()` (that of the running plugin, `CURRENT`, so also in hooks), and `with_plugin_vars` sets
  `LUISH_PLUGIN_OPTIONS` (an associative array of their text: `true`/`false` for booleans) beside
  `LUISH_PLUGIN_DIR`, for every shell file of the plugin. `savestate` writes them as `plugin restore NAME PATH
  OPTION=VALUE...` (`Host::option_args`), which converts them again with the manifest (`given_options`); the rc cache
  is keyed on `config.toml` and the manifests, so changed options or declarations invalidate it. Test:
  `tests/plugins/options.sh`.
- `fetch.rs` runs git through the shell (`command git`, in a forked child, with `GIT_TERMINAL_PROMPT=0`), with
  `-C` a bare repository per URL in `$XDG_CACHE_HOME/luish/plugins/git/REPO-HASH` (FNV-1a of the URL). A ref is
  fetched with `--depth 1` and read from `FETCH_HEAD^{commit}`; a locked commit that is missing is fetched by hash,
  else with a full fetch of its ref. `git archive` into a temporary directory next to `src/REPO-HASH/COMMIT`,
  extracted with `tar` and renamed into place, so an existing directory is complete. The data directory (not the
  cache) holds these, since startup needs them and can't recreate them, with a `README` saying that `plugin sync`
  fetches them again; the bare repositories are only for fetching, so they are in the cache (whose `README` lists
  them).
- `plugin add` (`add.rs`) turns its argument into one line of `config.toml` (`parse`, unit-tested with the
  GitHub URL forms: a `file:` URL is a path, percent-decoded, unless it is a git repository (a `.git`, or `HEAD`
  and `objects`); a path if it starts with `/`, `.` or `~`; `gh:`/GitHub URLs; other URLs; `SOURCE/PATH` of a
  named source; an existing relative path; then `OWNER/REPO`), adds it as text after the last non-blank line of its
  table (`insert`, keeping comments; a new table at the end if there is none), and reads the new text with
  `package::config_names` before writing (`prepare` and `add_to`, which the first run shares): if the entry isn't there (say `[plugins]` has `enabled = {...}`, which a new
  `[plugins.enabled]` would duplicate), it prints the line to add by hand. It asks on stderr and reads the answer a
  byte at a time from fd 0 (`interactive::read_answer`; no answer is no), writes through the symbolic link if `config.toml` is one, then runs
  `sync` and `package::load_added`. Before asking, a git source is fetched (`fetch::resolve`, into the cache's bare
  repository) and extracted into a temporary data directory (`DATA/.add.PID`, removed after), to see whether it is
  a collection of more than one plugin (`holds`), which goes to `plugins.available`, or has no plugin at all (a file
  that isn't `.rhai` or `.lsh`, or a directory none of whose entries is a plugin, as `available_names` counts only
  directories with an entry point, so a repository's `docs/` and `src/` aren't plugins), which is an error; so `sync` fetches it a second
  time, which the bare repository makes cheap. A source that can't be fetched leaves `config.toml` alone. Test:
  `tests/plugins/add.sh`.
- Not yet done (see `PLAN.md`): `plugin remove`/`gc`, version requirements other than `"*"`, `flock` for
  concurrent syncs, `login.lsh`.
- Tests: `tests/plugins/*` (packages: `packages.sh` for local sources, `nested.sh` for sub-collections,
  `git_packages.sh` for git ones with
  `file://` repositories, `add.sh`, `post_rc.sh`, `manifest.sh`; completers through `__luish_internal complete`:
  `complete.sh`, and `std_completion.sh` and `std_completion_more.sh` for `luish-std-plugins/completion`, found through
  `$STD_PLUGINS`, the latter with stand-ins for the programs it runs), `builtins/plugin.sh`, `builtins/internal_plugin.sh`, unit
  tests for the byte conversion, `git status` parsing and (with a stand-in completer) in `complete.rs`, and
  `plugin_builtin`, `plugin_completer`, `cobra_completer`, `git_completion` and `bash_completion_bridge` (skipped
  without bash-completion) in `tests/interactive.rs`.

### Signals and startup

- `main` is a C `main`, so Rust's runtime set-up doesn't run: SIGPIPE stays as inherited. Signal handlers are
  installed without `SA_RESTART` so that `wait` gets EINTR; all syscall wrappers in `sys.rs` retry on EINTR
  (`sys::read` has an `interruptible` flag).

## Deviations and their tests

Each deviation in `docs/compatibility.md` has a test: a case marked `# reference: zsh`, or one with a `.expected`
file. When adding a deviation, add it to both.

A case that must tell zsh from luish tests `$ZSH_NAME`, not `$ZSH_VERSION`: conda-forge's aarch64 zsh has
`ZSH_VERSION` empty, because the linker merged the string `5.9` into the tail of its module path, which conda
truncates when it relocates the package.

| Deviation | Tests |
|---|---|
| `source` | `builtins/source.sh` (zsh), `builtins/source_missing.sh` |
| `pushd`, `popd`, `dirs` | `builtins/dirstack.sh` (zsh `-o noposixcd`), `builtins/dirstack_interactive.sh` (zsh), `builtins/popd_dir.sh` |
| `setopt`, `unsetopt` | `options/setopt.sh` (zsh), `options/setopt_list.sh` |
| `%` sequences in prompts | `misc/prompt_percent.sh`, `misc/prompt_percent_long.sh`, `misc/prompt_style.sh` |
| The right prompt | `right_prompt` in `tests/interactive.rs` |
| `<<< word` | `exec/here_string.sh` (zsh), `parse/here_string_error.sh` |
| `<(...)`, `>(...)` | `expand/procsubst.sh` (zsh), `expand/procsubst_exec.sh` (zsh), `expand/procsubst_quoted.sh` (zsh), `expand/procsubst_word.sh` (zsh), `expand/procsubst_output.sh` (waiting for `>(...)`) |
| Brace expansion | `expand/braces.sh` (zsh `-o noignorebraces`), `expand/braces_luish.sh`, `expand/braces_off.sh` (dash) |
| `**/` | `expand/globstar.sh` (zsh), `expand/globstar_off.sh` (dash), `expand/globstar_loop.sh` |
| Glob qualifiers | `expand/glob_qualifiers.sh`, `expand/glob_qualifier_errors.sh` (zsh `+o shglob -o bareglobqual +o ksharrays`), `builtins/internal_savestate_globqual.sh` |
| A directory as a command | `builtins/autocd.sh` (zsh) |
| `bindkey` | `builtins/bindkey.sh` (same as dash), `builtins/internal_bindkey.sh`, `line_editor_keys` in `tests/interactive.rs` |
| `style`, colour schemes | `builtins/internal_style.sh`, `misc/config_toml_style.sh`, `tests/plugins/manifest_style.sh`, `syntax_highlighting` in `tests/interactive.rs` |
| History file | `history_file` and `share_history` in `tests/interactive.rs`, unit tests in `interactive/histfile.rs` |
| History expansion | `history_expansion` in `tests/interactive.rs`, unit tests in `interactive/bang.rs` |
| `alias`, `unalias` options | `builtins/alias_options.sh` (zsh), `builtins/alias_deviations.sh` |
| Global aliases | `parse/alias_global.sh` (zsh), `builtins/alias_deviations.sh` (here-document delimiter), `builtins/internal_savestate_aliases.sh` |
| Suffix aliases | `parse/alias_suffix.sh` (zsh), `builtins/alias_deviations.sh` (`command -v`) |
| `RANDOM`, `SECONDS` and the other specials | `expand/special_vars.sh` (zsh), `expand/special_vars_luish.sh`, `expand/version_vars_luish.sh`, `histcmd_shlvl` in `tests/interactive.rs` |
| `pipestatus`, `PIPESTATUS` | `expand/pipestatus.sh`, `pipefail_job_control` in `tests/interactive.rs` |
| `path` tied to `PATH` by array assignments | `expand/path_tied.sh`, `expand/path_ordinary.sh` (dash) |
| `dirstack` tied to the directory stack by array assignments | `expand/dirstack_tied.sh`, `expand/dirstack_ordinary.sh` (dash) |
| Arrays | `expand/arrays.sh` (zsh), `expand/arrays_errors.sh`, `expand/arrays_luish.sh` |
| Associative arrays | `builtins/assoc.sh` (zsh), `builtins/assoc_luish.sh` |
| `${a[i..j]}` | `expand/array_slices.sh` |
| `${!a[@]}`, `${!a[*]}` | `expand/array_keys.sh` |
| `${!x}`, `${!prefix@}` | `expand/indirect.sh` |
| Parameter flags, `${(o)a[@]}` | `expand/param_flags.sh` (zsh), `expand/param_flags_luish.sh` |
| `typeset`, `declare` | `builtins/typeset.sh` (zsh), `builtins/typeset_luish.sh`, `builtins/typeset_special.sh`, `builtins/typeset_integer.sh` (zsh), `builtins/typeset_integer_luish.sh`, `builtins/typeset_case.sh` (zsh), `builtins/typeset_unique.sh` (zsh), `builtins/typeset_case_luish.sh`, `builtins/typeset_functions.sh` (zsh), `builtins/typeset_functions_luish.sh` |
| `read -A`, `read -a` | `builtins/read_array.sh` (zsh), `builtins/read_array_luish.sh` |
| `${x:offset:length}`, `${x/pattern/replacement}` | `expand/substring.sh` (zsh), `expand/substring_error.sh` (zsh), `expand/replace.sh` (zsh), `expand/substring_bad.sh` (same as dash) |
| Modifiers, `${x:h}` | `expand/modifiers.sh` (zsh), `expand/modifiers_luish.sh`, `parse/dash_lenient.sh` |
| `SHLVL` | `misc/shlvl.sh`, `histcmd_shlvl` in `tests/interactive.rs` |
| Last command of `sh -c` | `exec/c_exec_last.sh` (zsh) |
| Script read from a pipe | `misc/stdin_script.sh` (zsh) |
| `$LINENO` | `misc/lineno.sh` |
| fd numbers in redirections | `exec/redirect_big_fd.sh` |
| `exec -- cmd` | `exec/exec_dashdash.sh` |
| `cd -e` | `builtins/cd_e.sh` |
| `[[ ... ]]` | `parse/cond.sh` (zsh), `parse/cond_regex_match.sh` (zsh), `parse/cond_regex_rematch.sh` (zsh), `parse/cond_regex_bash.sh`, `parse/cond_xtrace.sh`, `parser::tests::cond` |
| `set -o pipefail` | `options/pipefail.sh` (zsh), `options/pipefail_async.sh`, `pipefail_job_control` in `tests/interactive.rs` |
| `set -o` / `set +o` list | `options/set_o_hashall.sh`, `options/setopt_list.sh` |
| `set -x` output | `options/xtrace.sh` |
| `kill %n` without job control | `builtins/kill_job.sh` |
| `fc` | `builtins/fc_noninteractive.sh`, `fc_history` in `tests/interactive.rs` |
| `$((` fallback | `parse/arith_fallback.sh` |
| `emacs` option | `options/interactive_c.sh` |
| `BASH_SOURCE` | `misc/bash_source.sh`, `misc/bash_source_startup.sh` |
| `FUNCNAME`, `BASH_LINENO`, `caller` | `misc/call_stack.sh` |
| Error messages | `exec/error_stack.sh`, `exec/stack_guard.sh` |
| Command cache | `path_cache` in `tests/interactive.rs` |
| Command-line options | `options/command_line.sh` |
| Running out of stack | `exec/stack_guard.sh`, `exec/recursion_limit.sh` (same as dash) |
| `__luish_internal` | `builtins/internal_savestate.sh`, `builtins/internal_git_rev.sh`, `tests/plugins/default_config.sh`, `builtins/internal_complete_expand.sh`, `builtins/internal_complete_subscript.sh`, `tests/plugins/complete.sh` |
| Startup files | `misc/startup_cache.sh`, `misc/startup_cache_assigned.sh`, `misc/startup_cache_blocks.sh`, `misc/startup_cache_check.sh`, `misc/config_toml.sh`, `misc/config_toml_env.sh`, `tests/plugins/startup_cache_check.sh`, `tests/plugins/startup_cache_blocks.sh`, `tests/plugins/startup_cache_nested.sh` |
| Grouped option names | `options/setopt_values.sh`, `options/setopt_group.sh`, `options/setopt_list.sh` |
| `help` | `builtins/internal_help.sh`, `builtins/help_noninteractive.sh` (same as dash), `help_builtin` in `tests/interactive.rs` |
| `jobs -i` | `jobs_menu` in `tests/interactive.rs`, unit tests in `interactive/jobmenu.rs`, `builtins/jobs_menu.sh` (no terminal: status 2, as dash) |
| `print` | `builtins/print.sh` (zsh), `builtins/print_luish.sh`, `print_builtin` in `tests/interactive.rs` |
| `clipcopy` | `builtins/internal_clipcopy.sh`, `clipcopy` in `tests/interactive.rs` |
| `plugin` | `builtins/internal_plugin.sh`, `builtins/plugin.sh` (same as dash), `plugin_builtin` in `tests/interactive.rs` |
| Hints for commands not found | `exec/not_found_hint.sh` |
| `where`, startup caches with `vars.trace` | `misc/vartrace.sh`, `misc/vartrace_restore.sh`, `misc/vartrace_history.sh`, `misc/vartrace_startup.sh`, `tests/plugins/vartrace_prompt.sh` |

## dash as the reference

- The system dash is Debian's 0.5.12 with patches. Notably it doesn't exec the last command of `sh -c` (Debian patch
  0004), processes `\e` in `echo`/`printf`, and gives 127 for exec errors other than EACCES. Upstream source:
  `https://git.kernel.org/pub/scm/utils/dash/dash.git/plain/src/<file>?h=v0.5.12` (sometimes returns 502; retry).
  Debian's patches: `https://sources.debian.org/api/src/dash/0.5.12-12/debian/patches/`.
- CI runs on Ubuntu 24.04, whose dash (0.5.12-6) lacks some of Debian's later patches, such as the one that makes
  `test FILE -nt MISSING` true (POSIX.1-2024). Cases whose result depends on such a patch use a `.expected` file
  (`builtins/test_newer.sh`).
- dash behaviours the tests rely on: `$-` lists option letters in reverse table order; `$(...)` doesn't update `$?`
  in the middle of a command; `set -x` doesn't quote; there is no `$LINENO` and no `-h`; `.*` matches `.` and `..`;
  alias listing is in hash order, so tests list aliases by name; without job control, jobs have no command text.

## Testing notes

- A case whose reference shell times out fails (two timeouts would otherwise match, and the case would test
  nothing).
- pty tests use `TERM=dumb` (rustyline does no editing), except completion and highlighting, which use `TERM=vt100`.
  After Ctrl-C, wait for `"\x1b[K$ "` (the fresh prompt), not `"$ "`, which also matches the line redrawn under the
  completion menu. Ctrl-C at the prompt sets `$?` to 130.
- A job notification is printed before the *next* prompt, so it shows up in the output of the command that caused
  it. With `stty -echo`, neither the command nor its newline is echoed; Ctrl-D isn't echoed as a newline.
- The pty is created with size 0x0 (rustyline then assumes 80 columns, the menu 24 rows); `Pty::resize` sets it.
- Checking the line editor by hand: tmux works well (`tmux new-session -d -x 70 -y 10 ...`, `send-keys`,
  `capture-pane -p [-e]`), but run `tmux set -sg escape-time 0` first, or tmux holds Esc for 500 ms and glues it to
  the next key. Wait after Enter before typing: input sent before the next prompt is discarded.
- pixi task `outputs` caching ignores paths under `.pixi/`.
- `tests/compare.rs` runs each case in a session of its own (`setsid`), so it has no controlling terminal. Under
  `pixi run`, `cargo test` is in the background group of the user's terminal, and a case that touched it (`luish -i`,
  `set -m`) was stopped by SIGTTIN/SIGTTOU and left in state T. Now it gets ENOTTY, and a timeout kills the whole
  process group, not just the shell.
- Plugin cases (`tests/plugins/`) get `$STD_PLUGINS`, the path of `luish-std-plugins`, and test completers with
  `__luish_internal complete LINE`, which needs no terminal. Its output has a space at the end of a match that ends
  the word, before the tab of a description.
- `# reference: zsh` cases: errors give status 2 in luish (as dash) but 1 in zsh, so don't print `$?` after one,
  and don't end a case on a fatal error (the case's exit status is compared): run it in a subshell,
  `(x=1+; echo not reached) 2>/dev/null || echo failed`. `((expr))` isn't arithmetic in luish (dash parses nested
  subshells), so write `: $((expr))`. `echo` interprets backslashes in luish (as dash) but not in zsh's sh
  emulation, so print with `printf '%s\n'`. zsh sorts with the locale's collation, so compare under `LC_ALL=C`, as
  the tests run. After `for i in "${a[@]}"`, `i` holds an element, and `$((a[i]))` then fails in luish and dash
  (`Illegal number`) where zsh evaluates the value recursively.
- To write a `.expected` file, run the case in the scratchpad (a script run from the repository root can leave files
  there) with a cleared environment: `env -i LC_ALL=C HOME=$PWD .../luish case.sh 2>/dev/null`. When the failure
  report of `tests/compare.rs` shows an empty `luish:` block (only a few lines differ), diff the two outputs by hand
  the same way.
- Anything that prints code should use `unparse.rs` (source that re-parses exactly, as for `savestate` and
  `typeset -f`), not `cmdtext.rs` (dash's lossy job text).
- To compare `zsh --emulate sh`, bash and luish on a snippet, define these and run `t 'code'`, or `T 'code' 'stdin'`
  with standard input as a `printf` format (bash gets `-A` rewritten to `-a`). Never pass them `typeset` or
  `typeset -x` without names, nor bash's `typeset +f` (which lists all variables): they print the whole
  environment, tokens included.

  ```sh
  Z=$(pixi run -q which zsh)
  t() { printf '== %s\n' "$1"; z=$($Z --emulate sh -c "$1" 2>&1 | tr '\n' '|'); b=$(bash --posix -c "$1" 2>&1 | tr '\n' '|'); l=$(./target/debug/luish -c "$1" 2>&1 | tr '\n' '|'); printf ' zsh-sh: %s\n bash:   %s\n luish:  %s\n' "$z" "$b" "$l"; }
  T() { printf '== %s\n' "$1"; z=$(printf "$2" | $Z --emulate sh -c "$1" 2>&1 | tr '\n' '|'); b=$(printf "$2" | bash --posix -c "${1//-A/-a}" 2>&1 | tr '\n' '|'); l=$(printf "$2" | ./target/debug/luish -c "$1" 2>&1 | tr '\n' '|'); printf ' zsh-sh: %s\n bash:   %s\n luish:  %s\n' "$z" "$b" "$l"; }
  ```

## Fuzzing

`fuzz/` has [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for the code that takes arbitrary text
without running anything. Each target is a function in `fuzz/src/targets.rs` that, besides not panicking (debug
assertions, so overflow too, and AddressSanitizer), checks what it can of the result:

- `parse`: the lexer and parser, and `cmdtext.rs`, on the whole input (as for scripts), incrementally (as the
  interactive loop parses, where the input may be incomplete), and with aliases and `glob.bare_qualifiers`.
  `consumed()` must stay within the input.
- `unparse`: `unparse.rs`, on the input made a function's body. The printed text must parse back to the same tree,
  apart from line numbers (`strip_lines`).
- `arith`: `$((...))` (`expand/arith.rs`).
- `pattern`: `trim` and `replace` (`expand/pattern.rs`), which only try the prefixes, suffixes and positions the
  pattern could match, must give what trying them all gives.
- `highlight`: the syntax highlighter, which sees every prefix of a line as it is typed: one cell per byte, the
  `error` modifier from where `error_start` puts it, and rendering only adds SGR sequences.
- `bang`: history expansion (`interactive/bang.rs`).
- `prompt`: `%` sequences (`prompt.rs`).

cargo-fuzz needs a nightly Rust (`rustup toolchain install nightly`, `cargo install cargo-fuzz`), which pixi
doesn't have, so run it outside pixi: `fuzz/run.sh TARGET [SECONDS]` runs a target (until stopped, without
`SECONDS`), seeded with `tests/cases` for the targets that read scripts and with `fuzz/seeds/TARGET` for the others.
It keeps what it finds in `fuzz/corpus/TARGET` and crashes in `fuzz/artifacts/TARGET` (both untracked);
`cargo +nightly fuzz tmin TARGET FILE` minimises a crash. Add each bug found as a test case (a differential case,
or a unit test) when fixing it.

luish is a binary crate, so the fuzz crate compiles its modules itself: `fuzz/build.rs` writes a `#[path]` module for
each `mod NAME;` of `src/main.rs`, so the list needs no upkeep. The fuzz targets can use crate-private items, and
code only they need is under `#[cfg(any(test, fuzzing))]` (`fuzzing` is set by cargo-fuzz, and by `fuzz/build.rs`).

## Conformance

Checked on 2026-09-26 (the scripts are not in the repository):

- **autoconf**: GNU hello 2.12.1 and GNU sed 4.9 `configure` give the same output and `config.h` under luish (as
  `CONFIG_SHELL`) as under dash; both build and `make check` passes (sed: the same PASS/SKIP lists). To rerun: get
  them from ftp.gnu.org, run `CONFIG_SHELL=$L $L ./configure` next to a dash-configured copy, and diff the output,
  `config.h` and `make check` results.
- **Oils spec tests** (`spec/*.test.sh` whose `compare_shells` include dash, 1620 cases): 161 differed at first, 43
  on 2026-09-26, before arrays, `declare`, `[[`, `function`, `let` and the rest of the extensions of 0.2.0 (so some
  of the 43 no longer differ; not rerun since): the deviations, bash-only features (arrays, `shopt`, `printf -v`/`%q`, `declare`), cases that differ only by
  temporary directory names or timestamps, and the known limitations. To rerun: `git clone --depth 1
  https://github.com/oils-for-unix/oils`, split `spec/*.test.sh` on `#### ` (skipping `## STDOUT:`...`## END`
  blocks, which are expected output), keep files whose `## compare_shells:` includes dash, and run each case with
  `sh -c` under dash and luish in a fresh temporary directory (env: `PATH`, `SH`, `TMP`, `HOME`, `REPO_ROOT`,
  `LC_ALL=C.UTF-8`; stdin `/dev/null`; timeout 5 s), comparing stdout and status. `case $SH in dash)` in the cases
  doesn't match when `SH` is a full path, so some cases meant to skip dash run anyway.

## Performance

The timings (script benchmarks against dash, bash, zsh and BusyBox, startup, and the startup cache) are in the user
docs, `docs/performance.md`; update them there after rerunning `bench/run.sh` or the startup measurements. What
follows is what they came from and what is left.

Per external command, luish makes the same syscalls as dash (before `posix_spawn`, a loop running `/bin/true` 3000
times took 2.42 s, then 1.80 s as in dash). Startup makes 66 syscalls to dash's 49 (56 without the `plugins`
feature; it made 140 before `#![no_main]`, lazy signal-disposition lookup, and looking up the executable's path only
when a script without `#!` needs it). The remaining startup gap (0.56 ms to dash's 0.36 ms per `-c true` in
`docs/performance.md`) comes from the larger binary (4.8 MB) and its libraries, `libm` (for Rhai's floats),
`libpthread` and `libgcc_s`: on 2026-10-01 `LD_DEBUG=statistics` put about 100k cycles in the dynamic loader to dash's
50k (so only some 15 µs of the gap), and luish took 252 page faults to dash's 189. A command substitution takes 1.2
times as long as in dash with the same system calls; `clone` itself costs more for the larger process. The `plugins`
feature accounted for about 250 µs on 2026-09-26, accepted while it stays under 1 ms; on the virtual machine of the
2026-09-29 tables a build without it started in the same time, within the noise. A
static build (`-C target-feature=+crt-static`) started in 1.09 ms to the dynamic build's 1.5 ms, but static glibc
looks users up (`~user`) through NSS modules loaded at run time.

Binaries linked by pixi's toolchain (`pixi run release`) have an RPATH into the checkout's `.pixi` environment,
added by conda-forge's gcc specs, so the loader first looks for each library there; it costs no measurable time. The
release packages have it removed (`scripts/dist.sh`, below).

Profiled with callgrind, the in-shell gap in the script benchmarks came from `$((...))` comparing the text with each
of 35 operator strings, SipHash on every variable lookup, `${x#pat}` trying every prefix or suffix (and copying),
`case` compiling literal patterns, and needless copies. Work inside the shell is now as fast as dash or faster; the
fork-heavy scripts are within 15%, mostly startup and forks. Most of the remaining in-shell time is `malloc` and
`free`, since expansion builds `Vec`s where dash uses its stack allocator.

For small changes, instruction counts are steadier than timings: run a benchmark script under
`valgrind --tool=callgrind` with a release build before and after, and compare "Collected" for the main process. To
build the "before" without touching other worktrees, `git archive HEAD | tar x` into a temporary directory and build
there with its own `CARGO_TARGET_DIR`. Arrays cost arith +1.2% and functions +1.8% this way (the `Value` match in
`Vars::get`, the fast-path check in `expand_assigns`), parameter flags under 1% (the `flags` check on the `$x` fast
path). New variable kinds or expansions must keep `Vars::get` and the `$x` fast path in `expand_param` this cheap.
The `Arith` branch of `expand_part` deliberately doesn't call `arith_word`, which is slower out of line.

Measured this way on 2026-09-29, the features added after 0.1.0 (from `76b9576`, where the tables in
`docs/performance.md` had been made, to `138cf09`) had cost arith +4.1%, functions +7.9% and textproc +3.8%
(strings got 3.8% faster). Most of functions was `local` becoming `declare` (shared with `typeset`): each variable
went through the attribute and array-conversion code and two more `Vars::entry` lookups, and each `x="$1"`
argument was copied into a new word by `split_assignment_with`, whose `ParamExp` had grown. `declare` now skips all
that without attributes, `Vars::entry` looks the name up once when it inserts it, and `expand_plain_declaration`
expands such arguments in place; `local a b=1 c="$1"` in a loop is now faster than at `76b9576`. `"$@"` and `"$*"`
also no longer join the positional parameters into a string nobody uses (as they did at `76b9576` too). That left
arith +3.7%, functions +0.9% and textproc +3.4%, spread thinly over the new features, each 0.5% or less: `pipestatus`
in `run_pipeline` (about 11 instructions per pipeline), the stack checks in `run_list_exit` and `Arith::expr`, the
array arms and the specials' check in `Vars::get` and `Vars::set`, the `typeset` transforms in `try_set_var`, and
the parameter-flag split of `expand_param`, which is now a function call before its `expand_unflagged` (making it
`#[inline]` gained nothing measurable). In timings (`bench/run.sh -r 20` with both builds) the two are within the
noise on all four in-shell benchmarks, as are startup and the single-command loops.

The `arrays` benchmark (`bench/scripts/arrays.sh`) found `${#a[@]}` and `${a[@]:i:n}` copying the whole array
(`expand_array`), which made them quadratic in a loop over a growing array; see Arrays above. bash's slices are
linear in the offset (its arrays are linked lists), which is why the benchmark slices arrays of a fixed size.

luish parses large files about three times as slowly as dash (`-n` of nvm's 144 KB `nvm.sh`: about 6 ms to dash's
2 ms, after startup), and touches about 4 MB of memory doing it (1046 page faults to dash's 228), so the AST or the
parser's buffers are large. This makes sourcing `nvm.sh` without the startup cache take 1.4 times as long as in dash,
and slows the warm startup cache (the `-n` row of its table). It is worth profiling (and see lazy function parsing
in `PLAN.md`).

## Releases

Releases are built by `.github/workflows/release.yml`, on GitHub's x86_64 and arm64 Ubuntu runners: each runs
`pixi run dist` (`scripts/dist.sh`), which builds and checks two binaries and packages them in `target/dist/` as
`luish-ARCH-linux-LIBC.tar.gz` with a `.sha256` file each, then `scripts/test-install.sh`, which runs `install.sh`
against those packages (with `LUISH_DOWNLOAD_URL=file://...`) under dash, bash and the packaged luish: each
build, a reinstall over the running binary, a corrupt or missing download, the fallback to musl when the gnu
build doesn't run, and the configuration (none without a terminal, `--config` with a `git` that fails so nothing is
fetched, an existing one left alone, `--no-config`), in a `HOME` of its own. Where there is no configuration
(`$XDG_CONFIG_HOME/luish` or `~/.config/luish` missing or empty, as for the first run), `install.sh` asks on
`/dev/tty` (stdin is the script under `curl | sh`) if stderr is a terminal, writes the output of
`__luish_internal default-config --extra` (a release without it gets nothing, and its first run asks), and runs
`__luish_internal plugin sync`, whose failure is only a warning. It uses colours when stderr is a terminal, `TERM`
isn't `dumb` and `NO_COLOR` is empty. The workflow also runs on pull requests that change any of these, without publishing. To make a
release, set the version in `Cargo.toml` and push a tag `vVERSION`: the workflow checks that the two agree and
publishes the packages and `install.sh` as a GitHub release. The tag is also where every luish of that version
takes the `std` plugins from (`package::std_ref`), so they can't change after the release, and a build whose
version has no tag yet can't fetch them: developers use a `path` or `branch` source named `std`. `install.sh` downloads from the latest release's URLs
(`releases/latest/download/NAME`), so the packages' names must not change.

Before tagging: `pixi run check` is green; `version` in `Cargo.toml` (and `Cargo.lock`) is the new one;
`Unreleased` in `ChangeLog` is renamed to `Version X.Y.Z YYYY-MM-DD by luispedro`; the new section of
`docs/whatsnew.md` has its date; the measurements in `docs/performance.md` were made with a release build of a
revision that is in the history (their date and revision are stated there); and `pixi run docs` builds without
warnings.

- **gnu**: linked against glibc 2.17 with conda-forge's `sysroot_linux-64` (or `-aarch64`) and `gcc_linux-*` as the
  linker, from the `dist` environment in `pixi.toml`. conda-forge's gcc adds its environment's `lib` as an RPATH to
  everything it links, which `dist.sh` removes with `patchelf` (a release binary would otherwise look for its
  libraries in a directory of the CI runner first, which anyone who can create that directory could use), and it
  checks that neither build has an RPATH or RUNPATH. glibc is backward compatible: a binary runs on any glibc at
  least as new as the one it was linked against, and luish needs nothing newer than 2.17 (Rust's own minimum), so
  it runs on any distribution from 2014 on. `dist.sh` checks that no symbol needs a newer version. It is as fast as a
  build linked against the system's glibc, and passes the same tests.
- **musl**: static, for systems without glibc or without its dynamic loader in the usual place (Alpine, NixOS).
  conda-forge has no musl Rust standard library, so `dist.sh` builds it with rustup's toolchain of the same Rust
  version as pixi's. It is a fallback, and its differences are listed in `docs/compatibility.md` (Known
  limitations). The only code it needed is the type of `getrlimit`'s argument (`builtins/misc.rs`). It passes the
  test suite except `builtins/kill_trap_signals.sh` (its real-time signals start at 35). It starts in about 0.7 ms
  (2.0 ms for the gnu build, whose time goes to the dynamic loader, and 1.6 ms for dash), but runs the in-shell
  benchmarks (arith, functions, strings, textproc) 1.2 to 1.7 times as slowly as dash, as musl's `malloc` is slow.
  With mimalloc as the global allocator it was within 5% to 20% of dash (and started in 1.2 ms), at the cost of C
  code in the build and a musl C compiler to build it.

`rust-version` in `Cargo.toml` is the oldest Rust that builds luish, for those who build it with their own
toolchain (1.95, checked with `rustup run 1.95 cargo check --all-targets`; 1.94 lacks `if let` guards). Raise it
when the code needs something newer; CI doesn't check it.

A static glibc build (`-C target-feature=+crt-static`) would be the fastest, but glibc loads the NSS modules that
look users up (for `~user`) at run time, and they must come from the same glibc version it was linked against.

Nix users build from `flake.nix` instead (`docs/installation.md`). Its package uses nixpkgs's Rust, so that the
toolchain comes from the binary cache, and builds only from `Cargo.*`, `build.rs`, `src` and `docs/builtins` (without
`.git`, so `--version` shows the revision as `unknown`); it skips the tests, which need dash, zsh and a pty. The dev
shell (`nix develop`) instead has the Rust of `pixi.toml` from rust-overlay (keep `rustVersion` in step), with dash,
zsh and bash. nixpkgs's dash is upstream's, not Debian's, so about ten differential cases fail there (e.g.
`builtins/test_parse.sh`, `builtins/getopts_dash.sh`). `nix build` leaves a `result` symlink in the repository,
which the completion unit test (`interactive::complete::tests::candidates`) sees: delete it, or use `--no-link`.
`flake.lock` pins nixpkgs and rust-overlay; `nix flake update` updates them.

## Known gaps for developers

User-visible limitations are listed in `docs/compatibility.md`. Beyond those:

- Fds saved at 10 or above could collide with a user redirection to fd 10+ in the same command.
- The native built-ins are a `fn` table, not yet on a `Builtin` trait shared with extension built-ins.
- The tests never run on the musl build (CI tests glibc; the release workflow only runs `test-install.sh` on musl),
  so musl's offset in `exec/cond.rs::re_nsub` (0, from the `libc` crate's struct definition) has only been checked
  by reading it: run `exec::cond::tests::groups` on musl.

## References

- POSIX.1-2017, XCU chapter 2 "Shell Command Language", and the pages for `sh`, `set`, `trap`, `read`, `test`,
  `printf` and `getopts`.
- The source code of dash; mrsh; the Oils project's blog posts on shell parsing.
- The Rhai book (<https://rhai.rs/book>), especially embedding, safety limits, function pointers and closures.
- The glibc manual's chapter "Implementing a Job Control Shell".
