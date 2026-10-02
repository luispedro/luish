// Specs for shells: luish itself, sh (dash), bash and zsh (see lib.rhai for
// their fields).

// luish's options, by the names `-o` and `setopt` take (the tests check
// that it has all of those of `src/options.rs`).
fn luish_options() {
    [
        "allexport", "emacs", "errexit", "hashall", "ignoreeof", "monitor", "noclobber", "noexec", "noglob", "nolog",
        "notify", "nounset", "pipefail", "privileged", "verbose", "vi", "xtrace", "prompt.percent",
        "prompt.transient_rprompt", "glob.star", "glob.bare_qualifiers", "expand.braces", "cd.auto",
        "history.ignore_space", "history.reduce_blanks", "history.save_no_dups", "history.inc_append", "history.share",
        "history.expand", "history.verify", "editor.autosuggest", "editor.no_highlight",
        "highlight.paths",
        "pushd.auto", "pushd.ignore_dups", "pushd.silent", "terminal.no_integration",
    ]
}

// The options of dash, as `set -o` names them.
fn dash_options() {
    [
        "errexit", "noglob", "ignoreeof", "interactive", "monitor", "noexec", "stdin", "xtrace", "verbose", "vi",
        "emacs", "noclobber", "allexport", "notify", "nounset", "privileged", "nolog", "debug",
    ]
}

fn bash_options() {
    [
        "allexport", "braceexpand", "emacs", "errexit", "errtrace", "functrace", "hashall", "histexpand", "history",
        "ignoreeof", "interactive-comments", "keyword", "monitor", "noclobber", "noexec", "noglob", "nolog",
        "notify", "nounset", "onecmd", "physical", "pipefail", "posix", "privileged", "verbose", "vi", "xtrace",
    ]
}

fn shopt_options() {
    [
        "autocd", "assoc_expand_once", "cdable_vars", "cdspell", "checkhash", "checkjobs", "checkwinsize",
        "cmdhist", "compat31", "compat32", "compat40", "compat41", "compat42", "compat43", "compat44",
        "complete_fullquote", "direxpand", "dirspell", "dotglob", "execfail", "expand_aliases", "extdebug",
        "extglob", "extquote", "failglob", "force_fignore", "globasciiranges", "globskipdots", "globstar",
        "gnu_errfmt", "histappend", "histreedit", "histverify", "hostcomplete", "huponexit", "inherit_errexit",
        "interactive_comments", "lastpipe", "lithist", "localvar_inherit", "localvar_unset", "login_shell",
        "mailwarn", "no_empty_cmd_completion", "nocaseglob", "nocasematch", "noexpand_translation", "nullglob",
        "patsub_replacement", "progcomp", "progcomp_alias", "promptvars", "restricted_shell", "shift_verbose",
        "sourcepath", "varredir_close", "xpg_echo",
    ]
}

// zsh's options, from zsh.
fn zsh_options() {
    let r = sh::capture(["zsh", "-fc", "print -l ${(ko)options}"]);
    if r.status == 0 { r.out.split("\n") } else { [] }
}

fn kind(name, cur, words) {
    switch name {
        "luish" => luish_options(),
        "dash" => dash_options(),
        "bash" => bash_options(),
        "shopt" => shopt_options(),
        "zsh" => zsh_options(),
        "emulate" => ["zsh", "sh", "ksh", "csh", "bash"],
        _ => throw `unknown kind: ${name}`,
    }
}

// The options that dash and luish share: `set`'s letters.
fn set_letters() {
    `
        -a  export all variables that are assigned (allexport)
        -b  report the status of background jobs at once (notify)
        -C  don't overwrite existing files with > (noclobber)
        -e  exit when a command fails (errexit)
        -f  disable pathname expansion (noglob)
        -h  remember where commands are found (hashall)
        -I  ignore end-of-file on the terminal (ignoreeof)
        -m  enable job control (monitor)
        -n  read commands but don't run them (noexec)
        -p  privileged mode (privileged)
        -u  make expanding an unset variable an error (nounset)
        -v  print the input as it is read (verbose)
        -x  print commands as they are run (xtrace)
        -V  use vi-style line editing (vi)
        -E  use emacs-style line editing (emacs)
    `
}

