// Specs for system tools: systemd's (systemctl, journalctl, loginctl),
// processes (ps, top, htop, lsof, strace), file systems and devices (mount,
// umount, lsblk, dmesg, free) and tmux (see lib.rhai for their fields).

import "kinds" as kinds;

// Whether the command line manages the user's services (`--user`).
fn user_mode(words) {
    words.extract(1, words.len() - 2).some(|w| w == "--user")
}

// The directories of unit files.
fn unit_dirs(user) {
    if user {
        let h = kinds::home();
        [
            h + "/.config/systemd/user", "/etc/systemd/user", "/run/systemd/user",
            h + "/.local/share/systemd/user", "/usr/local/lib/systemd/user", "/usr/lib/systemd/user",
        ]
    } else {
        [
            "/etc/systemd/system", "/run/systemd/system", "/usr/local/lib/systemd/system",
            "/usr/lib/systemd/system", "/lib/systemd/system",
        ]
    }
}

// Units: those loaded (with their descriptions), and unless `loaded`,
// those with unit files. Devices only for a word that starts like one.
fn units(cur, words, loaded, suffix) {
    let user = user_mode(words);
    let devices = cur.starts_with("dev-") || cur.starts_with("sys-");
    let out = [];
    let seen = #{};
    let r = sh::capture(
        "systemctl" + (if user { " --user" } else { "" }) +
            " list-units --all --plain --no-legend --full --no-pager 2>/dev/null",
    );
    for line in r.out.split("\n") {
        let w = line.split();
        if w.len() < 4 {
            continue;
        }
        let u = w[0];
        if !u.starts_with(cur) || (u.ends_with(".device") && !devices) || (suffix != "" && !u.ends_with(suffix)) {
            continue;
        }
        seen[u] = true;
        out.push(#{value: u, desc: w.extract(4).reduce(|s, x| if s == "" { x } else { s + " " + x }, "")});
    }
    if loaded {
        return out;
    }
    for d in unit_dirs(user) {
        for n in fs::list_dir(d) ?? [] {
            if !n.starts_with(cur) || seen.contains(n) || !n.contains(".") || n.ends_with(".wants")
                || n.ends_with(".requires") || n.ends_with(".d") || (suffix != "" && !n.ends_with(suffix))
            {
                continue;
            }
            seen[n] = true;
            // A template: its instances are `NAME@INSTANCE.TYPE`.
            out.push(if n.contains("@.") { #{value: n.sub_string(0, n.index_of("@") + 1), suffix: ""} } else { n });
        }
    }
    out
}

// The variables of the manager's environment.
fn manager_env(words) {
    let user = if user_mode(words) { " --user" } else { "" };
    let r = sh::capture("systemctl" + user + " show-environment 2>/dev/null");
    r.out.split("\n").filter(|l| l.contains("=")).map(|l| l.sub_string(0, l.index_of("=")))
}

// The IDs of the login sessions, with their users.
fn sessions() {
    let r = sh::capture("loginctl list-sessions --no-legend --no-pager 2>/dev/null");
    let out = [];
    for line in r.out.split("\n") {
        let w = line.split();
        if w.len() >= 3 {
            out.push(#{value: w[0], desc: w[2]});
        }
    }
    out
}

fn seats() {
    let r = sh::capture("loginctl list-seats --no-legend --no-pager 2>/dev/null");
    r.out.split("\n").map(|l| l.split()).filter(|w| !w.is_empty()).map(|w| w[0])
}

// The mount points, from /proc/mounts.
fn mounts() {
    let out = [];
    for line in (fs::read_file("/proc/mounts") ?? "").split("\n") {
        let w = line.split();
        if w.len() >= 2 {
            // Spaces and tabs are octal escapes.
            let p = w[1];
            p.replace("\\040", " ");
            p.replace("\\011", "\t");
            out.push(p);
        }
    }
    out
}

// The mount options of `mount -o`, after the last comma.
fn mount_options(cur) {
    let opts = [
        "async", "atime", "noatime", "auto", "noauto", "bind", "rbind", "defaults", "dev", "nodev", "diratime",
        "nodiratime", "dirsync", "exec", "noexec", "group", "iversion", "noiversion", "mand", "nomand", "_netdev",
        "nofail", "relatime", "norelatime", "strictatime", "nostrictatime", "lazytime", "nolazytime", "suid",
        "nosuid", "silent", "loud", "owner", "remount", "ro", "rw", "sync", "user", "nouser", "users", "uid=",
        "gid=", "umask=", "fmask=", "dmask=", "loop", "X-mount.mkdir",
    ];
    let comma = -1;
    let k = 0;
    for c in cur.chars() {
        if c == ',' {
            comma = k;
        }
        k += 1;
    }
    let out = opts.map(|o| if o.ends_with("=") { #{value: o, suffix: ""} } else { o });
    #{prefix: cur.sub_string(0, comma + 1), candidates: out}
}

// The sessions of the tmux server, for `-t` (and windows after `SESSION:`).
fn tmux_targets(cur) {
    let colon = cur.index_of(":");
    if colon >= 0 {
        let s = cur.sub_string(0, colon);
        let r = sh::capture(`tmux list-windows -t ${sh::quote(s)} -F '#I	#W' 2>/dev/null`);
        let out = [];
        for line in r.out.split("\n") {
            let tab = line.index_of("\t");
            if tab > 0 {
                out.push(#{value: line.sub_string(0, tab), desc: line.sub_string(tab + 1)});
            }
        }
        return #{prefix: cur.sub_string(0, colon + 1), candidates: out};
    }
    let r = sh::capture("tmux list-sessions -F '#S	#{session_windows} windows' 2>/dev/null");
    let out = [];
    for line in r.out.split("\n") {
        let tab = line.index_of("\t");
        if tab > 0 {
            out.push(#{value: line.sub_string(0, tab), desc: line.sub_string(tab + 1)});
        }
    }
    out
}

fn ps_fields() {
    [
        #{value: "pid", desc: "process ID"},
        #{value: "ppid", desc: "parent process ID"},
        #{value: "pgid", desc: "process group ID"},
        #{value: "sid", desc: "session ID"},
        #{value: "user", desc: "effective user name"},
        #{value: "uid", desc: "effective user ID"},
        #{value: "group", desc: "effective group name"},
        #{value: "comm", desc: "command name"},
        #{value: "args", desc: "command with all its arguments"},
        #{value: "cmd", desc: "command with all its arguments"},
        #{value: "%cpu", desc: "CPU utilization"},
        #{value: "%mem", desc: "memory utilization"},
        #{value: "rss", desc: "resident set size, in KiB"},
        #{value: "vsz", desc: "virtual memory size, in KiB"},
        #{value: "tty", desc: "controlling terminal"},
        #{value: "stat", desc: "process state"},
        #{value: "ni", desc: "nice value"},
        #{value: "pri", desc: "priority"},
        #{value: "psr", desc: "processor it last ran on"},
        #{value: "nlwp", desc: "number of threads"},
        #{value: "lstart", desc: "time the command started"},
        #{value: "start", desc: "time the command started"},
        #{value: "etime", desc: "elapsed time since it started"},
        #{value: "time", desc: "cumulative CPU time"},
        #{value: "wchan", desc: "kernel function where it sleeps"},
        #{value: "cgroup", desc: "control groups"},
        #{value: "label", desc: "security label"},
    ]
}

// ps's fields, after the last comma (and a `-` for --sort).
fn fields(cur, list) {
    let comma = -1;
    let k = 0;
    for c in cur.chars() {
        if c == ',' {
            comma = k;
        }
        k += 1;
    }
    let p = cur.sub_string(0, comma + 1);
    let rest = cur.sub_string(comma + 1);
    if rest.starts_with("-") || rest.starts_with("+") {
        p += rest.sub_string(0, 1);
    }
    #{prefix: p, candidates: list}
}

fn kind(name, cur, words) {
    switch name {
        "units" => units(cur, words, false, ""),
        "loaded" => units(cur, words, true, ""),
        "targets" => units(cur, words, false, ".target"),
        "manager_env" => manager_env(words),
        "sessions" => sessions(),
        "seats" => seats(),
        "mounts" => mounts(),
        "mount_options" => mount_options(cur),
        "tmux_targets" => tmux_targets(cur),
        "ps_fields" => fields(cur, ps_fields()),
        "top_fields" => [
            "PID", "USER", "PR", "NI", "VIRT", "RES", "SHR", "S", "%CPU", "%MEM", "TIME+", "COMMAND", "PPID", "UID",
            "GROUP", "TTY", "P", "TIME", "SWAP", "CODE", "DATA", "nMaj", "nMin", "WCHAN", "Flags",
        ],
        "strace_expr" => [
            "trace=", "abbrev=", "verbose=", "raw=", "signal=", "read=", "write=", "fault=", "inject=", "status=",
            "quiet=", "kvm=", "decode-fds=",
        ].map(|v| #{value: v, suffix: ""}),
        _ => throw `unknown kind: ${name}`,
    }
}

fn output_modes() {
    [
        "short", "short-precise", "short-iso", "short-iso-precise", "short-full", "short-monotonic", "short-unix",
        "short-delta", "verbose", "export", "json", "json-pretty", "json-sse", "json-seq", "cat", "with-unit",
    ]
}

fn systemctl() {
    let units = #{args: ["system:units"]};
    let loaded = #{args: ["system:loaded"]};
    #{
        opts: `
            -h, --help  show the help
            --version  show the package version
            --system  connect to the system manager
            --user  connect to the user's service manager
            -C, --capsule=NAME  connect to the service manager of a capsule
            -H, --host=[USER@]HOST  operate on a remote host
            -M, --machine=CONTAINER  operate on a local container
        `,
        common: `
            -t, --type=TYPE  list units of a particular type
            --state=STATE  list units with a particular LOAD, SUB or ACTIVE state
            --failed  shortcut for --state=failed
            -p, --property=NAME  show only the properties with this name
            -P NAME  like --value --property=NAME
            -a, --all  show all properties, or all units in memory
            -l, --full  don't ellipsize unit names on output
            -r, --recursive  show the units of the host and local containers
            --reverse  show reverse dependencies with list-dependencies
            --before  show the units ordered before with list-dependencies
            --after  show the units ordered after with list-dependencies
            --with-dependencies  also show the dependencies of units
            --job-mode=MODE  how to deal with already queued jobs
            -T, --show-transaction  show the full transaction when enqueuing a job
            --show-types  show the types of sockets
            --value  when showing properties, only print the value
            --check-inhibitors=MODE  whether to check inhibitors before shutting down
            -i  shortcut for --check-inhibitors=no
            --kill-whom=WHOM  whom to send the signal to
            --kill-value=INT  the signal value to enqueue
            -s, --signal=SIGNAL  the signal to send
            --what=RESOURCES  the types of resources to remove
            --now  start or stop the unit after enabling or disabling it
            --dry-run  only print what would be done
            -q, --quiet  suppress output
            --no-warn  suppress several warnings shown by default
            --wait  wait until the service stopped again, or startup completed
            --no-block  do not wait until the operation finished
            --no-wall  don't send a wall message before halt, power-off or reboot
            --message=MESSAGE  a reason for a system shutdown
            --no-reload  don't reload the daemon after enabling or disabling unit files
            --legend=BOOL  enable or disable the legend
            --no-pager  do not pipe the output into a pager
            --no-ask-password  do not ask for system passwords
            --global  edit, enable, disable or mask default user unit files globally
            --runtime  edit, enable, disable or mask unit files until the next reboot
            -f, --force  override existing symlinks, or shut down at once
            --preset-mode=MODE  apply only enable, only disable, or all presets
            --root=PATH  operate on unit files in the root directory PATH
            --image=PATH  operate on unit files in the disk image PATH
            -n, --lines=INTEGER  the number of journal entries to show
            -o, --output=STRING  the journal output mode
            --firmware-setup  tell the firmware to show the setup menu on next boot
            --boot-loader-menu=TIME  boot into the boot loader menu on next boot
            --boot-loader-entry=NAME  boot into a boot loader entry on next boot
            --plain  print unit dependencies as a list instead of a tree
            --timestamp=FORMAT  the format of printed timestamps
            --marked  restart or reload previously marked units
        `,
        values: #{
            "-C": "none", "-H": "hosts", "-M": "none",
            "-t": [
                "service", "mount", "swap", "socket", "target", "device", "automount", "timer", "path", "slice",
                "scope",
            ],
            "--state": [
                "active", "inactive", "failed", "activating", "deactivating", "running", "exited", "dead",
                "loaded", "not-found", "masked", "enabled", "disabled", "static",
            ],
            "-p": "none", "-P": "none",
            "--job-mode": [
                "fail", "replace", "replace-irreversibly", "isolate", "ignore-dependencies", "ignore-requirements",
                "flush", "triggering", "restart-dependencies",
            ],
            "--check-inhibitors": ["auto", "yes", "no"],
            "--kill-whom": ["main", "control", "all"], "--kill-value": "none", "-s": "signals",
            "--what": ["configuration", "state", "cache", "logs", "runtime", "fdstore", "all"],
            "--message": "none", "--legend": ["yes", "no"], "--preset-mode": ["full", "enable-only", "disable-only"],
            "--root": "dirs", "-n": "none", "-o": output_modes(), "--boot-loader-menu": "none",
            "--boot-loader-entry": "none", "--timestamp": ["pretty", "unix", "us", "utc", "us+utc"],
        },
        commands: `
            list-units  list the units in memory
            list-automounts  list the automount units in memory
            list-paths  list the path units in memory
            list-sockets  list the socket units in memory
            list-timers  list the timer units in memory
            is-active  check whether units are active
            is-failed  check whether units are failed
            status  show the runtime status of units
            show  show the properties of units, jobs or the manager
            cat  show the files and drop-ins of units
            help  show the manual of units
            list-dependencies  show the dependencies of units
            start  start (activate) units
            stop  stop (deactivate) units
            reload  reload units
            restart  start or restart units
            try-restart  restart units if active
            reload-or-restart  reload units if possible, otherwise start or restart them
            try-reload-or-restart  if active, reload units if possible, otherwise restart them
            isolate  start one unit and stop all others
            kill  send a signal to the processes of a unit
            clean  clean the runtime, cache, state, logs or configuration of a unit
            freeze  freeze the execution of the processes of units
            thaw  resume the execution of frozen units
            set-property  set properties of a unit
            bind  bind-mount a path from the host into a unit's namespace
            mount-image  mount an image from the host into a unit's namespace
            service-log-level  get or set the logging threshold of a service
            service-log-target  get or set the logging target of a service
            reset-failed  reset the failed state of units
            whoami  show the unit of the caller or of processes
            list-unit-files  list the installed unit files
            enable  enable unit files
            disable  disable unit files
            reenable  reenable unit files
            preset  enable or disable unit files by the preset configuration
            preset-all  enable or disable all unit files by the preset configuration
            is-enabled  check whether unit files are enabled
            mask  mask units
            unmask  unmask units
            link  link unit files into the search path
            revert  revert unit files to the vendor version
            add-wants  add a Wants dependency of a target on units
            add-requires  add a Requires dependency of a target on units
            edit  edit unit files
            get-default  get the name of the default target
            set-default  set the default target
            list-machines  list the local containers and the host
            list-jobs  list jobs
            cancel  cancel jobs
            show-environment  dump the environment
            set-environment  set environment variables
            unset-environment  unset environment variables
            import-environment  import environment variables
            daemon-reload  reload the systemd manager configuration
            daemon-reexec  reexecute the systemd manager
            log-level  get or set the logging threshold of the manager
            log-target  get or set the logging target of the manager
            service-watchdogs  get or set the service watchdog state
            is-system-running  check whether the system is fully running
            default  enter the system default mode
            rescue  enter the system rescue mode
            emergency  enter the system emergency mode
            halt  shut down and halt the system
            poweroff  shut down and power off the system
            reboot  shut down and reboot the system
            kexec  shut down and reboot the system with kexec
            soft-reboot  shut down and reboot userspace
            exit  ask the user instance or container to exit
            switch-root  change to a different root file system
            sleep  put the system to sleep
            suspend  suspend the system
            hibernate  hibernate the system
            hybrid-sleep  hibernate and suspend the system
            suspend-then-hibernate  suspend the system, then hibernate it
        `,
        subs: #{
            "list-units": #{args: ["none"]},
            "list-automounts": #{args: ["none"]},
            "list-paths": #{args: ["none"]},
            "list-sockets": #{args: ["none"]},
            "list-timers": #{args: ["none"]},
            "is-active": loaded,
            "is-failed": loaded,
            status: units,
            show: units,
            cat: units,
            help: units,
            "list-dependencies": units,
            start: units,
            stop: loaded,
            reload: loaded,
            restart: units,
            "try-restart": loaded,
            "reload-or-restart": units,
            "try-reload-or-restart": loaded,
            isolate: #{args: ["system:targets"]},
            kill: loaded,
            clean: units,
            freeze: loaded,
            thaw: loaded,
            "set-property": #{args: ["system:units", "none"]},
            bind: #{args: ["system:loaded", "files"]},
            "mount-image": #{args: ["system:loaded", "files"]},
            "service-log-level": #{
                args: ["system:loaded", ["emerg", "alert", "crit", "err", "warning", "notice", "info", "debug"]],
            },
            "service-log-target": #{args: ["system:loaded", ["console", "kmsg", "journal", "syslog"]]},
            "reset-failed": loaded,
            whoami: #{args: ["pids"]},
            "list-unit-files": #{args: ["none"]},
            enable: units,
            disable: units,
            reenable: units,
            preset: units,
            "preset-all": #{args: ["none"]},
            "is-enabled": units,
            mask: units,
            unmask: units,
            revert: units,
            "add-wants": #{args: ["system:targets", "system:units"]},
            "add-requires": #{args: ["system:targets", "system:units"]},
            edit: units,
            "get-default": #{args: ["none"]},
            "set-default": #{args: ["system:targets", "none"]},
            "list-machines": #{args: ["none"]},
            "list-jobs": #{args: ["none"]},
            cancel: #{args: ["none"]},
            "show-environment": #{args: ["none"]},
            "set-environment": #{args: ["none"]},
            "unset-environment": #{args: ["system:manager_env"]},
            "import-environment": #{args: ["none"]},
            "daemon-reload": #{args: ["none"]},
            "daemon-reexec": #{args: ["none"]},
            "log-level": #{args: [["emerg", "alert", "crit", "err", "warning", "notice", "info", "debug"], "none"]},
            "log-target": #{args: [["console", "kmsg", "journal", "journal-or-kmsg", "syslog", "null"], "none"]},
            "service-watchdogs": #{args: [["yes", "no"], "none"]},
        },
    }
}

fn journalctl() {
    #{
        opts: `
            --system  show the system journal
            --user  show the user journal of the current user
            -M, --machine=CONTAINER  operate on a local container
            -m, --merge  show entries from all available journals
            -D, --directory=PATH  show the journal files of a directory
            -i, --file=PATH  show a journal file
            --root=PATH  operate on an alternate file system root
            --image=PATH  operate on a disk image as the file system root
            --namespace=NAMESPACE  show the journal data of a namespace
            -S, --since=DATE  show entries not older than DATE
            -U, --until=DATE  show entries not newer than DATE
            -c, --cursor=CURSOR  show entries starting at a cursor
            --after-cursor=CURSOR  show entries after a cursor
            --cursor-file=FILE  show entries after the cursor in FILE and update FILE
            -b, --boot[=ID]  show the current boot or the specified boot
            -u, --unit=UNIT  show the logs of a unit
            --user-unit=UNIT  show the logs of a user unit
            --invocation=ID  show the logs of an invocation ID
            -I  show the logs of the latest invocation of the unit
            -t, --identifier=STRING  show entries with the syslog identifier STRING
            -T, --exclude-identifier=STRING  hide entries with the syslog identifier STRING
            -p, --priority=RANGE  show entries within a priority range
            --facility=FACILITY  show entries with the facilities FACILITY
            -g, --grep=PATTERN  show entries whose MESSAGE matches PATTERN
            --case-sensitive[=BOOL]  force case sensitive or insensitive matching
            -k, --dmesg  show the kernel message log of the current boot
            -o, --output=STRING  the output mode
            --output-fields=LIST  the fields to print in the verbose, export and json modes
            -n, --lines[=INTEGER]  the number of journal entries to show
            -r, --reverse  show the newest entries first
            --show-cursor  print the cursor after all the entries
            --utc  show times in UTC
            -x, --catalog  add message explanations where available
            --no-hostname  suppress the hostname field
            --no-full  ellipsize fields
            -a, --all  show all fields, including long and unprintable
            -f, --follow  follow the journal
            --no-tail  show all lines, even in follow mode
            --truncate-newline  truncate entries at the first newline
            -q, --quiet  do not show info messages and privilege warnings
            --no-pager  do not pipe the output into a pager
            -e, --pager-end  jump to the end in the pager at once
            --interval=TIME  the time interval for changing the FSS sealing key
            --verify-key=KEY  the FSS verification key
            --force  override the FSS key pair with --setup-keys
            -h, --help  show the help
            --version  show the package version
            -N, --fields  list all the field names in use
            -F, --field=FIELD  list all the values that a field takes
            --list-boots  show terse information about recorded boots
            --list-invocations  show the invocation IDs of a unit
            --list-namespaces  show the list of journal namespaces
            --disk-usage  show the total disk usage of all journal files
            --vacuum-size=BYTES  reduce disk usage below BYTES
            --vacuum-files=INT  leave only INT journal files
            --vacuum-time=TIME  remove journal files older than TIME
            --verify  verify the consistency of journal files
            --sync  synchronize unwritten journal messages to disk
            --relinquish-var  stop logging to disk, log to a temporary file system
            --smart-relinquish-var  like --relinquish-var, unless the log directory is on the root mount
            --flush  flush all journal data from /run into /var
            --rotate  rotate the journal files at once
            --header  show journal header information
            --list-catalog  show all message IDs in the catalog
            --dump-catalog  show the entries in the message catalog
            --update-catalog  update the message catalog database
            --setup-keys  generate a new FSS key pair
        `,
        values: #{
            "-M": "none", "-D": "dirs", "--namespace": "none", "-S": ["today", "yesterday", "now"],
            "-U": ["today", "yesterday", "now"], "-c": "none", "--after-cursor": "none",
            "-b": "none", "-u": "system:units", "--user-unit": "system:units", "--invocation": "none",
            "-t": "none", "-T": "none",
            "-p": ["emerg", "alert", "crit", "err", "warning", "notice", "info", "debug"],
            "--facility": [
                "kern", "user", "mail", "daemon", "auth", "syslog", "lpr", "news", "uucp", "cron", "authpriv", "ftp",
                "local0", "local1", "local2", "local3", "local4", "local5", "local6", "local7", "help",
            ],
            "-g": "none", "--case-sensitive": ["yes", "no"], "-o": output_modes(), "--output-fields": "none",
            "-n": "none", "--interval": "none", "--verify-key": "none", "-F": "none", "--vacuum-size": "none",
            "--vacuum-files": "none", "--vacuum-time": "none",
        },
        args: ["none"],
    }
}

fn loginctl() {
    let sessions = #{args: ["system:sessions"]};
    let users = #{args: ["users"]};
    let seats = #{args: ["system:seats"]};
    #{
        opts: `
            -h, --help  show the help
            --version  show the package version
            -H, --host=[USER@]HOST  operate on a remote host
            -M, --machine=CONTAINER  operate on a local container
        `,
        common: `
            --no-pager  do not pipe the output into a pager
            --no-legend  do not show the headers and footers
            --no-ask-password  don't prompt for a password
            -p, --property=NAME  show only the properties with this name
            -P NAME  like --value --property=NAME
            -a, --all  show all properties, including empty ones
            --value  when showing properties, only print the value
            -l, --full  do not ellipsize the output
            --kill-whom=WHOM  whom to send the signal to
            -s, --signal=SIGNAL  the signal to send
            -n, --lines=INTEGER  the number of journal entries to show
            --json=MODE  JSON output for list-sessions, list-users and list-seats
            -j  like --json=pretty on a terminal, --json=short otherwise
            -o, --output=MODE  the journal output mode
        `,
        values: #{
            "-H": "hosts", "-M": "none", "-p": "none", "-P": "none", "--kill-whom": ["leader", "all"],
            "-s": "signals", "-n": "none", "--json": ["pretty", "short", "off"], "-o": output_modes(),
        },
        commands: `
            list-sessions  list sessions
            session-status  show the status of sessions
            show-session  show the properties of sessions or the manager
            activate  activate a session
            lock-session  screen lock sessions
            unlock-session  screen unlock sessions
            lock-sessions  screen lock all current sessions
            unlock-sessions  screen unlock all current sessions
            terminate-session  terminate sessions
            kill-session  send a signal to the processes of a session
            list-users  list users
            user-status  show the status of users
            show-user  show the properties of users or the manager
            enable-linger  enable the linger state of users
            disable-linger  disable the linger state of users
            terminate-user  terminate all the sessions of users
            kill-user  send a signal to the processes of a user
            list-seats  list seats
            seat-status  show the status of seats
            show-seat  show the properties of seats or the manager
            attach  attach devices to a seat
            flush-devices  flush all device associations
            terminate-seat  terminate all the sessions on seats
        `,
        subs: #{
            "list-sessions": #{args: ["none"]},
            "session-status": sessions,
            "show-session": sessions,
            activate: sessions,
            "lock-session": sessions,
            "unlock-session": sessions,
            "lock-sessions": #{args: ["none"]},
            "unlock-sessions": #{args: ["none"]},
            "terminate-session": sessions,
            "kill-session": sessions,
            "list-users": #{args: ["none"]},
            "user-status": users,
            "show-user": users,
            "enable-linger": users,
            "disable-linger": users,
            "terminate-user": users,
            "kill-user": users,
            "list-seats": #{args: ["none"]},
            "seat-status": seats,
            "show-seat": seats,
            attach: #{args: ["system:seats", "files"]},
            "flush-devices": #{args: ["none"]},
            "terminate-seat": seats,
        },
    }
}

