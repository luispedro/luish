// Specs for the package managers of systems: apt, apt-get, apt-cache,
// apt-mark and dpkg (Debian, Ubuntu), dnf, yum and rpm (Fedora, RHEL),
// pacman (Arch), zypper (openSUSE), apk (Alpine), brew (Homebrew), snap and
// flatpak (see lib.rhai for their fields).
//
// The packages that can be installed are only listed once the word has a
// letter or more (there are tens of thousands), and not at all for dnf,
// yum and zypper, whose lists are slow to get.

import "kinds" as kinds;

// The lines of the output of a command, without empty ones.
fn lines(cmd) {
    sh::capture(cmd + " 2>/dev/null").out.split("\n").filter(|l| l != "")
}

fn apt_available(cur) {
    if cur == "" { [] } else { lines(`apt-cache --no-generate pkgnames ${sh::quote(cur)}`) }
}

fn deb_installed() {
    lines("dpkg-query -W -f='${db:Status-Abbrev} ${Package}\\n'").filter(|l| l.starts_with("i")).map(|l| l.split()[-1])
}

fn rpm_installed() {
    lines("rpm -qa --qf '%{NAME}\\n'")
}

fn pacman_available(cur) {
    if cur == "" { [] } else { lines("pacman -Slq").filter(|p| p.starts_with(cur)) }
}

// The prefix of Homebrew.
fn brew_prefix() {
    let p = sh::getvar("HOMEBREW_PREFIX");
    if p != () && p != "" {
        return p;
    }
    for d in ["/opt/homebrew", "/home/linuxbrew/.linuxbrew", "/usr/local"] {
        if fs::is_dir(d + "/Cellar") {
            return d;
        }
    }
    ()
}

fn brew_installed() {
    let p = brew_prefix();
    if p == () {
        return [];
    }
    (fs::list_dir(p + "/Cellar") ?? []) + (fs::list_dir(p + "/Caskroom") ?? [])
}

// The formulae and casks that can be installed, from Homebrew's cache of
// its API.
fn brew_available(cur) {
    if cur == "" {
        return [];
    }
    let cache = sh::getvar("HOMEBREW_CACHE");
    let dirs = if cache != () && cache != "" {
        [cache]
    } else {
        [kinds::home() + "/Library/Caches/Homebrew", kinds::home() + "/.cache/Homebrew"]
    };
    let out = [];
    for d in dirs {
        for f in ["formula_names.txt", "cask_names.txt"] {
            for n in (fs::read_file(d + "/api/" + f) ?? "").split("\n") {
                if n != "" && n.starts_with(cur) {
                    out.push(n);
                }
            }
        }
    }
    out
}

fn brew_taps() {
    let p = brew_prefix();
    if p == () {
        return [];
    }
    let out = [];
    let taps = p + "/Library/Taps";
    for user in fs::list_dir(taps) ?? [] {
        for repo in fs::list_dir(taps + "/" + user) ?? [] {
            if repo.starts_with("homebrew-") {
                out.push(user + "/" + repo.sub_string(9));
            }
        }
    }
    out
}

