// Completion for common commands. Each command is described by a spec in
// one of the modules below (specs.rhai for coreutils, grep, diffutils, tar,
// make, rsync, man, ssh ...), and completed by lib.rhai. Programs that
// complete themselves (those built with Cobra or Click, and nix) are asked,
// in bridges.rhai; `complete-cobra PROG...` and `complete-click PROG...` add
// more of them. git is completed by git.rhai.
//
// The modules are imported by the completer, so that they are compiled on
// the first Tab for one of their commands rather than whenever a shell
// starts (luish keeps them once compiled). luish calls a completer with the
// command's name as the first word. (A closure here that used `cmd` would
// see its last value.)

let modules = #{
    specs: [
        // coreutils
        "cat", "ls", "cp", "mv", "rm", "mkdir", "ln", "touch", "chmod", "chown", "chgrp", "head", "tail", "wc",
        "sort", "uniq", "cut", "du", "df", "stat", "tee", "readlink", "realpath", "basename", "dirname", "date",
        "dd", "install", "mktemp", "nl", "od", "paste", "comm", "join", "tr", "seq", "shuf", "split", "tac",
        "truncate", "md5sum", "sha1sum", "sha224sum", "sha256sum", "sha384sum", "sha512sum", "b2sum", "base64",
        "base32", "shred", "rmdir", "uname", "id", "fold",
        // others
        "grep", "egrep", "fgrep", "diff", "cmp", "diff3", "sdiff", "tar", "make", "rsync", "man", "ssh", "scp",
        "sftp", "pkill", "pgrep", "killall",
    ],
    shells: ["luish", "sh", "dash", "bash", "zsh"],
    tools: [
        "find", "fd", "fdfind", "locate", "plocate", "mlocate", "sed", "awk", "nawk", "mawk", "gawk", "jq",
        "rg", "less", "more", "file", "ldd", "patch", "tree", "gzip", "gunzip", "zcat", "xz", "lzma", "unxz",
        "xzcat", "unlzma", "lzcat", "zstd", "unzstd", "zstdcat", "bzip2", "bunzip2", "bzcat", "zip", "unzip",
    ],
    system: [
        "systemctl", "journalctl", "loginctl", "ps", "top", "htop", "lsof", "strace", "mount", "umount", "lsblk",
        "dmesg", "free", "tmux",
    ],
    net: [
        "curl", "wget", "ip", "ss", "ping", "ping6", "dig", "ssh-add", "ssh-keygen", "ssh-copy-id", "gpg", "gpg2",
        "openssl",
    ],
    dev: [
        "cargo", "rustup", "go", "gcc", "g++", "cc", "c++", "clang", "clang++", "cmake", "meson", "ninja", "gdb",
        "python", "python3", "sqlite3", "vim", "vi", "view", "vimdiff", "gvim", "rvim", "nvim", "nano", "emacs",
        "emacsclient",
    ],
    langs: ["pip", "pip3", "uv", "conda", "mamba", "micromamba", "pixi", "npm", "npx", "yarn", "pnpm"],
    packages: [
        "apt", "apt-get", "apt-cache", "apt-mark", "dpkg", "dnf", "dnf5", "yum", "rpm", "pacman", "zypper", "apk",
        "brew", "snap", "flatpak",
    ],
};

let module_of = #{};
for m in modules.keys() {
    for cmd in modules[m] {
        module_of[cmd] = m;
    }
}

// Programs built with Cobra, which complete themselves.
let cobra = [
    "gh", "glab", "docker", "podman", "podman-remote", "nerdctl", "buildah", "skopeo", "kubectl", "kubeadm", "oc",
    "helm", "minikube", "kind", "k3d", "k9s", "kustomize", "flux", "argocd", "istioctl", "cilium", "velero", "tkn",
    "kn", "crane", "cosign", "syft", "grype", "trivy", "etcdctl", "doctl", "flyctl", "hugo", "rclone", "chezmoi",
    "golangci-lint", "goreleaser", "dagger", "operator-sdk",
];
fn cobra_completer(cmd) {
    sh::completer(cmd, |words, i| {
        import "bridges" as bridges;
        bridges::cobra(words, i)
    });
}
for cmd in cobra {
    cobra_completer(cmd);
}
// complete-cobra PROG...: complete PROG... as programs built with Cobra;
// without arguments, list those programs.
sh::builtin("complete-cobra", |argv| {
    if argv.len() < 2 {
        for cmd in cobra {
            print(cmd);
        }
        return;
    }
    for cmd in argv.extract(1) {
        if cmd == "" || cmd.contains("/") {
            throw `${cmd}: bad command name`;
        }
        cobra_completer(cmd);
        if !cobra.contains(cmd) {
            cobra.push(cmd);
        }
    }
});
// Programs built with Click (Python), which complete themselves. Not the
// ones that take seconds to start (dbt, mlflow ...): `complete-click` adds them.
let click = [
    "black", "flask", "uvicorn", "pip-compile", "pip-sync", "hatch", "mkdocs", "cookiecutter", "celery",
    "datasette", "sqlite-utils", "llm", "pgcli", "mycli", "litecli", "streamlit", "wandb",
];
fn click_completer(cmd) {
    sh::completer(cmd, |words, i| {
        import "bridges" as bridges;
        bridges::click(words, i)
    });
}
for cmd in click {
    click_completer(cmd);
}
// complete-click PROG...: complete PROG... as programs built with Click;
// without arguments, list those programs. (Each Tab runs the program, so
// only for programs that are built with Click.)
sh::builtin("complete-click", |argv| {
    if argv.len() < 2 {
        for cmd in click {
            print(cmd);
        }
        return;
    }
    for cmd in argv.extract(1) {
        if cmd == "" || cmd.contains("/") {
            throw `${cmd}: bad command name`;
        }
        click_completer(cmd);
        if !click.contains(cmd) {
            click.push(cmd);
        }
    }
});
sh::completer("nix", |words, i| {
    import "bridges" as bridges;
    bridges::nix(words, i)
});
sh::completer("git", |words, i| {
    import "git" as git;
    git::complete(words, i)
});

for cmd in module_of.keys() {
    sh::completer(cmd, |words, i| {
        import "lib" as lib;
        let m = module_of[words[0]];
        import m as specs;
        lib::complete(specs::spec(words[0]), words, i)
    });
}