fn ps() {
    #{
        opts: `
            -A, -e  select all processes
            -a  select all processes with a terminal, except session leaders
            -d  select all processes except session leaders
            -N, --deselect  negate the selection
            -C CMDLIST  select by command name
            -G, --Group=GRPLIST  select by real group ID or name
            -g, --group=GRPLIST  select by session or effective group name
            -p, --pid=PIDLIST  select by process ID
            --ppid=PIDLIST  select by parent process ID
            -q, --quick-pid=PIDLIST  select by process ID (quick mode)
            -s, --sid=SIDLIST  select by session ID
            -t, --tty=TTYLIST  select by terminal
            -U, --User=USERLIST  select by real user ID or name
            -u, --user=USERLIST  select by effective user ID or name
            -F  extra full format
            -f  full format
            -j  jobs format
            -l  long format
            -o, --format=FORMAT  user-defined format
            -O FORMAT  like -o, with the default fields first
            -y  don't show flags; show rss instead of addr (with -l)
            -M, -Z, --context  add a column of security data
            -c  show scheduler information
            -H, --forest  show the process hierarchy
            -w  wide output
            --cols, --columns, --width=NUM  set the screen width
            --lines, --rows=NUM  set the screen height
            --headers  repeat the header lines, one per page
            --no-headers  don't print the header line
            --sort=SPEC  the sort order, such as -%cpu or +pid
            -L  show threads, with LWP and NLWP columns
            -T  show threads, with a SPID column
            -m  show threads after processes
            -V, --version  display version information
            --help  display help
        `,
        values: #{
            "-C": "processes", "-G": "groups", "-g": "groups", "-p": "pids", "--ppid": "pids", "-q": "pids",
            "-s": "none", "-t": "none", "-U": "users", "-u": "users", "-o": "system:ps_fields",
            "-O": "system:ps_fields", "--sort": "system:ps_fields", "--cols": "none", "--lines": "none",
        },
        args: [[
            #{value: "aux", desc: "all processes, in the user-oriented format"},
            #{value: "axjf", desc: "all processes, as a tree, in the jobs format"},
            #{value: "xafw", desc: "your processes and those with a terminal, as a tree"},
        ]],
    }
}