// After `-c`, a command, then `$0` and the arguments; otherwise a script,
// then its arguments.
fn script_args() {
    #{"-c": ["none", "none", "files"]}
}

fn luish() {
    #{
        opts: `
            -c  run COMMAND, with ARG0 as $0 and ARGs as $1, $2...
            -s, --stdin  read commands from standard input (ARGs are $1, $2...)
            -i, --interactive  run as an interactive shell
            -l, --login  run as a login shell
            -o NAME  set an option by name, as for setopt
            --no-rcs  don't read any startup files
            --no-plugins  make plugin load do nothing
            --help  show the help and exit
            --version  show the version and exit
        ` + set_letters(),
        values: #{"-o": "shells:luish"},
        args_if: script_args(),
    }
}

fn dash() {
    #{
        opts: `
            -c  run COMMAND, with ARG0 as $0 and ARGs as $1, $2...
            -s  read commands from standard input
            -i  run as an interactive shell
            -l  run as a login shell
            -o NAME  set an option by name
        ` + set_letters(),
        values: #{"-o": "shells:dash"},
        args_if: script_args(),
    }
}

fn bash() {
    #{
        opts: `
            --debugger  enable the debugger profile
            --dump-po-strings  print the translatable strings, in PO format
            --dump-strings  print the translatable strings
            --help  show the help and exit
            --init-file FILE  read FILE instead of ~/.bashrc
            --rcfile FILE  read FILE instead of ~/.bashrc
            --login  run as a login shell
            --noediting  don't use readline
            --noprofile  don't read /etc/profile or ~/.bash_profile
            --norc  don't read ~/.bashrc
            --posix  follow POSIX where bash differs
            --pretty-print  print the script formatted
            --restricted  run as a restricted shell
            --verbose  print the input as it is read
            --version  show the version and exit
            -c  run COMMAND, with ARG0 as $0 and ARGs as $1, $2...
            -i  run as an interactive shell
            -l  run as a login shell
            -r  run as a restricted shell
            -s  read commands from standard input
            -D  print the translatable strings
            -O SHOPT  set a shopt option
            -o NAME  set an option by name
            -a  export all variables that are assigned (allexport)
            -B  perform brace expansion (braceexpand)
            -b  report the status of background jobs at once (notify)
            -C  don't overwrite existing files with > (noclobber)
            -E  inherit the ERR trap in functions (errtrace)
            -e  exit when a command fails (errexit)
            -f  disable pathname expansion (noglob)
            -H  enable ! history expansion (histexpand)
            -h  remember where commands are found (hashall)
            -k  take assignments anywhere in a command (keyword)
            -m  enable job control (monitor)
            -n  read commands but don't run them (noexec)
            -P  don't resolve symbolic links in cd (physical)
            -p  privileged mode (privileged)
            -T  inherit the DEBUG and RETURN traps in functions (functrace)
            -t  exit after one command (onecmd)
            -u  make expanding an unset variable an error (nounset)
            -v  print the input as it is read (verbose)
            -x  print commands as they are run (xtrace)
        `,
        values: #{"-o": "shells:bash", "-O": "shells:shopt"},
        args_if: script_args(),
    }
}

fn zsh() {
    #{
        opts: `
            --help  show the help and exit
            --version  show the version and exit
            --emulate SHELL  emulate SHELL (sh, ksh, csh, bash)
            -b  end option processing, like --
            -c  run COMMAND, with ARG0 as $0 and ARGs as $1, $2...
            -d  don't read the global startup files (no_global_rcs)
            -f  don't read any startup files (no_rcs)
            -i  run as an interactive shell
            -l  run as a login shell
            -o OPTION  set an option by name
            -s  read commands from standard input
            -v  print the input as it is read (verbose)
            -x  print commands as they are run (xtrace)
        `,
        values: #{"-o": "shells:zsh", "--emulate": "shells:emulate"},
        args_if: script_args(),
    }
}

fn spec(cmd) {
    switch cmd {
        "luish" => luish(),
        "sh" | "dash" => dash(),
        "bash" => bash(),
        "zsh" => zsh(),
        _ => throw `no spec for ${cmd}`,
    }
}
