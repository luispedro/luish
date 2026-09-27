// Completion for common commands: coreutils (ls, cp, chmod ...), grep,
// diffutils, tar, make, rsync, man, ssh, scp and pkill. Each command is
// described in specs.rhai, and completed by lib.rhai. git's completion is
// the plugin git-completion, which plugin.toml loads.
//
// The modules are imported by the completer, so that they are compiled on
// the first Tab rather than whenever a shell starts (luish keeps them once
// compiled). luish calls a completer with the command's name as the first
// word. (A closure here that used `cmd` would see its last value.)

let commands = [
    // coreutils
    "cat", "ls", "cp", "mv", "rm", "mkdir", "ln", "touch", "chmod", "chown", "chgrp", "head", "tail", "wc",
    "sort", "uniq", "cut", "du", "df", "stat", "tee", "readlink", "realpath", "basename", "dirname", "date", "dd",
    "install", "mktemp", "nl", "od", "paste", "comm", "join", "tr", "seq", "shuf", "split", "tac", "truncate",
    "md5sum", "sha1sum", "sha224sum", "sha256sum", "sha384sum", "sha512sum", "b2sum", "base64", "base32", "shred",
    "rmdir", "uname", "id", "fold",
    // others
    "grep", "egrep", "fgrep", "diff", "cmp", "diff3", "sdiff", "tar", "make", "rsync", "man", "ssh", "scp", "sftp",
    "pkill", "pgrep", "killall",
];

for cmd in commands {
    sh::completer(cmd, |words, i| {
        import "lib" as lib;
        import "specs" as specs;
        lib::complete(specs::spec(words[0]), words, i)
    });
}