fn top() {
    #{
        opts: `
            -b, --batch-mode  run in non-interactive batch mode
            -c, --cmdline-toggle  reverse the last remembered c state
            -d, --delay=SECS  the delay between updates
            -E, --scale-summary-mem=SCALE  scale the memory summary (k, m, g, t, p, e)
            -e, --scale-task-mem=SCALE  scale the task memory (k, m, g, t, p)
            -H, --threads-show  show tasks plus all their threads
            -i, --idle-toggle  reverse the last remembered i state
            -n, --iterations=NUMBER  exit after NUMBER iterations
            -O, --list-fields  output all field names, then exit
            -o, --sort-override=FIELD  sort on the field FIELD
            -p, --pid=PIDLIST  monitor only the tasks in PIDLIST
            -S, --accum-time-toggle  reverse the last remembered S state
            -s, --secure-mode  run with secure mode restrictions
            -U, --filter-any-user=USER  show only processes owned by USER
            -u, --filter-only-euser=USER  show only processes whose effective user is USER
            -w, --width[=COLUMNS]  change the print width
            -1, --single-cpu-toggle  reverse the last remembered 1 state
            -h, --help  display this help text, then exit
            -V, --version  output version information and exit
        `,
        values: #{
            "-d": "none", "-E": ["k", "m", "g", "t", "p", "e"], "-e": ["k", "m", "g", "t", "p"], "-n": "none",
            "-o": "system:top_fields", "-p": "pids", "-U": "users", "-u": "users", "-w": "none",
        },
        args: ["none"],
    }
}

