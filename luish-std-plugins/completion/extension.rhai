// Completion for common commands. Each command is described by a spec in
// one of the modules below (specs.rhai for coreutils, grep, diffutils, tar,
// make, rsync, man, ssh ...), and completed by lib.rhai. Programs that
// complete themselves (those built with Cobra, and nix) are asked, in
// bridges.rhai. git is completed by git.rhai.
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
for cmd in cobra {
    sh::completer(cmd, |words, i| {
        import "bridges" as bridges;
        bridges::cobra(words, i)
    });
}
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