fn snaps() {
    let out = [];
    for line in lines("snap list").extract(1) {
        let w = line.split();
        if !w.is_empty() {
            out.push(#{value: w[0], desc: if w.len() > 1 { w[1] } else { "" }});
        }
    }
    out
}

fn flatpak_installed() {
    let out = [];
    for line in lines("flatpak list --columns=application,name") {
        let tab = line.index_of("\t");
        out.push(if tab > 0 { #{value: line.sub_string(0, tab), desc: line.sub_string(tab + 1)} } else { line });
    }
    out
}

fn flatpak_available(cur) {
    if cur == "" {
        return [];
    }
    lines("flatpak remote-ls --app --cached --columns=application").filter(|a| a.starts_with(cur))
}

fn flatpak_remotes() {
    lines("flatpak remotes --columns=name")
}

fn kind(name, cur, words) {
    switch name {
        "apt" => apt_available(cur),
        "deb_installed" => deb_installed(),
        "debs" => {
            let r = kinds::files(cur, "");
            r.candidates = r.candidates.filter(|c| type_of(c) == "map" || c.ends_with(".deb"));
            r
        }
        "rpms" => {
            let r = kinds::files(cur, "");
            r.candidates = r.candidates.filter(|c| type_of(c) == "map" || c.ends_with(".rpm"));
            r
        }
        "pkgs" => {
            let r = kinds::files(cur, "");
            r.candidates = r.candidates.filter(|c| type_of(c) == "map" || c.contains(".pkg.tar"));
            r
        }
        "rpm_installed" => rpm_installed(),
        "pacman" => pacman_available(cur),
        "pacman_installed" => lines("pacman -Qq"),
        "pacman_groups" => lines("pacman -Sgq"),
        "pacman_repos" => lines("pacman-conf --repo-list"),
        "apk" => if cur == "" { [] } else { lines(`apk search -q ${sh::quote(cur + "*")}`) },
        "apk_installed" => lines("apk info -q"),
        "brew" => brew_available(cur),
        "brew_installed" => brew_installed(),
        "brew_taps" => brew_taps(),
        "snaps" => snaps(),
        "flatpak" => flatpak_available(cur),
        "flatpak_installed" => flatpak_installed(),
        "flatpak_remotes" => flatpak_remotes(),
        _ => throw `unknown kind: ${name}`,
    }
}

fn apt_opts() {
    `
        -h, --help  show the help
        -v, --version  show the version
        -y, --yes, --assume-yes  answer yes to all prompts
        --assume-no  answer no to all prompts
        -q, --quiet  produce output suitable for logging
        -s, --simulate, --dry-run, --just-print  show what would be done, without doing it
        -d, --download-only  only download the packages
        -f, --fix-broken  fix a system with broken dependencies
        -m, --ignore-missing, --fix-missing  ignore missing packages
        --no-download  don't download packages, use those already downloaded
        --no-install-recommends  don't install recommended packages
        --install-recommends  install recommended packages
        --install-suggests  install suggested packages
        -t, --target-release RELEASE  the release to install packages from
        --host-architecture ARCH  the architecture of the packages to build
        --reinstall  reinstall packages that are already installed
        --only-upgrade  upgrade packages that are installed, and install no new ones
        --no-upgrade  don't upgrade packages that are installed
        --allow-downgrades  allow downgrading packages
        --allow-remove-essential  allow removing essential packages
        --allow-change-held-packages  allow changing held packages
        --purge  purge the packages that are removed
        --auto-remove, --autoremove  also remove the dependencies that are no longer needed
        -V, --verbose-versions  show the full versions of packages
        --print-uris  print the URIs of the files to fetch
        -o, --option OPTION  set a configuration option
        -c, --config-file FILE  read this configuration file
    `
}

fn apt_values() {
    #{"-t": "none", "--host-architecture": "none", "-o": "none"}
}

fn apt() {
    let available = #{args: ["packages:apt"]};
    let installed = #{args: ["packages:deb_installed"]};
    #{
        common: apt_opts(),
        values: apt_values(),
        commands: `
            list  list packages based on package names
            search  search in package descriptions
            show  show package details
            install  install packages
            reinstall  reinstall packages
            remove  remove packages
            purge  remove packages and their configuration files
            autoremove  remove all unused packages
            update  update the list of available packages
            upgrade  upgrade the system by installing and upgrading packages
            full-upgrade, dist-upgrade  upgrade the system by removing, installing and upgrading packages
            edit-sources  edit the source information file
            satisfy  satisfy dependency strings
            policy  show the priorities of packages and sources
            depends  show the dependencies of packages
            rdepends  show the reverse dependencies of packages
            download  download the package files into the current directory
            source  download source packages
            build-dep  install the build-dependencies of source packages
            changelog  show the changelog of packages
            showsrc  show the source records of packages
            clean  erase downloaded package files
            autoclean  erase old downloaded package files
            moo  have some cow powers
        `,
        subs: #{
            list: #{opts: `
                    --installed  list the installed packages
                    --upgradable  list the packages that can be upgraded
                    -a, --all-versions  list all versions
                `, args: ["packages:apt"]},
            search: #{opts: `
                    -n, --names-only  search the package names only
                    --full  show the full records
                `, args: ["none"]},
            show: #{opts: "-a, --all-versions  show all versions", args: available.args},
            install: available,
            reinstall: installed,
            remove: installed,
            purge: installed,
            autoremove: installed,
            upgrade: installed,
            "full-upgrade": #{args: ["none"]},
            update: #{args: ["none"]},
            satisfy: #{args: ["none"]},
            policy: available,
            depends: available,
            rdepends: available,
            download: available,
            source: available,
            "build-dep": available,
            changelog: available,
            showsrc: available,
            clean: #{args: ["none"]},
            autoclean: #{args: ["none"]},
        },
    }
}

fn apt_get() {
    let s = apt();
    s.commands = `
        update  retrieve new lists of packages
        upgrade  perform an upgrade
        install  install new packages
        reinstall  reinstall packages
        remove  remove packages
        purge  remove packages and their configuration files
        autoremove  remove all unused packages
        dist-upgrade  distribution upgrade
        dselect-upgrade  follow dselect selections
        build-dep  configure the build-dependencies of source packages
        satisfy  satisfy dependency strings
        clean  erase downloaded archive files
        autoclean  erase old downloaded archive files
        check  verify that there are no broken dependencies
        source  download source archives
        download  download the binary package into the current directory
        changelog  download and show the changelog of the given package
        indextargets  list the index files apt-get update would download
    `;
    s
}

fn apt_cache() {
    let available = #{args: ["packages:apt"]};
    #{
        opts: `
            -h, --help  show the help
            -v, --version  show the version
            -p, --pkg-cache FILE  the package cache to use
            -s, --src-cache FILE  the source cache to use
            -q, --quiet  produce output suitable for logging
            -i, --important  print only important dependencies
            --no-pre-depends, --no-depends, --no-recommends, --no-suggests  leave out a kind of dependency
            --installed  limit the output to installed packages
            -a, --all-versions  print all versions
            -g, --generate  regenerate the cache if needed
            --no-generate  don't regenerate the cache
            -n, --names-only  search the package names only
            -f, --full  print the full records when searching
            --no-all-names  with pkgnames, only print real packages
            --recurse  make depends and rdepends recursive
            -o, --option OPTION  set a configuration option
            -c, --config-file FILE  read this configuration file
        `,
        values: #{"-o": "none"},
        commands: `
            gencaches  build the package and source caches
            showpkg  show general information about packages
            showsrc  show the source records of packages
            stats  show some basic statistics
            dump  show the whole cache
            dumpavail  print the available versions
            unmet  show unmet dependencies
            search  search the package list for a regular expression
            show  show readable records of packages
            depends  show the dependencies of packages
            rdepends  show the reverse dependencies of packages
            pkgnames  list the names of all packages
            dotty  generate a dot graph of packages
            xvcg  generate a xvcg graph of packages
            policy  show the priorities of packages and sources
            madison  show the available versions of packages
        `,
        subs: #{
            showpkg: available, showsrc: available, show: available, depends: available, rdepends: available,
            dotty: available, xvcg: available, policy: available, madison: available,
            search: #{args: ["none"]}, pkgnames: #{args: ["none"]},
        },
    }
}

fn apt_mark() {
    let installed = #{args: ["packages:deb_installed"]};
    #{
        opts: `
            -h, --help  show the help
            -v, --version  show the version
            -f, --file FILE  read and write the package statistics to FILE
            -s, --simulate  show what would be done
            -o, --option OPTION  set a configuration option
        `,
        values: #{"-o": "none"},
        commands: `
            auto  mark packages as automatically installed
            manual, install  mark packages as manually installed
            minimize-manual  mark all dependencies of meta packages as automatically installed
            hold  hold back packages
            unhold  unset packages as held back
            showauto  print the automatically installed packages
            showmanual, showinstall  print the manually installed packages
            showhold  print the packages that are held back
        `,
        subs: #{auto: installed, manual: installed, hold: installed, unhold: installed},
    }
}

fn dpkg() {
    let installed = ["packages:deb_installed"];
    let debs = ["packages:debs"];
    #{
        opts: `
            -i, --install  install the package files
            --unpack  unpack the package files, without configuring them
            -A, --record-avail  update the available packages from package files
            --configure  configure unpacked packages (or -a, all of them)
            --triggers-only  process only triggers
            -r, --remove  remove packages, but not their configuration files
            -P, --purge  remove packages and their configuration files
            -V, --verify  verify the integrity of packages
            --get-selections  print the selections
            --set-selections  set the selections from standard input
            --clear-selections  deselect every non-essential package
            --update-avail  replace the available packages
            --merge-avail  merge with the available packages
            --clear-avail  erase the available packages
            -C, --audit  check for broken packages
            --yet-to-unpack  print the packages selected for installation
            --predep-package  print the pre-dependency to unpack next
            --add-architecture  add an architecture
            --remove-architecture  remove an architecture
            --print-architecture  print the dpkg architecture
            --print-foreign-architectures  print the foreign architectures
            --compare-versions  compare version numbers
            -l, --list  list packages briefly
            -L, --listfiles  list the files installed by packages
            -s, --status  show the status of packages
            -p, --print-avail  show the details of available packages
            -S, --search  find the packages that own files
            -c, --contents  list the contents of package files
            -I, --info  show information about package files
            -x, --extract  extract the files of a package file
            -a, --pending  with --configure, --remove or --purge, all pending packages
            -R, --recursive  go into directories recursively for package files
            -G, --refuse-downgrade  skip packages with earlier versions than installed
            -B, --auto-deconfigure  install even if it would break some other package
            --no-triggers  skip or defer triggers processing
            --dry-run, --no-act  just say what we would do
            -D, --debug=OCTAL  enable debugging
            --force-THINGS  override problems
            --root=DIR  install on a different root directory
            --instdir=DIR  change the installation directory without changing the admin directory
            --admindir=DIR  use DIR instead of /var/lib/dpkg
            --path-exclude=PATTERN  do not install paths matching a shell pattern
            --path-include=PATTERN  re-include a pattern after a previous exclusion
            -O, --selected-only  skip packages not selected for install or upgrade
            -E, --skip-same-version  skip packages whose same version is installed
            -?, --help  show the help
            --version  show the version
        `,
        values: #{"-D": "none", "--root": "dirs", "--instdir": "dirs", "--admindir": "dirs",
            "--path-exclude": "none", "--path-include": "none"},
        args_if: #{
            "-i": debs, "--unpack": debs, "-A": debs, "--configure": installed, "--triggers-only": installed,
            "-r": installed, "-P": installed, "-V": installed, "-l": installed, "-L": installed, "-s": installed,
            "-p": ["packages:apt"], "-S": ["files"], "-c": debs, "-I": debs, "-x": debs + ["dirs"],
        },
        args: ["none"],
    }
}

fn dnf_opts() {
    `
        -h, --help  show the help
        --version  show the version
        -y, --assumeyes  answer yes to all questions
        --assumeno  answer no to all questions
        -q, --quiet  show only the results
        -v, --verbose  show more output
        -C, --cacheonly  run entirely from the cache
        --refresh  set the metadata as expired before running
        --repo, --repoid REPO  enable only the given repositories
        --enablerepo REPO  enable more repositories
        --disablerepo REPO  disable repositories
        -x, --exclude PACKAGE  exclude packages
        --best  try the best available package versions
        --nobest  allow packages that aren't the best version
        --allowerasing  allow erasing installed packages to resolve dependencies
        --skip-broken  skip packages that have broken dependencies
        --skip-unavailable  skip packages that are unavailable
        --nogpgcheck  skip checking the GPG signatures of packages
        --downloadonly  only download packages
        --setopt OPTION=VALUE  set a configuration option
        --releasever VERSION  override the release version
        --installroot DIR  set the install root
        --security  include only security updates
        --advisory ADVISORY  include packages of these advisories
        --forcearch ARCH  force the architecture
        --noplugins  disable all plugins
        --disableplugin PLUGIN  disable plugins
        --enableplugin PLUGIN  enable plugins
        -c, --config FILE  the configuration file
    `
}

fn dnf() {
    let installed = #{args: ["packages:rpm_installed"]};
    let available = #{args: ["none"]};
    #{
        common: dnf_opts(),
        values: #{"--repo": "none", "--enablerepo": "none", "--disablerepo": "none", "-x": "packages:rpm_installed",
            "--setopt": "none", "--releasever": "none", "--installroot": "dirs", "--advisory": "none",
            "--forcearch": "none", "--disableplugin": "none", "--enableplugin": "none"},
        commands: `
            install, in  install packages
            remove, rm, erase  remove packages
            reinstall, rei  reinstall packages
            upgrade, update, up  upgrade packages
            downgrade, dg  downgrade packages
            distro-sync, dsync  synchronize the installed packages to the latest available versions
            autoremove  remove the packages that are no longer needed
            check-update, check-upgrade  check for available upgrades
            clean  remove cached data
            info, if  show information about packages
            list, ls  list packages
            search, se  search for packages
            provides, whatprovides, wp  find the packages that provide a file or feature
            repolist  list the repositories
            repoquery, rq  query the packages of repositories
            repo  manage repositories
            makecache, mc  generate the metadata cache
            history, hist  show or undo transactions
            group, groups, grp  manage groups
            module  manage modules
            mark  mark packages as installed by the user or as dependencies
            swap  remove a package and install another in one transaction
            builddep  install the build dependencies of packages
            config-manager  manage the configuration and repositories
            copr  manage Copr repositories
            download  download packages
            needs-restarting  report the processes that need restarting
            shell, sh  run an interactive shell
            advisory, updateinfo  show advisories
            offline  manage offline transactions
            system-upgrade  upgrade the system to a new release
            help  show the help
        `,
        subs: #{
            install: available, remove: installed, reinstall: installed, upgrade: installed, downgrade: installed,
            "distro-sync": installed, autoremove: installed, info: installed, list: installed, search: available,
            provides: #{args: ["files"]}, repoquery: installed, mark: installed, swap: installed,
            builddep: #{args: ["files"]}, download: available,
            clean: #{args: [["all", "dbcache", "expire-cache", "metadata", "packages"]]},
            history: #{args: [["list", "info", "undo", "redo", "rollback", "store", "userinstalled"]]},
            group: #{args: [["list", "info", "install", "remove", "upgrade", "mark"]]},
            "module": #{args: [["list", "info", "enable", "disable", "reset", "install", "remove", "switch-to"]]},
        },
    }
}

fn rpm() {
    let installed = ["packages:rpm_installed"];
    #{
        opts: `
            -q, --query  query packages
            -i, --install  install packages
            -U, --upgrade  upgrade packages
            -F, --freshen  upgrade packages, if they are installed
            -e, --erase  erase (uninstall) packages
            -V, --verify  verify packages
            -a, --all  query or verify all packages
            -f, --file  query or verify the package owning files
            -p, --package  query or verify a package file
            -g, --group  query or verify the packages of a group
            -l, --list  list the files of the packages
            -c, --configfiles  list the configuration files
            -d, --docfiles  list the documentation files
            -R, --requires  list the capabilities the package requires
            --provides  list the capabilities the package provides
            --whatprovides  query the packages that provide a capability
            --whatrequires  query the packages that require a capability
            --changelog  show the changelog
            --scripts  list the scripts of the packages
            --qf, --queryformat QUERYFORMAT  the format of the output
            -h, --hash  print hash marks as the package installs
            -v, --verbose  provide more detailed output
            --nodeps  do not verify the dependencies of packages
            --force  like --replacepkgs --replacefiles --oldpackage
            --replacepkgs  reinstall packages that are already installed
            --replacefiles  ignore file conflicts between packages
            --oldpackage  allow an upgrade to replace a newer package with an older one
            --test  don't install, but tell if it would work or not
            --nosignature  don't verify the signatures of packages
            --import  import armored public keys
            --checksig  check the signatures of package files
            --root DIRECTORY  use DIRECTORY as the top level directory
            --dbpath DIRECTORY  use the database in DIRECTORY
            --rebuilddb  rebuild the database
            -?, --help  print the help
            --version  print the version
        `,
        values: #{"--qf": "none", "--root": "dirs", "--dbpath": "dirs"},
        args_if: #{
            "-p": ["packages:rpms"], "-f": ["files"], "-i": ["packages:rpms"], "-U": ["packages:rpms"],
            "-F": ["packages:rpms"], "--checksig": ["packages:rpms"], "--import": ["files"], "-q": installed,
            "-e": installed, "-V": installed,
        },
        args: ["none"],
    }
}

fn pacman() {
    let common = `
        -b, --dbpath PATH  set an alternate database location
        -r, --root PATH  set an alternate installation root
        -v, --verbose  be verbose
        --arch ARCH  set an alternate architecture
        --cachedir DIR  set an alternate package cache location
        --color WHEN  colorize the output
        --config PATH  set an alternate configuration file
        --debug  display debug messages
        --gpgdir PATH  set an alternate home directory for GnuPG
        --hookdir DIR  set an alternate hook location
        --logfile PATH  set an alternate log file
        --noconfirm  do not ask for any confirmation
        --confirm  always ask for confirmation
        --disable-download-timeout  use relaxed timeouts for downloads
        --sysroot DIR  operate on a mounted guest system (root-only)
    `;
    let transaction = `
        -d, --nodeps  skip dependency version checks (-dd to skip all checks)
        --assume-installed PACKAGE=VERSION  add a virtual package to satisfy dependencies
        --dbonly  only modify database entries, not package files
        --noprogressbar  do not show a progress bar when downloading files
        --noscriptlet  do not execute the install scriptlet if one exists
        -p, --print  print the targets instead of performing the operation
        --print-format STRING  specify how the targets should be printed
    `;
    let upgrade = `
        -w, --downloadonly  download packages but do not install or upgrade anything
        --asdeps  install packages as non-explicitly installed
        --asexplicit  install packages as explicitly installed
        --ignore PACKAGE  ignore a package upgrade (can be used more than once)
        --ignoregroup GROUP  ignore a group upgrade (can be used more than once)
        --needed  do not reinstall up to date packages
        --overwrite PATH  overwrite conflicting files (can be used more than once)
    `;
    let installed = ["packages:pacman_installed"];
    #{
        opts: `
            -S, --sync  synchronize packages
            -Q, --query  query the package database
            -R, --remove  remove packages from the system
            -U, --upgrade  upgrade or add packages from files or URLs
            -D, --database  operate on the package database
            -F, --files  query the files database
            -T, --deptest  check dependencies
            -V, --version  display the version and exit
            -h, --help  display the help
        `,
        modes: #{
            "-S": #{
                opts: common + transaction + upgrade + `
                    -c, --clean  remove old packages from the cache directory (-cc for all)
                    -g, --groups  view all members of a package group
                    -i, --info  view package information (-ii for extended information)
                    -l, --list  view a list of the packages of a repository
                    -q, --quiet  show less information for query and search
                    -s, --search  search the remote repositories for matching strings
                    -u, --sysupgrade  upgrade installed packages (-uu enables downgrades)
                    -y, --refresh  download fresh package databases from the server
                `,
                args: ["packages:pacman"],
                args_if: #{"-s": ["none"], "-l": ["packages:pacman_repos"], "-g": ["packages:pacman_groups"]},
            },
            "-Q": #{
                opts: common + `
                    -c, --changelog  view the changelog of a package
                    -d, --deps  list packages installed as dependencies
                    -e, --explicit  list packages explicitly installed
                    -g, --groups  view all members of a package group
                    -i, --info  view package information (-ii for backup files)
                    -k, --check  check that package files exist (-kk for file properties)
                    -l, --list  list the files owned by the queried package
                    -m, --foreign  list installed packages not found in sync databases
                    -n, --native  list installed packages only found in sync databases
                    -o, --owns  query the package that owns a file
                    -p, --file  query a package file instead of the database
                    -q, --quiet  show less information for query and search
                    -s, --search  search locally-installed packages for matching strings
                    -t, --unrequired  list packages not required by any package
                    -u, --upgrades  list outdated packages
                `,
                args: installed,
                args_if: #{"-o": ["files"], "-p": ["packages:pkgs"], "-s": ["none"]},
            },
            "-R": #{
                opts: common + transaction + `
                    -c, --cascade  remove packages and all packages that depend on them
                    -n, --nosave  remove configuration files
                    -s, --recursive  remove unnecessary dependencies
                    -u, --unneeded  remove unneeded packages
                `,
                args: installed,
            },
            "-U": #{opts: common + transaction + upgrade, args: ["packages:pkgs"]},
            "-D": #{
                opts: common + `
                    --asdeps  mark packages as non-explicitly installed
                    --asexplicit  mark packages as explicitly installed
                    -k, --check  test local database for validity (-kk for sync databases)
                    -q, --quiet  suppress output of success messages
                `,
                args: installed,
            },
            "-F": #{
                opts: common + `
                    -l, --list  list the files owned by the queried package
                    -q, --quiet  show less information for query and search
                    -x, --regex  enable searching using regular expressions
                    -y, --refresh  download fresh package databases from the server
                    --machinereadable  produce machine-readable output
                `,
                args: ["files"],
                args_if: #{"-l": ["packages:pacman"]},
            },
            "-T": #{opts: common, args: ["none"]},
        },
        values: #{
            "--color": ["always", "auto", "never"], "--cachedir": "dirs", "--hookdir": "dirs", "--sysroot": "dirs",
            "--ignore": "packages:pacman_installed", "--ignoregroup": "packages:pacman_groups",
            "--assume-installed": "none", "--print-format": "none", "--arch": "none",
        },
        args: ["none"],
    }
}

fn zypper() {
    let installed = #{args: ["packages:rpm_installed"]};
    #{
        opts: `
            -h, --help  show the help
            -V, --version  output the version number
            -q, --quiet  suppress normal output, print only error messages
            -v, --verbose  increase verbosity
            -n, --non-interactive  do not ask anything, use default answers automatically
            -x, --xmlout  switch to XML output
            -t, --terse  terse output for machine consumption
            -s, --table-style STYLE  the table style
            --no-refresh  do not refresh the repositories
            --no-gpg-checks  ignore GPG check failures and continue
            --gpg-auto-import-keys  automatically trust and import new repository signing keys
            -c, --config FILE  use the specified config file instead of the default
            -R, --root DIR  operate on a different root directory
            --reposd-dir DIR  use an alternative repository definition file directory
            --releasever VERSION  set the value of the $releasever variable
        `,
        common: `
            -r, --repo ALIAS  work only with the specified repository
            -y, --no-confirm  don't ask for confirmation
            -d, --download-only  only download the packages, do not install
            -D, --dry-run  test the operation, do not actually change anything
            -f, --force  install even if the item is already installed (reinstall)
            -l, --auto-agree-with-licenses  automatically say yes to license confirmations
            --no-recommends  do not install recommended packages
            --recommends  install recommended packages too
            --oldpackage  allow replacing a newer item with an older one
            --allow-vendor-change  allow vendor changes
            --allow-downgrade  allow downgrades
            -C, --capability  select packages by capability
            -t, --type TYPE  the type of package (package, patch, pattern, product)
        `,
        values: #{"-s": "none", "-R": "dirs", "--reposd-dir": "dirs", "--releasever": "none", "-r": "none",
            "-t": ["package", "patch", "pattern", "product", "srcpackage", "application"]},
        commands: `
            install, in  install packages
            remove, rm  remove packages
            verify, ve  verify the integrity of package dependencies
            source-install, si  install source packages and their build dependencies
            install-new-recommends, inr  install newly added packages recommended by installed packages
            update, up  update installed packages with newer versions
            list-updates, lu  list available updates
            patch  install needed patches
            list-patches, lp  list needed patches
            dist-upgrade, dup  perform a distribution upgrade
            patch-check, pchk  check for patches
            search, se  search for packages matching a pattern
            info, if  show full information for specified packages
            patch-info  show full information for specified patches
            pattern-info  show full information for specified patterns
            product-info  show full information for specified products
            patches, pch  list all available patches
            packages, pa  list all available packages
            patterns, pt  list all available patterns
            products, pd  list all available products
            what-provides, wp  list packages providing specified capability
            repos, lr  list all defined repositories
            addrepo, ar  add a new repository
            removerepo, rr  remove specified repository
            renamerepo, nr  rename specified repository
            modifyrepo, mr  modify specified repository
            refresh, ref  refresh all repositories
            clean, cc  clean local caches
            services, ls  list all defined services
            addservice, as  add a new service
            removeservice, rs  remove specified service
            refresh-services, refs  refresh all services
            locks, ll  list current package locks
            addlock, al  add a package lock
            removelock, rl  remove a package lock
            cleanlocks, cl  remove useless locks
            ps  list running processes which use files deleted by recent upgrades
            needs-rebooting  check if the reboot-needed flag was set
            versioncmp, vcmp  compare two version strings
            targetos, tos  print the target operating system ID string
            licenses  print a report about licenses and EULAs of installed packages
            download  download rpms specified on the command line to a local directory
            source-download  download source rpms for all installed packages to a local directory
            help, ?  print help
            shell, sh  accept multiple commands at once
        `,
        subs: #{
            install: #{args: ["none"]}, remove: installed, update: installed, verify: installed,
            info: installed, addlock: installed, removelock: installed, search: #{args: ["none"]},
            "source-install": #{args: ["none"]}, download: #{args: ["none"]},
        },
    }
}

fn apk() {
    let installed = #{args: ["packages:apk_installed"]};
    let available = #{args: ["packages:apk"]};
    #{
        common: `
            -h, --help  show the help
            -V, --version  print the program version and exit
            -q, --quiet  print less information
            -v, --verbose  print more information
            -i, --interactive  ask confirmation for certain operations
            -p, --root ROOT  manage the file system at ROOT
            -X, --repository REPO  specify an additional package repository
            -U, --update-cache  update the repository cache
            --allow-untrusted  install packages with untrusted signatures or no signature
            --arch ARCH  temporarily override the architecture
            --cache-dir CACHEDIR  temporarily override the cache directory
            --force-broken-world  continue even if world cannot be satisfied
            --force-overwrite  overwrite files in other packages
            --no-cache  do not use any local cache path
            --no-network  do not use the network
            --no-progress  disable the progress bar
            --progress  show a progress bar
            -s, --simulate  simulate the requested operation without making any changes
        `,
        values: #{"-p": "dirs", "-X": "none", "--arch": "none", "--cache-dir": "dirs"},
        commands: `
            add  add packages to world and commit changes
            del  remove packages from world and commit changes
            fix  fix, reinstall or upgrade packages without modifying world
            update  update repository indexes
            upgrade  install upgrades available from repositories
            cache  manage the local package cache
            info  give detailed information about packages or repositories
            list  list packages matching a pattern or other criteria
            dot  render dependencies as graphviz graphs
            policy  show repository policy for packages
            search  search for packages by name or description
            index  create a repository index file from packages
            fetch  download packages from repositories to a local directory
            manifest  show the checksums of package contents
            verify  verify package integrity and signature
            audit  audit system for changes
            stats  show statistics about repositories and installations
        `,
        subs: #{
            add: #{opts: `
                    -t, --virtual NAME  create a virtual package with the given dependencies
                    -l, --latest  select the latest version
                    -u, --upgrade  upgrade the packages and their dependencies
                    --no-chown  do not change the ownership of files
                `,
                values: #{"-t": "none"}, args: available.args},
            del: #{opts: `
                    -r, --rdepends  recursively delete all top-level reverse dependencies too
                `, args: installed.args},
            fix: installed, info: installed, dot: installed, policy: available, fetch: available,
            manifest: installed, search: #{args: ["none"]},
            upgrade: #{opts: `
                    -a, --available  reset all packages to versions available from current repositories
                    -l, --latest  select the latest version
                    --no-self-upgrade  do not do an early upgrade of apk-tools
                `,
                args: installed.args},
        },
    }
}

fn brew() {
    let installed = #{args: ["packages:brew_installed"]};
    let available = #{args: ["packages:brew"]};
    let common = `
        -d, --debug  display any debugging information
        -q, --quiet  make some output more quiet
        -v, --verbose  make some output more verbose
        -h, --help  show the help
    `;
    #{
        opts: `
            --version  print the version
            --prefix  print the prefix
            --cellar  print the Cellar path
            --cache  print the cache path
            --repository  print the repository path
        `,
        common: common,
        commands: `
            install  install formulae or casks
            uninstall, remove, rm  uninstall formulae or casks
            reinstall  uninstall and install formulae or casks again
            upgrade  upgrade outdated formulae and casks
            update, up  fetch the newest version of Homebrew and all formulae
            list, ls  list installed formulae or casks
            info, abv  display information about formulae or casks
            search  search for formulae or casks
            outdated  list installed formulae and casks with newer versions
            cleanup  remove stale lock files and outdated downloads
            autoremove  uninstall formulae that were only installed as dependencies
            deps  show the dependencies of formulae
            uses  show the formulae that use a formula
            leaves  list installed formulae that are not dependencies of another
            doctor, dr  check your system for potential problems
            config  show Homebrew and system configuration info
            tap  tap a formula repository
            untap  remove a tapped formula repository
            services  manage background services
            bundle  install or upgrade from a Brewfile
            link, ln  symlink the files of a formula into the prefix
            unlink  remove the symlinks of a formula from the prefix
            pin  pin formulae, to keep them from being upgraded
            unpin  unpin formulae
            fetch  download a bottle or source tarball
            home, homepage  open the homepage of a formula or cask
            edit  open a formula or cask in the editor
            cat  display the source of a formula or cask
            desc  display the description of formulae or casks
            log  show the git log of a formula or cask
            commands  show the built-in and external commands
            help  show the help of a command
        `,
        subs: #{
            install: #{opts: `
                --formula, --formulae  treat all named arguments as formulae
                --cask, --casks  treat all named arguments as casks
                -f, --force  install without checking for previously installed keg-only or non-migrated versions
                -n, --dry-run  show what would be installed, but do not actually install anything
                -s, --build-from-source  compile from source even if a bottle is available
                --HEAD  install the HEAD version
                --fetch-HEAD  fetch the upstream repository to detect if the HEAD installation is outdated
                --ignore-dependencies  skip installing any dependencies of any kind
                --only-dependencies  install the dependencies but not the formula itself
                --overwrite  delete files that already exist in the prefix while linking
                --keep-tmp  retain the temporary files created during installation
                -i, --interactive  download and patch the formula, then open a shell
                --adopt  adopt existing artifacts in the destination that are identical to those being installed
                --no-quarantine  disable quarantining for downloaded files
            `, args: available.args},
            uninstall: #{opts: `
                    --formula  treat all named arguments as formulae
                    --cask  treat all named arguments as casks
                    -f, --force  delete all installed versions
                    --zap  remove all files associated with a cask
                    --ignore-dependencies  don't fail uninstall even if it is a dependency
                `,
                args: installed.args},
            reinstall: installed, upgrade: installed, info: #{args: ["packages:brew_installed", "packages:brew"]},
            deps: installed, uses: installed, link: installed, unlink: installed, pin: installed, unpin: installed,
            fetch: available, home: available, edit: available, cat: available, desc: available, log: available,
            list: #{opts: `
                    --formula  list only formulae
                    --cask  list only casks
                    --versions  show the version number of each
                    -1  one entry per line
                    -l  long format
                    --pinned  list only pinned formulae
                `,
                args: installed.args},
            search: #{opts: `
                    --formula  search only formulae
                    --cask  search only casks
                    --desc  search descriptions
                `, args: ["none"]},
            outdated: #{opts: `
                    --formula  list only formulae
                    --cask  list only casks
                    --greedy  also casks with auto-updates
                    --json  output JSON
                `, args: installed.args},
            tap: #{args: ["none"]},
            untap: #{args: ["packages:brew_taps"]},
            services: #{commands: `
                list  list all managed services
                info  list all information about a service
                run  run the service without registering it to launch at login or boot
                start  start the service and register it to launch at login or boot
                stop  stop the service and unregister it
                kill  stop the service without unregistering it
                restart  stop and start the service
                cleanup  remove all unused services
            `, args: installed.args},
        },
    }
}

fn snap() {
    let installed = #{args: ["packages:snaps"]};
    #{
        opts: "-h, --help  show the help\n--version  show the version",
        commands: `
            install  install snaps
            remove  remove snaps
            refresh  refresh snaps
            revert  revert snaps to their previous revision
            list  list the installed snaps
            find, search  find snaps in the store
            info  show detailed information about snaps
            run  run a snap command
            connections  list the interface connections
            connect  connect a plug to a slot
            disconnect  disconnect a plug from a slot
            enable  enable a snap
            disable  disable a snap
            services  query the services of snaps
            start  start services
            stop  stop services
            restart  restart services
            logs  show the logs of services
            alias  set up a manual alias
            aliases  list the aliases
            unalias  remove a manual alias, or an automatic alias of a snap
            changes  list the system changes
            tasks  list the tasks of a change
            abort  abort a pending change
            download  download a snap and its assertions
            set  change the configuration options of a snap
            get  print the configuration options of a snap
            unset  remove the configuration options of a snap
            switch  switch the tracking channel of a snap
            saved  list the saved snapshots
            save  save a snapshot of the current data
            restore  restore a snapshot
            version  show the version details
            whoami  show the email the user is logged in with
            login  authenticate to the snap daemon and the store
            logout  log out of the snap store
            help  show the help of commands
        `,
        subs: #{
            install: #{opts: `
                --classic  put the snap in classic mode and disable security confinement
                --channel CHANNEL  use this channel instead of stable
                --edge  install from the edge channel
                --beta  install from the beta channel
                --candidate  install from the candidate channel
                --stable  install from the stable channel
                --devmode  put the snap in development mode and disable security confinement
                --jailmode  put the snap in enforced mode even if it asked for devmode
                --dangerous  install the given snap file even if there are no assertions
                --revision REVISION  install the given revision
                --name NAME  install the snap under the given instance name
                --unaliased  install the snap without enabling its automatic aliases
            `, values: #{"--channel": "none", "--revision": "none", "--name": "none"}, args: ["none"]},
            remove: #{opts: `
                    --purge  remove without saving a snapshot of its data
                    --revision REVISION  remove only the given revision
                `,
                values: #{"--revision": "none"}, args: installed.args},
            refresh: #{opts: `
                    --channel CHANNEL  use this channel instead of stable
                    --classic  put the snap in classic mode
                    --list  show the available updates
                    --hold  hold refreshes
                    --unhold  remove refresh holds
                `,
                values: #{"--channel": "none"}, args: installed.args},
            revert: installed, info: #{args: ["packages:snaps", "none"]}, run: installed, connections: installed,
            enable: installed, disable: installed, services: installed, start: installed, stop: installed,
            restart: installed, logs: installed, get: installed, set: installed, unset: installed,
            "switch": installed, save: installed, alias: installed, find: #{args: ["none"]},
            download: #{args: ["none"]},
        },
    }
}

fn flatpak() {
    let installed = #{args: ["packages:flatpak_installed"]};
    let scope = `
        --user  work on the user installation
        --system  work on the system-wide installation (the default)
        --installation NAME  work on a non-default system-wide installation
        -y, --assumeyes  answer yes to all questions
        --noninteractive  produce minimal output and don't ask questions
    `;
    #{
        opts: `
            -h, --help  show the help
            --version  print version information and exit
            --default-arch  print the default arch and exit
            --supported-arches  print the supported arches and exit
            --gl-drivers  print the active gl drivers and exit
            --installations  print the paths of the system installations and exit
            -v, --verbose  show debug information
            --ostree-verbose  show OSTree debug information
        `,
        common: scope,
        values: #{"--installation": "none"},
        commands: `
            install  install an application or runtime
            update, upgrade  update installed applications or runtimes
            uninstall, remove  uninstall installed applications or runtimes
            mask  mask out updates and automatic installation
            pin  pin a runtime to prevent automatic removal
            list  list installed apps and runtimes
            info  show info for an installed app or runtime
            history  show the history
            config  configure flatpak
            repair  repair a flatpak installation
            create-usb  put applications or runtimes onto removable media
            search  search for remote apps or runtimes
            run  run an application
            override  override permissions for an application
            make-current  specify the default version to run
            enter  enter the namespace of a running application
            ps  enumerate running applications
            kill  stop a running application
            documents  list exported files
            document-export  grant an application access to a specific file
            document-unexport  revoke access to a specific file
            document-info  show information about a specific file
            permissions  list permissions
            permission-remove  remove an item from the permission store
            permission-set  set permissions
            permission-show  show app permissions
            permission-reset  reset app permissions
            remotes  list all configured remotes
            remote-add  add a new remote repository (by URL)
            remote-modify  modify properties of a configured remote
            remote-delete  delete a configured remote
            remote-ls  list the contents of a configured remote
            remote-info  show information about a remote app or runtime
            build-init  initialize a directory for building
            build  run a build command inside the build directory
            build-finish  finish a build directory for export
            build-export  export a build directory to a repository
            build-bundle  create a bundle file from a ref in a local repository
            build-import-bundle  import a bundle file
            build-sign  sign an application or runtime
            build-update-repo  update the summary file in a repository
            build-commit-from  create a new commit based on an existing ref
            repo  show information about a repository
        `,
        subs: #{
            install: #{opts: `
                    --or-update  update the app if it is already installed
                    --no-deps  don't install dependencies
                    --app  look for apps
                    --runtime  look for runtimes
                    --from  take the argument as a .flatpakref file
                `,
                args: ["packages:flatpak_remotes", "packages:flatpak"]},
            update: #{opts: `
                    --no-deps  don't update dependencies
                    --app  only apps
                    --runtime  only runtimes
                    --commit COMMIT  deploy this commit
                `,
                values: #{"--commit": "none"}, args: installed.args},
            uninstall: #{opts: `
                    --unused  uninstall the unused runtimes
                    --delete-data  delete the data of the app
                    --all  uninstall everything
                    --keep-ref  keep the ref in the local repository
                `,
                args: installed.args},
            info: installed, run: #{opts: `
                    --command COMMAND  the command to run
                    --branch BRANCH  the branch to use
                    --arch ARCH  the arch to use
                    --filesystem FS  expose a file system to the app
                `,
                values: #{"--command": "none", "--branch": "none", "--arch": "none", "--filesystem": "dirs"},
                args: ["packages:flatpak_installed", "files"]},
            override: installed, "make-current": installed, kill: installed, "permission-show": installed,
            "permission-reset": installed, mask: installed, pin: installed,
            list: #{opts: `
                    --app  list apps
                    --runtime  list runtimes
                    -d, --show-details  show extra information
                    --columns FIELD  the fields to show
                `,
                values: #{"--columns": "none"}, args: ["none"]},
            search: #{args: ["none"]},
            "remote-ls": #{opts: `
                    --app  list apps
                    --runtime  list runtimes
                    --updates  show only those with updates
                    --cached  use the cached data
                `, args: ["packages:flatpak_remotes"]},
            "remote-info": #{args: ["packages:flatpak_remotes", "packages:flatpak"]},
            "remote-modify": #{args: ["packages:flatpak_remotes"]},
            "remote-delete": #{args: ["packages:flatpak_remotes"]},
            "remote-add": #{opts: `
                    --if-not-exists  do nothing if the remote exists
                    --from  take the argument as a .flatpakrepo file
                `,
                args: ["none", "files"]},
        },
    }
}

fn spec(cmd) {
    switch cmd {
        "apt" => apt(),
        "apt-get" => apt_get(),
        "apt-cache" => apt_cache(),
        "apt-mark" => apt_mark(),
        "dpkg" => dpkg(),
        "dnf" | "dnf5" | "yum" => dnf(),
        "rpm" => rpm(),
        "pacman" => pacman(),
        "zypper" => zypper(),
        "apk" => apk(),
        "brew" => brew(),
        "snap" => snap(),
        "flatpak" => flatpak(),
        _ => throw `no spec for ${cmd}`,
    }
}