fn htop() {
    #{
        opts: `
            -C, --no-color  use a monochrome color scheme
            -d, --delay=DELAY  the delay between updates, in tenths of seconds
            -F, --filter=FILTER  show only the commands matching the filter
            -h, --help  print this help screen
            -H, --highlight-changes[=DELAY]  highlight new and old processes
            -M, --no-mouse  disable the mouse
            --no-meters  hide the meters
            -n, --max-iterations=NUMBER  exit after NUMBER iterations
            -p, --pid=PID  show only the given PIDs
            --readonly  disable all system and process changing features
            -s, --sort-key=COLUMN  sort by COLUMN in list view
            -t, --tree  show the tree view
            -u, --user[=USERNAME]  show only the processes of a user
            -U, --no-unicode  do not use unicode but plain ASCII
            -V, --version  print the version
        `,
        values: #{
            "-d": "none", "-F": "none", "-n": "none", "-p": "pids", "-u": "users",
            "-s": [
                "PID", "USER", "PRIORITY", "NICE", "M_VIRT", "M_RESIDENT", "M_SHARE", "STATE", "PERCENT_CPU",
                "PERCENT_MEM", "TIME", "Command",
            ],
        },
        args: ["none"],
    }
}

fn lsof() {
    #{
        opts: `
            -?, -h  list help
            -a  AND the selections (the default is OR)
            -b  avoid kernel blocks
            -c STRING  select the processes whose command starts with STRING
            -d FDSET  select by file descriptor set
            -E  display endpoint information
            -g PGIDS  select by process group ID
            -i ADDR  select Internet files ([46][proto][@host][:port])
            -K  list tasks (threads)
            -l  list user IDs instead of names
            -n  no host names
            -N  select NFS files
            -o  list file offsets
            -O  no overhead (risky)
            -P  no port names
            -p PIDS  select by process ID (^ excludes)
            -Q  allow failed searches
            -R  list the parent process ID
            -s  list file sizes
            -S SECONDS  the stat timeout
            -t  terse listing: only process IDs
            -T  disable TCP/TPI information
            -U  select Unix sockets
            -u USERS  select by login name or user ID (^ excludes)
            -v  list version information
            -V  verbose search
            -X  skip TCP and UDP files
            -x FL  cross over file systems or symbolic links with +d and +D
            -F FIELDS  select the fields to output
        `,
        values: #{"-c": "processes", "-d": "none", "-g": "none", "-i": "none", "-p": "pids", "-S": "none",
            "-u": "users", "-x": ["f", "l", "fl"], "-F": "none"},
    }
}

fn strace() {
    #{
        opts: `
            -e EXPR  a qualifying expression: OPTION=[!]all or OPTION=[!]VAL1[,VAL2]...
            -E, --env=VAR=VAL  put VAR=VAL in the environment of the command
            -p, --attach=PID  trace the process with ID PID
            -u, --user=USERNAME  run the command as USERNAME
            --argv0=NAME  set the argv[0] of the command to NAME
            -b, --detach-on=execve  detach on the execve syscall
            -D, --daemonize[=grandchild]  run the tracer as a grandchild, not as a parent
            -f, --follow-forks  follow forks
            -I, --interruptible=INTERRUPTIBLE  when strace can be interrupted by signals
            --kill-on-exit  kill all tracees if strace is killed
            --trace=SET  trace only the given syscalls
            --signal=SET  trace only the given set of signals
            --status=SET  print only the system calls with these return statuses
            -P, --trace-path=PATH  trace accesses to PATH
            -z, --successful-only  print only syscalls that returned without an error
            -Z, --failed-only  print only syscalls that returned with an error
            -a, --columns=COLUMN  the column to align syscall results at
            -i, --instruction-pointer  print the instruction pointer at the time of the syscall
            -k, --stack-trace  print the stack trace of each syscall
            -o, --output=FILE  send the trace output to FILE
            -A, --output-append-mode  append to the output file instead of overwriting it
            --output-separately  output into separate files, with -o FILE and -f
            -q, --quiet  suppress messages about attaching and detaching
            -r, --relative-timestamps  print relative timestamps
            -s, --string-limit=STRSIZE  the maximum length of strings printed
            -t, --absolute-timestamps  print absolute timestamps
            -T, --syscall-times  print the time spent in each syscall
            -v, --no-abbrev  print unabbreviated structures
            -x  print non-ASCII strings in hexadecimal
            -X, --const-print-style=FORMAT  how to print named constants and flags
            -y, --decode-fds  print the paths of file descriptors
            -c, --summary-only  count time, calls and errors of each syscall, and report a summary
            -C, --summary  like -c, and print the regular output too
            -O, --summary-syscall-overhead=OVERHEAD  the overhead of tracing syscalls
            -S, --summary-sort-by=SORTBY  sort the summary by SORTBY
            -U, --summary-columns=COLUMNS  the columns of the summary
            -w, --summary-wall-clock  summarise the syscall latency
            --seccomp-bpf  use seccomp-bpf filtering to reduce overhead
            -d, --debug  enable debug output to stderr
            -h, --help  print the help and exit
            -V, --version  print the version and exit
        `,
        values: #{
            "-e": "system:strace_expr", "-E": "none", "-p": "pids", "-u": "users", "--argv0": "none",
            "-b": ["execve"], "-D": ["grandchild", "pgroup", "session"],
            "-I": ["1", "anywhere", "2", "waiting", "3", "never", "4", "never_tstp"], "--trace": "none",
            "--signal": "signals", "--status": ["successful", "failed", "unfinished", "unavailable", "detached"],
            "-a": "none", "-s": "none", "-X": ["raw", "abbrev", "verbose"], "-O": "none",
            "-S": ["time", "min-time", "max-time", "avg-time", "calls", "errors", "name", "nothing"], "-U": "none",
        },
        args: ["commands", "files"],
    }
}

fn mount() {
    #{
        opts: `
            -a, --all  mount all filesystems mentioned in fstab
            -c, --no-canonicalize  don't canonicalize paths
            -f, --fake  dry run; skip the mount(2) syscall
            -F, --fork  fork off for each device (use with -a)
            -T, --fstab <path>  an alternative file to /etc/fstab
            -i, --internal-only  don't call the mount.<type> helpers
            -l, --show-labels  also show filesystem labels
            -m, --mkdir[=<mode>]  like -o X-mount.mkdir[=<mode>]
            -n, --no-mtab  don't write to /etc/mtab
            --options-mode <mode>  what to do with options loaded from fstab
            --options-source <source>  the source of the mount options
            --options-source-force  force use of options from fstab/mtab
            --onlyonce  check if filesystem is already mounted
            -o, --options <list>  comma-separated list of mount options
            -O, --test-opts <list>  limit the set of filesystems (use with -a)
            -r, --read-only  mount the filesystem read-only (same as -o ro)
            -t, --types <list>  limit the set of filesystem types
            --source <src>  the source (path, label, uuid)
            --target <target>  the mountpoint
            --target-prefix <path>  the path used for all mountpoints
            -v, --verbose  say what is being done
            -w, --rw, --read-write  mount the filesystem read-write (the default)
            -N, --namespace <ns>  perform mount in another namespace
            -h, --help  display this help
            -V, --version  display version
            -L, --label <label>  synonym for LABEL=<label>
            -U, --uuid <uuid>  synonym for UUID=<uuid>
            -B, --bind  mount a subtree somewhere else (same as -o bind)
            -M, --move  move a subtree to some other place
            -R, --rbind  mount a subtree and all submounts somewhere else
            --make-shared  mark a subtree as shared
            --make-slave  mark a subtree as slave
            --make-private  mark a subtree as private
            --make-unbindable  mark a subtree as unbindable
            --make-rshared  recursively mark a whole subtree as shared
            --make-rslave  recursively mark a whole subtree as slave
            --make-rprivate  recursively mark a whole subtree as private
            --make-runbindable  recursively mark a whole subtree as unbindable
        `,
        values: #{
            "-m": "none", "--options-mode": ["ignore", "append", "prepend", "replace"],
            "--options-source": ["fstab", "mtab", "disable"], "-o": "system:mount_options", "-O": "none",
            "-t": "tools:fstypes", "--target": "dirs", "--target-prefix": "dirs", "-N": "none", "-L": "none",
            "-U": "none",
        },
    }
}

fn umount() {
    #{
        opts: `
            -a, --all  unmount all filesystems
            -A, --all-targets  unmount all mountpoints of the device in the current namespace
            -c, --no-canonicalize  don't canonicalize paths
            -d, --detach-loop  if mounted loop device, also free this loop device
            --fake  dry run; skip the umount(2) syscall
            -f, --force  force unmount (in case of an unreachable NFS system)
            -i, --internal-only  don't call the umount.<type> helpers
            -n, --no-mtab  don't write to /etc/mtab
            -l, --lazy  detach the filesystem now, clean up things later
            -O, --test-opts <list>  limit the set of filesystems (use with -a)
            -R, --recursive  recursively unmount a target with all its children
            -r, --read-only  in case unmounting fails, try to remount read-only
            -t, --types <list>  limit the set of filesystem types
            -v, --verbose  say what is being done
            -q, --quiet  suppress not mounted error messages
            -N, --namespace <ns>  perform umount in another namespace
            -h, --help  display this help
            -V, --version  display version
        `,
        values: #{"-O": "none", "-t": "tools:fstypes", "-N": "none"},
        args: ["system:mounts"],
    }
}

fn lsblk() {
    #{
        opts: `
            -A, --noempty  don't print empty devices
            -D, --discard  print discard capabilities
            -E, --dedup <column>  de-duplicate output by <column>
            -I, --include <list>  show only devices with specified major numbers
            -J, --json  use JSON output format
            -M, --merge  group parents of sub-trees (usable for RAIDs, Multi-path)
            -O, --output-all  output all columns
            -P, --pairs  use key="value" output format
            -S, --scsi  output info about SCSI devices
            -N, --nvme  output info about NVMe devices
            -v, --virtio  output info about virtio devices
            -a, --all  print all devices
            -b, --bytes  print SIZE in bytes rather than in human readable format
            -d, --nodeps  don't print slaves or holders
            -e, --exclude <list>  exclude devices by major number
            -f, --fs  output info about filesystems
            -i, --ascii  use ascii characters only
            -l, --list  use list format output
            -m, --perms  output info about permissions
            -n, --noheadings  don't print headings
            -o, --output <list>  output columns
            -p, --paths  print complete device path
            -r, --raw  use raw output format
            -s, --inverse  inverse dependencies
            -t, --topology  output info about topology
            -w, --width <num>  the output width, in characters
            -x, --sort <column>  sort output by <column>
            -y, --shell  use column names to be usable as shell variable identifiers
            -z, --zoned  print zone related information
            --sysroot <dir>  use the directory as the system root
            -h, --help  display this help
            -V, --version  display version
        `,
        values: #{
            "-E": "none", "-I": "none", "-e": "none", "-w": "none", "--sysroot": "dirs",
            "-o": [
                "NAME", "KNAME", "PATH", "MAJ:MIN", "FSTYPE", "FSVER", "LABEL", "UUID", "PARTLABEL", "PARTUUID",
                "MOUNTPOINT", "MOUNTPOINTS", "FSSIZE", "FSUSED", "FSAVAIL", "FSUSE%", "SIZE", "TYPE", "RO", "RM",
                "MODEL", "SERIAL", "STATE", "OWNER", "GROUP", "MODE", "ROTA", "TRAN", "VENDOR",
            ],
            "-x": [
                "NAME", "KNAME", "PATH", "MAJ:MIN", "FSTYPE", "LABEL", "UUID", "MOUNTPOINT", "SIZE", "TYPE", "MODEL",
            ],
        },
    }
}

fn dmesg() {
    #{
        opts: `
            -C, --clear  clear the kernel ring buffer
            -c, --read-clear  read and clear all messages
            -D, --console-off  disable printing messages to console
            -E, --console-on  enable printing messages to console
            -F, --file <file>  use the file instead of the kernel log buffer
            -f, --facility <list>  restrict output to defined facilities
            -H, --human  human readable output
            -J, --json  use JSON output format
            -k, --kernel  display kernel messages
            -L, --color[=<when>]  colorize messages (auto, always or never)
            -l, --level <list>  restrict output to defined levels
            -n, --console-level <level>  set level of messages printed to console
            -P, --nopager  do not pipe output into a pager
            -p, --force-prefix  force timestamp output on each line of multi-line messages
            -r, --raw  print the raw message buffer
            --noescape  don't escape unprintable character
            -S, --syslog  force to use syslog(2) rather than /dev/kmsg
            -s, --buffer-size <size>  buffer size to query the kernel ring buffer
            -u, --userspace  display userspace messages
            -w, --follow  wait for new messages
            -W, --follow-new  wait and print only new messages
            -x, --decode  decode facility and level to readable string
            -d, --show-delta  show time delta between printed messages
            -e, --reltime  show local time and time delta in readable format
            -T, --ctime  show human-readable timestamp (may be inaccurate!)
            -t, --notime  don't show any timestamp with messages
            --time-format <format>  show timestamps in the given format
            --since <time>  display the lines since the specified time
            --until <time>  display the lines until the specified time
            -h, --help  display this help
            -V, --version  display version
        `,
        values: #{
            "-f": [
                "kern", "user", "mail", "daemon", "auth", "syslog", "lpr", "news", "uucp", "cron", "authpriv", "ftp",
                "local0", "local1", "local2", "local3", "local4", "local5", "local6", "local7",
            ],
            "-L": ["auto", "always", "never"],
            "-l": ["emerg", "alert", "crit", "err", "warn", "notice", "info", "debug"],
            "-n": ["emerg", "alert", "crit", "err", "warn", "notice", "info", "debug"],
            "-s": "none", "--time-format": ["delta", "reltime", "ctime", "notime", "iso", "raw"],
            "--since": "none", "--until": "none",
        },
        args: ["none"],
    }
}

fn free() {
    #{
        opts: `
            -b, --bytes  show output in bytes
            --kilo  show output in kilobytes
            --mega  show output in megabytes
            --giga  show output in gigabytes
            --tera  show output in terabytes
            --peta  show output in petabytes
            -k, --kibi  show output in kibibytes
            -m, --mebi  show output in mebibytes
            -g, --gibi  show output in gibibytes
            --tebi  show output in tebibytes
            --pebi  show output in pebibytes
            -h, --human  show human-readable output
            --si  use powers of 1000 not 1024
            -l, --lohi  show detailed low and high memory statistics
            -L, --line  show output on a single line
            -t, --total  show total for RAM + swap
            -v, --committed  show committed memory and commit limit
            -s N, --seconds N  repeat printing every N seconds
            -c N, --count N  repeat printing N times, then exit
            -w, --wide  wide output
            --help  display this help and exit
            -V, --version  output version information and exit
        `,
        values: #{"-s": "none", "-c": "none"},
        args: ["none"],
    }
}

fn tmux() {
    let target = #{opts: "-t TARGET  the target session, window or pane", values: #{"-t": "system:tmux_targets"}};
    #{
        opts: `
            -2  force 256 colours
            -C  start in control mode (-CC disables echo)
            -c SHELL-COMMAND  run SHELL-COMMAND with the default shell
            -D  don't start the server as a daemon
            -f FILE  use FILE as the configuration file
            -h  print the usage and exit
            -L SOCKET-NAME  use a different socket name
            -l  behave as a login shell
            -N  do not start the server
            -S SOCKET-PATH  use a different socket path
            -T FEATURES  set terminal features for the client
            -u  write UTF-8 output to the terminal
            -V  report the tmux version
            -v  request verbose logging
        `,
        values: #{"-c": "none", "-L": "none", "-T": "none"},
        commands: `
            attach-session, attach, a, at  attach to a session
            detach-client, detach  detach the current client
            has-session, has  report whether a session exists
            kill-server  kill the server and all its sessions
            kill-session  kill a session
            kill-window, killw  kill a window
            kill-pane, killp  kill a pane
            list-sessions, ls  list the sessions
            list-windows, lsw  list the windows of a session
            list-panes, lsp  list the panes of a window
            list-clients, lsc  list the clients
            list-keys, lsk  list the key bindings
            list-commands, lscm  list the commands
            list-buffers, lsb  list the paste buffers
            new-session, new  create a new session
            new-window, neww  create a new window
            rename-session, rename  rename a session
            rename-window, renamew  rename a window
            select-window, selectw  select a window
            select-pane, selectp  select a pane
            split-window, splitw  split a pane in two
            send-keys, send  send keys to a pane
            switch-client, switchc  switch the client to another session
            source-file, source  run the commands in a file
            set-option, set  set an option
            show-options, show  show options
            set-environment, setenv  set an environment variable
            show-environment, showenv  show the environment
            bind-key, bind  bind a key to a command
            unbind-key, unbind  unbind a key
            capture-pane, capturep  capture the contents of a pane
            display-message, display  display a message
            display-popup, popup  display a popup
            run-shell, run  run a shell command
            resize-pane, resizep  resize a pane
            swap-window, swapw  swap two windows
            move-window, movew  move a window
            start-server, start  start the server
            lock-server, lock  lock all clients
            refresh-client, refresh  refresh a client
        `,
        subs: #{
            "attach-session": #{
                opts: `
                    -t TARGET-SESSION  the session to attach to
                    -c WORKING-DIRECTORY  the working directory of the session
                    -d  detach other clients
                    -E  don't apply update-environment
                    -f FLAGS  client flags
                    -r  attach read-only
                    -x  send SIGHUP to the parents of the other clients
                `,
                values: #{"-t": "system:tmux_targets", "-c": "dirs", "-f": "none"},
                args: ["none"],
            },
            "has-session": target,
            "kill-session": #{
                opts: `
                    -t TARGET-SESSION  the session to kill
                    -a  kill all sessions but the target
                    -C  clear the alerts of all windows of the session
                `,
                values: #{"-t": "system:tmux_targets"},
                args: ["none"],
            },
            "kill-window": target,
            "kill-pane": target,
            "list-windows": target,
            "list-panes": target,
            "new-session": #{
                opts: `
                    -s SESSION-NAME  the name of the session
                    -n WINDOW-NAME  the name of the first window
                    -c START-DIRECTORY  the working directory
                    -t GROUP-NAME  add to the group of this session
                    -A  attach to the session if it exists
                    -d  don't attach to the new session
                    -D  with -A, detach other clients
                    -E  don't apply update-environment
                    -P  print information about the new session
                    -X  with -A and -D, send SIGHUP to the other clients
                    -e ENVIRONMENT  set an environment variable
                    -f FLAGS  client flags
                    -F FORMAT  the format for -P
                    -x WIDTH  the width of the session
                    -y HEIGHT  the height of the session
                `,
                values: #{
                    "-s": "none", "-n": "none", "-c": "dirs", "-t": "system:tmux_targets", "-e": "none", "-f": "none",
                    "-F": "none", "-x": "none", "-y": "none",
                },
                args: ["commands", "files"],
            },
            "new-window": #{
                opts: `
                    -t TARGET-WINDOW  where to create the window
                    -n WINDOW-NAME  the name of the window
                    -c START-DIRECTORY  the working directory
                    -a  after the target window
                    -b  before the target window
                    -d  don't make it the current window
                    -k  destroy the target window if it exists
                    -S  select the window if it exists
                    -P  print information about the new window
                    -e ENVIRONMENT  set an environment variable
                    -F FORMAT  the format for -P
                `,
                values: #{"-t": "system:tmux_targets", "-n": "none", "-c": "dirs", "-e": "none", "-F": "none"},
                args: ["commands", "files"],
            },
            "rename-session": #{opts: target.opts, values: target.values, args: ["none"]},
            "rename-window": #{opts: target.opts, values: target.values, args: ["none"]},
            "select-window": target,
            "select-pane": target,
            "split-window": #{
                opts: `
                    -t TARGET-PANE  the pane to split
                    -c START-DIRECTORY  the working directory
                    -h  split horizontally (side by side)
                    -v  split vertically (one above the other)
                    -b  put the new pane before the target
                    -d  don't make it the current pane
                    -f  span the full window width or height
                    -I  forward standard input to the pane
                    -l SIZE  the size of the new pane
                    -P  print information about the new pane
                    -Z  zoom the pane
                    -e ENVIRONMENT  set an environment variable
                    -F FORMAT  the format for -P
                `,
                values: #{"-t": "system:tmux_targets", "-c": "dirs", "-l": "none", "-e": "none", "-F": "none"},
                args: ["commands", "files"],
            },
            "send-keys": #{
                opts: `
                    -t TARGET-PANE  the pane to send to
                    -l  send the keys literally
                    -H  keys are hexadecimal
                    -K  send the keys to the client
                    -M  pass through a mouse event
                    -R  reset the terminal state
                    -X  send a copy mode command
                    -N REPEAT-COUNT  repeat the keys
                `,
                values: #{"-t": "system:tmux_targets", "-N": "none"},
                args: ["none"],
            },
            "switch-client": #{
                opts: `
                    -t TARGET-SESSION  the session to switch to
                    -c TARGET-CLIENT  the client to switch
                    -l  the last session
                    -n  the next session
                    -p  the previous session
                    -r  toggle read-only
                    -Z  keep the window zoomed
                `,
                values: #{"-t": "system:tmux_targets", "-c": "none"},
                args: ["none"],
            },
            "detach-client": #{
                opts: `
                    -s TARGET-SESSION  detach all clients of this session
                    -t TARGET-CLIENT  the client to detach
                    -a  detach all clients but the current one
                    -P  send SIGHUP to the parent of the client
                `,
                values: #{"-s": "system:tmux_targets", "-t": "none"},
                args: ["none"],
            },
        },
    }
}

fn spec(cmd) {
    switch cmd {
        "systemctl" => systemctl(),
        "journalctl" => journalctl(),
        "loginctl" => loginctl(),
        "ps" => ps(),
        "top" => top(),
        "htop" => htop(),
        "lsof" => lsof(),
        "strace" => strace(),
        "mount" => mount(),
        "umount" => umount(),
        "lsblk" => lsblk(),
        "dmesg" => dmesg(),
        "free" => free(),
        "tmux" => tmux(),
        _ => throw `no spec for ${cmd}`,
    }
}
