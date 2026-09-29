// Specs for tools that work on files and text: find, fd, locate, sed, awk,
// jq, rg, less, more, file, ldd, patch, tree, and compressors and
// archivers (gzip, xz, zstd, bzip2, zip) (see lib.rhai for their fields).

import "kinds" as kinds;

fn when() {
    ["always", "auto", "never"]
}

// Directories, and the files whose names end with one of `exts`.
fn with_ext(cur, exts) {
    let r = kinds::files(cur, "");
    r.candidates = r.candidates.filter(|c| type_of(c) == "map" || exts.some(|e| c.ends_with(e)));
    r
}

// The files in the zip archive of an unzip command line (its first
// argument), one directory at a time.
fn zip_members(cur, words) {
    let archive = ();
    for w in words.extract(1, words.len() - 2) {
        if !w.starts_with("-") {
            archive = w;
            break;
        }
    }
    if archive == () {
        return ();
    }
    archive = kinds::tilde(archive);
    if !fs::is_file(archive) && fs::is_file(archive + ".zip") {
        archive += ".zip";
    }
    let r = sh::capture(`unzip -Z1 ${sh::quote(archive)} 2>/dev/null`);
    if r.status != 0 {
        return ();
    }
    let out = [];
    let seen = #{};
    for m in r.out.split("\n") {
        if !m.starts_with(cur) || m == cur {
            continue;
        }
        let slash = m.sub_string(cur.len()).index_of("/");
        let v = if slash < 0 || cur.len() + slash + 1 == m.len() { m } else { m.sub_string(0, cur.len() + slash + 1) };
        if !seen.contains(v) {
            seen[v] = true;
            out.push(if v.ends_with("/") { #{value: v, suffix: ""} } else { v });
        }
    }
    out
}

// The file systems the kernel knows.
fn fstypes() {
    let out = [];
    for line in (fs::read_file("/proc/filesystems") ?? "").split("\n") {
        let w = line.split();
        if !w.is_empty() {
            out.push(w[-1]);
        }
    }
    out
}

// ripgrep's file types.
fn rg_types() {
    let r = sh::capture("rg --type-list 2>/dev/null");
    let out = [];
    for line in r.out.split("\n") {
        let c = line.index_of(":");
        if c > 0 {
            let desc = line.sub_string(c + 1);
            desc.trim();
            out.push(#{value: line.sub_string(0, c), desc: desc});
        }
    }
    out
}

fn kind(name, cur, words) {
    switch name {
        "gz" => with_ext(cur, [".gz", ".tgz", ".z", ".Z"]),
        "xz" => with_ext(cur, [".xz", ".txz", ".lzma", ".tlz"]),
        "zst" => with_ext(cur, [".zst", ".tzst"]),
        "bz2" => with_ext(cur, [".bz2", ".tbz", ".tbz2"]),
        "zip" => with_ext(cur, [".zip", ".jar", ".whl", ".ZIP"]),
        "zip_members" => zip_members(cur, words),
        "fstypes" => fstypes(),
        "rg_types" => rg_types(),
        _ => throw `unknown kind: ${name}`,
    }
}

fn find() {
    #{
        single_dash: true,
        opts: `
            -H  follow symbolic links on the command line only
            -L  follow all symbolic links
            -P  never follow symbolic links (the default)
            -D DEBUGOPTS  print diagnostics
            -daystart  measure times from the start of today
            -follow  follow symbolic links (like -L)
            -regextype TYPE  the syntax of the regular expressions of -regex
            -warn  print warnings about the command line
            -nowarn  don't print warnings about the command line
            -depth  process a directory's contents before the directory
            -files0-from FILE  start at the NUL-separated paths in FILE
            -maxdepth LEVELS  descend at most LEVELS levels
            -mindepth LEVELS  ignore the first LEVELS levels
            -mount  don't descend into other file systems
            -xdev  don't descend into other file systems
            -noleaf  don't assume that directories have 2 fewer subdirectories than links
            -ignore_readdir_race  don't report files that vanish
            -noignore_readdir_race  report files that vanish
            -amin N  last accessed N minutes ago
            -anewer FILE  last accessed after FILE was modified
            -atime N  last accessed N days ago
            -cmin N  status last changed N minutes ago
            -cnewer FILE  status last changed after FILE was modified
            -ctime N  status last changed N days ago
            -context CONTEXT  SELinux context matches CONTEXT
            -empty  empty file or directory
            -executable  executable file, or searchable directory
            -false  always false
            -fstype TYPE  on a file system of type TYPE
            -gid N  numeric group ID is N
            -group NAME  belongs to the group NAME
            -ilname PATTERN  like -lname, ignoring case
            -iname PATTERN  like -name, ignoring case
            -inum N  inode number is N
            -ipath PATTERN  like -path, ignoring case
            -iregex PATTERN  like -regex, ignoring case
            -iwholename PATTERN  like -wholename, ignoring case
            -links N  has N hard links
            -lname PATTERN  symbolic link whose target matches PATTERN
            -mmin N  last modified N minutes ago
            -mtime N  last modified N days ago
            -name PATTERN  base name matches the shell pattern PATTERN
            -newer FILE  last modified after FILE
            -nogroup  no group matches its group ID
            -nouser  no user matches its user ID
            -path PATTERN  path matches the shell pattern PATTERN
            -perm MODE  permission bits are MODE (-MODE: all of them, /MODE: any)
            -readable  readable
            -regex PATTERN  path matches the regular expression PATTERN
            -samefile FILE  same inode as FILE
            -size N  uses N units of space (c, k, M, G)
            -true  always true
            -type TYPE  of type TYPE (f, d, l ...)
            -uid N  numeric user ID is N
            -used N  last accessed N days after its status last changed
            -user NAME  owned by the user NAME
            -wholename PATTERN  like -path
            -writable  writable
            -xtype TYPE  like -type, for the target of a symbolic link
            -delete  delete it
            -exec COMMAND  run COMMAND (ending with ; or {} +)
            -execdir COMMAND  run COMMAND in its directory (ending with ; or {} +)
            -ok COMMAND  run COMMAND after asking (ending with ;)
            -okdir COMMAND  run COMMAND in its directory after asking (ending with ;)
            -fls FILE  list it in the ls -dils format to FILE
            -fprint FILE  print its path to FILE
            -fprint0 FILE  print its path to FILE, ending with NUL
            -fprintf FILE  print it in a format to FILE
            -ls  list it in the ls -dils format
            -print  print its path
            -print0  print its path, ending with NUL
            -printf FORMAT  print it in FORMAT
            -prune  don't descend into it
            -quit  exit at once
            -not  negate the next expression
            -a, -and  both expressions
            -o, -or  either expression
            --help  display the help and exit
            --version  output version information and exit
        `,
        values: #{
            "-D": ["exec", "opt", "rates", "search", "stat", "time", "tree", "all", "help"],
            "-regextype": [
                "findutils-default", "ed", "emacs", "gnu-awk", "grep", "posix-awk", "awk", "posix-basic",
                "posix-egrep", "egrep", "posix-extended", "posix-minimal-basic", "sed",
            ],
            "-type": [
                #{value: "f", desc: "regular file"},
                #{value: "d", desc: "directory"},
                #{value: "l", desc: "symbolic link"},
                #{value: "p", desc: "named pipe (FIFO)"},
                #{value: "s", desc: "socket"},
                #{value: "b", desc: "block device"},
                #{value: "c", desc: "character device"},
                #{value: "D", desc: "door (Solaris)"},
            ],
            "-fstype": "tools:fstypes",
            "-group": "groups",
            "-user": "users",
            "-perm": "mode",
            "-exec": "commands",
            "-execdir": "commands",
            "-ok": "commands",
            "-okdir": "commands",
            "-amin": "none",
            "-atime": "none",
            "-cmin": "none",
            "-ctime": "none",
            "-context": "none",
            "-gid": "none",
            "-ilname": "none",
            "-iname": "none",
            "-inum": "none",
            "-ipath": "none",
            "-iregex": "none",
            "-iwholename": "none",
            "-links": "none",
            "-lname": "none",
            "-maxdepth": "none",
            "-mindepth": "none",
            "-mmin": "none",
            "-mtime": "none",
            "-name": "none",
            "-path": "none",
            "-printf": "none",
            "-regex": "none",
            "-size": "none",
            "-uid": "none",
            "-used": "none",
            "-wholename": "none",
        },
        args: ["dirs"],
    }
}

fn fd() {
    #{
        opts: `
            -H, --hidden  search hidden files and directories
            -I, --no-ignore  do not respect .(git|fd)ignore files
            -u, --unrestricted  search hidden and ignored files
            --no-ignore-vcs  do not respect .gitignore files
            --no-ignore-parent  do not respect ignore files in parent directories
            -s, --case-sensitive  case-sensitive search (default: smart case)
            -i, --ignore-case  case-insensitive search (default: smart case)
            -g, --glob  glob-based search (default: regular expression)
            --regex  regular-expression based search (the default)
            -F, --fixed-strings  treat the pattern as a literal string
            --and <pattern>  additional required search patterns
            -a, --absolute-path  show absolute instead of relative paths
            -l, --list-details  use a long listing format with file metadata
            -L, --follow  follow symbolic links
            -p, --full-path  search full abs. path (default: filename only)
            -0, --print0  separate search results by the null character
            -d, --max-depth <depth>  set maximum search depth (default: none)
            --min-depth <depth>  only show search results starting at the given depth
            --exact-depth <depth>  only show search results at the exact given depth
            -E, --exclude <pattern>  exclude entries that match the given glob pattern
            --prune  do not traverse into directories that match the search criteria
            -t, --type <filetype>  filter by type
            -e, --extension <ext>  filter by file extension
            -S, --size <size>  limit results based on the size of files
            --changed-within <date|dur>  filter by file modification time (newer than)
            --changed-before <date|dur>  filter by file modification time (older than)
            -o, --owner <user:group>  filter by owning user and/or group
            -x, --exec <cmd>  execute a command for each search result
            -X, --exec-batch <cmd>  execute a command with all search results at once
            --batch-size <size>  max number of arguments to run as a batch with -X
            --ignore-file <path>  add a custom ignore-file in '.gitignore' format
            -c, --color <when>  when to use colors
            -j, --threads <num>  set number of threads to use
            --max-results <count>  limit the number of search results
            -1  limit search to a single result
            -q, --quiet  print nothing, exit code 0 if match found, 1 otherwise
            --show-errors  show filesystem errors
            --base-directory <path>  change the current working directory
            --path-separator <separator>  set path separator when printing file paths
            --search-path <search-path>  provide paths to search
            --strip-cwd-prefix  strip ./ from the paths printed
            --one-file-system  do not descend into other file systems
            -h, --help  print help
            -V, --version  print version
        `,
        values: #{
            "-t": [
                #{value: "f", desc: "file"},
                #{value: "d", desc: "directory"},
                #{value: "l", desc: "symbolic link"},
                #{value: "x", desc: "executable"},
                #{value: "e", desc: "empty"},
                #{value: "s", desc: "socket"},
                #{value: "p", desc: "pipe"},
                #{value: "b", desc: "block device"},
                #{value: "c", desc: "character device"},
            ],
            "-c": when(),
            "-o": "owner",
            "-x": "commands",
            "-X": "commands",
            "--base-directory": "dirs",
            "--search-path": "dirs",
            "--and": "none",
            "-d": "none",
            "--min-depth": "none",
            "--exact-depth": "none",
            "-E": "none",
            "-e": "none",
            "-S": "none",
            "--changed-within": "none",
            "--changed-before": "none",
            "--batch-size": "none",
            "-j": "none",
            "--max-results": "none",
            "--path-separator": "none",
        },
        args: ["none", "dirs"],
    }
}

fn locate() {
    #{
        opts: `
            -A, --all  print only names that match all the patterns
            -b, --basename  match only the base name of path names
            -c, --count  print the number of matches instead of the matches
            -d, --database DBPATH  search the database DBPATH
            -e, --existing  print only entries that exist
            -i, --ignore-case  ignore case
            -l, --limit LIMIT  stop after LIMIT matches
            -0, --null  end the matches with NUL instead of newline
            -N, --literal  do not quote the file names
            -r, --regexp REGEXP  search for a basic regular expression
            --regex  take the patterns as extended regular expressions
            -S, --statistics  print statistics about each database
            -w, --wholename  match the whole path name (the default)
            -h, --help  print the help and exit
            -V, --version  print version information and exit
        `,
        values: #{"-l": "none", "-r": "none"},
        args: ["none"],
    }
}

fn sed() {
    #{
        opts: `
            -n, --quiet, --silent  suppress automatic printing of pattern space
            --debug  annotate program execution
            -e SCRIPT, --expression=SCRIPT  add the script to the commands to be executed
            -f SCRIPT-FILE, --file=SCRIPT-FILE  add the contents of script-file to the commands
            --follow-symlinks  follow symlinks when processing in place
            -i, --in-place[=SUFFIX]  edit files in place (makes backup if SUFFIX supplied)
            -l N, --line-length=N  specify the desired line-wrap length for the l command
            --posix  disable all GNU extensions
            -E, -r, --regexp-extended  use extended regular expressions in the script
            -s, --separate  consider files as separate rather than as a single stream
            --sandbox  operate in sandbox mode (disable e/r/w commands)
            -u, --unbuffered  load minimal amounts of data and flush output often
            -z, --null-data  separate lines by NUL characters
            --help  display this help and exit
            --version  output version information and exit
        `,
        values: #{"-e": "none", "-l": "none"},
        args: ["none", "files"],
        args_if: #{"-e": ["files"], "-f": ["files"]},
    }
}

fn awk_opts() {
    `
        -f PROGFILE  read the program from PROGFILE
        -F FS  use FS as the field separator
        -v VAR=VAL  assign VAL to VAR before the program runs
    `
}

fn awk() {
    #{opts: awk_opts(), values: #{"-F": "none", "-v": "none"}, args: ["none", "files"], args_if: #{"-f": ["files"]}}
}

fn mawk() {
    #{
        opts: awk_opts() + `
            -W OPT  a mawk option (version, dump, interactive, exec, sprintf, posix_space, usage)
            --version  show the version and exit
        `,
        values: #{
            "-F": "none", "-v": "none",
            "-W": ["version", "dump", "interactive", "exec", "sprintf", "posix_space", "usage"],
        },
        args: ["none", "files"],
        args_if: #{"-f": ["files"]},
    }
}

fn gawk() {
    #{
        opts: `
            -f PROGFILE, --file=PROGFILE  read the program from PROGFILE
            -F FS, --field-separator=FS  use FS as the field separator
            -v VAR=VAL, --assign=VAR=VAL  assign VAL to VAR before the program runs
            -b, --characters-as-bytes  treat all input data as single-byte characters
            -c, --traditional  run in compatibility mode (no GNU extensions)
            -C, --copyright  print the copyright notice
            -d, --dump-variables[=FILE]  print the global variables to FILE
            -D, --debug[=FILE]  run the debugger
            -e PROGRAM-TEXT, --source=PROGRAM-TEXT  use PROGRAM-TEXT as the program
            -E FILE, --exec=FILE  read the program from FILE, and stop processing options
            -g, --gen-pot  print a .pot file of the translatable strings
            -h, --help  print the help and exit
            -i INCLUDEFILE, --include=INCLUDEFILE  load the source file INCLUDEFILE
            -I, --trace  print the internal byte codes as they run
            -l LIBRARY, --load=LIBRARY  load the shared library LIBRARY
            -L, --lint[=fatal|invalid|no-ext]  warn about dubious or non-portable constructs
            -M, --bignum  use arbitrary precision arithmetic
            -N, --use-lc-numeric  use the locale's decimal point in input
            -n, --non-decimal-data  allow octal and hexadecimal values in input
            -o, --pretty-print[=FILE]  print a pretty-printed program to FILE
            -O, --optimize  optimize the program (the default)
            -p, --profile[=FILE]  write profiling data to FILE
            -P, --posix  follow POSIX strictly
            -r, --re-interval  allow interval expressions in regular expressions
            -s, --no-optimize  don't optimize the program
            -S, --sandbox  disable system(), redirections and extensions
            -t, --lint-old  warn about constructs not in the original awk
            -V, --version  print the version and exit
        `,
        values: #{"-F": "none", "-v": "none", "-e": "none", "-l": "none", "--lint": ["fatal", "invalid", "no-ext"]},
        args: ["none", "files"],
        args_if: #{"-f": ["files"], "-e": ["files"], "-E": ["files"]},
    }
}

fn jq() {
    #{
        opts: `
            -n, --null-input  use null as the single input value
            -R, --raw-input  read each line as string instead of JSON
            -s, --slurp  read all inputs into an array
            -c, --compact-output  compact instead of pretty-printed output
            -r, --raw-output  output strings without escapes and quotes
            --raw-output0  like -r, with NUL after each output
            -j, --join-output  like -r, without a newline after each output
            -a, --ascii-output  output strings with only ASCII characters
            -S, --sort-keys  sort the keys of each object on output
            -C, --color-output  colorize the output
            -M, --monochrome-output  disable colored output
            --tab  use tabs for indentation
            --indent n  use n spaces for indentation (max 7 spaces)
            --unbuffered  flush the output after each output
            --stream  parse the input in streaming fashion
            --stream-errors  like --stream, and report parse errors as values
            --seq  parse input and output as application/json-seq
            -f, --from-file FILE  load the filter from FILE
            -L DIRECTORY  search for modules in DIRECTORY
            --arg NAME  set $NAME to the string that follows
            --argjson NAME  set $NAME to the JSON value that follows
            --slurpfile NAME  set $NAME to an array of the JSON values of the file that follows
            --rawfile NAME  set $NAME to the contents of the file that follows
            --args  take the remaining arguments as positional string arguments
            --jsonargs  take the remaining arguments as positional JSON arguments
            -e, --exit-status  set the exit status from the output
            -V, --version  show the version
            --build-configuration  show jq's build configuration
            -h, --help  show the help
        `,
        values: #{
            "--indent": "none", "-L": "dirs", "--arg": "none", "--argjson": "none", "--slurpfile": "none",
            "--rawfile": "none",
        },
        args: ["none", "files"],
        args_if: #{"-f": ["files"]},
    }
}

fn rg() {
    #{
        opts: `
            -e, --regexp=PATTERN  a pattern to search for
            -f, --file=PATTERNFILE  search for the patterns in the given file
            --pre=COMMAND  search the output of COMMAND for each PATH
            --pre-glob=GLOB  include or exclude files from a preprocessor
            -z, --search-zip  search in compressed files
            -s, --case-sensitive  search case sensitively (the default)
            --crlf  use CRLF line terminators
            --dfa-size-limit=NUM  the upper size limit of the regex DFA
            -E, --encoding=ENCODING  the text encoding of the files
            --engine=ENGINE  the regex engine to use
            -F, --fixed-strings  treat all patterns as literals
            -i, --ignore-case  case insensitive search
            -v, --invert-match  invert matching
            -x, --line-regexp  show matches surrounded by line boundaries
            -m, --max-count=NUM  limit the number of matching lines
            --mmap  search with memory maps when possible
            -U, --multiline  enable searching across multiple lines
            --multiline-dotall  make . match line terminators
            --no-unicode  disable Unicode mode
            --null-data  use NUL as a line terminator
            -P, --pcre2  enable PCRE2 matching
            --regex-size-limit=NUM  the size limit of the compiled regex
            -S, --smart-case  smart case search
            --stop-on-nonmatch  stop searching after a non-match
            -a, --text  search binary files as if they were text
            -j, --threads=NUM  the approximate number of threads to use
            -w, --word-regexp  show matches surrounded by word boundaries
            --binary  search binary files
            -L, --follow  follow symbolic links
            -g, --glob=GLOB  include or exclude file paths
            --glob-case-insensitive  process all glob patterns case insensitively
            -., --hidden  search hidden files and directories
            --iglob=GLOB  include or exclude paths case insensitively
            --ignore-file=PATH  additional ignore files
            --ignore-file-case-insensitive  process ignore files case insensitively
            -d, --max-depth=NUM  descend at most NUM directories
            --max-filesize=NUM  ignore files larger than NUM in size
            --no-ignore  don't use ignore files
            --no-ignore-dot  don't use .ignore or .rgignore files
            --no-ignore-exclude  don't use local exclusion files
            --no-ignore-files  don't use --ignore-file arguments
            --no-ignore-global  don't use global ignore files
            --no-ignore-parent  don't use ignore files in parent directories
            --no-ignore-vcs  don't use ignore files from source control
            --no-require-git  use .gitignore outside of git repositories
            --one-file-system  skip directories on other file systems
            -t, --type=TYPE  only search files matching TYPE
            -T, --type-not=TYPE  do not search files matching TYPE
            --type-add=TYPESPEC  add a new glob for a file type
            --type-clear=TYPE  clear the globs of a file type
            -u, --unrestricted  reduce the level of smart filtering
            -A, --after-context=NUM  show NUM lines after each match
            -B, --before-context=NUM  show NUM lines before each match
            --block-buffered  force block buffering
            -b, --byte-offset  print the byte offset of each matching line
            --color=WHEN  when to use color
            --colors=COLOR_SPEC  configure color settings and styles
            --column  show column numbers
            -C, --context=NUM  show NUM lines before and after each match
            --context-separator=SEP  the separator of contextual chunks
            --field-context-separator=SEP  the field context separator
            --field-match-separator=SEP  the field match separator
            --heading  print matches grouped by file
            -h, --help  show the help
            --hostname-bin=COMMAND  a program that gives this system's hostname
            --hyperlink-format=FORMAT  the format of hyperlinks
            --include-zero  include zero matches in summary output
            --line-buffered  force line buffering
            -n, --line-number  show line numbers
            -N, --no-line-number  suppress line numbers
            -M, --max-columns=NUM  omit lines longer than this limit
            --max-columns-preview  show a preview of lines exceeding the limit
            -0, --null  print a NUL byte after file paths
            -o, --only-matching  print only the matched parts of a line
            --path-separator=SEP  the path separator for printing paths
            --passthru  print both matching and non-matching lines
            -p, --pretty  colors, headings and line numbers
            -q, --quiet  do not print anything to stdout
            -r, --replace=TEXT  replace matches with the given text
            --sort=SORTBY  sort results in ascending order
            --sortr=SORTBY  sort results in descending order
            --trim  trim leading whitespace from matches
            --vimgrep  print results in a vim compatible format
            -H, --with-filename  print the file path with each matching line
            -I, --no-filename  never print the path with each matching line
            -c, --count  show the count of matching lines for each file
            --count-matches  show the count of every match for each file
            -l, --files-with-matches  print the paths with at least one match
            --files-without-match  print the paths that contain zero matches
            --json  show search results in the JSON Lines format
            --debug  show debug messages
            --no-ignore-messages  suppress gitignore parse error messages
            --no-messages  suppress some error messages
            --stats  print statistics about the search
            --trace  show trace messages
            --files  print each file that would be searched
            --generate=KIND  generate man pages and completion scripts
            --no-config  never read configuration files
            --pcre2-version  print the version of PCRE2 that ripgrep uses
            --type-list  show all supported file types
            -V, --version  print ripgrep's version
        `,
        values: #{
            "-e": "none", "--pre": "commands", "--pre-glob": "none", "--dfa-size-limit": "none", "-E": "none",
            "--engine": ["default", "pcre2", "auto"], "-m": "none", "--regex-size-limit": "none", "-j": "none",
            "-g": "none", "--iglob": "none", "-d": "none", "--max-filesize": "none", "-t": "tools:rg_types",
            "-T": "tools:rg_types", "--type-add": "none", "--type-clear": "tools:rg_types", "-A": "none",
            "-B": "none", "--color": ["never", "auto", "always", "ansi"], "--colors": "none", "-C": "none",
            "--context-separator": "none", "--field-context-separator": "none", "--field-match-separator": "none",
            "--hostname-bin": "commands", "--hyperlink-format": "none", "-M": "none", "--path-separator": "none",
            "-r": "none", "--sort": ["none", "path", "modified", "accessed", "created"],
            "--sortr": ["none", "path", "modified", "accessed", "created"],
            "--generate": ["man", "complete-bash", "complete-zsh", "complete-fish", "complete-powershell"],
        },
        args: ["none", "files"],
        args_if: #{"-e": ["files"], "-f": ["files"], "--files": ["files"]},
    }
}

fn less() {
    #{
        opts: `
            -?, --help  display the help
            -a, --search-skip-screen  start searches after the last line displayed
            -A, --SEARCH-SKIP-SCREEN  start searches after the target line
            -b, --buffers=N  use N buffers of memory for each file
            -B, --auto-buffers  don't allocate buffers automatically for pipes
            -c, --clear-screen  repaint by clearing the screen
            -d, --dumb  suppress the error message for a dumb terminal
            -D, --color=COLOR  set the color of the text
            -e, --quit-at-eof  exit the second time the end of the file is reached
            -E, --QUIT-AT-EOF  exit the first time the end of the file is reached
            -f, --force  open non-regular files
            -F, --quit-if-one-screen  exit if the whole file fits on the screen
            -g, --hilite-search  highlight only the last match of a search
            -G, --HILITE-SEARCH  highlight no matches of searches
            -h, --max-back-scroll=N  scroll backward at most N lines
            -i, --ignore-case  ignore case in searches without uppercase letters
            -I, --IGNORE-CASE  ignore case in all searches
            -j, --jump-target=N  put the target line on screen line N
            -J, --status-column  show a status column
            -k, --lesskey-file=FILE  use FILE as a lesskey file
            -K, --quit-on-intr  exit on Ctrl-C
            -L, --no-lessopen  ignore LESSOPEN
            -m, --long-prompt  a prompt like more's
            -M, --LONG-PROMPT  an even longer prompt
            -n, --line-numbers  don't compute line numbers
            -N, --LINE-NUMBERS  show line numbers
            -o, --log-file=FILE  copy the input to FILE
            -O, --LOG-FILE=FILE  copy the input to FILE, overwriting it
            -p, --pattern=PATTERN  start at the first match of PATTERN
            -P, --prompt=PROMPT  set the prompt
            -q, --quiet, --silent  ring the bell less
            -Q, --QUIET, --SILENT  never ring the bell
            -r, --raw-control-chars  show control characters as they are
            -R, --RAW-CONTROL-CHARS  show ANSI colors and hyperlinks as they are
            -s, --squeeze-blank-lines  squeeze blank lines into one
            -S, --chop-long-lines  chop long lines rather than wrapping them
            -t, --tag=TAG  edit the file that has the tag TAG
            -u, --underline-special  show backspaces and carriage returns as they are
            -U, --UNDERLINE-SPECIAL  show backspaces, tabs and carriage returns as control characters
            -V, --version  show the version
            -w, --hilite-unread  highlight the first new line after moving by a page
            -W, --HILITE-UNREAD  highlight the first new line after any forward movement
            -x, --tabs=N  set tab stops every N columns
            -X, --no-init  don't clear the screen when starting and exiting
            -y, --max-forw-scroll=N  scroll forward at most N lines
            -z, --window=N  scroll N lines at a time
            -#, --shift=N  scroll N columns horizontally
            -~, --tilde  don't show ~ after the end of the file
            --follow-name  follow the file by name with F
            --incsearch  search as the pattern is typed
            --mouse  scroll with the mouse
            --no-histdups  don't save duplicate commands in the history
            --no-keypad  don't send the keypad initialization
            --redraw-on-quit  leave the last screen on the terminal on exit
            --save-marks  save marks in the history file
            --use-color  use colors
            --wordwrap  wrap lines at spaces
        `,
        values: #{
            "-b": "none", "-D": "none", "-h": "none", "-j": "none", "-p": "none", "-P": "none", "-t": "none",
            "-x": "none", "-y": "none", "-z": "none", "-#": "none",
        },
    }
}

fn more() {
    #{
        opts: `
            -d, --silent  display help instead of ringing bell
            -f, --logical  count logical rather than screen lines
            -l, --no-pause  suppress pause after form feed
            -c, --print-over  do not scroll, display text and clean line ends
            -p, --clean-print  do not scroll, clean screen and display text
            -e, --exit-on-eof  exit on end-of-file
            -s, --squeeze  squeeze multiple blank lines into one
            -u, --plain  suppress underlining and bold
            -n, --lines NUMBER  the number of lines per screenful
            -h, --help  display this help
            -V, --version  display version
        `,
        values: #{"-n": "none"},
    }
}

// The tests of file, for --exclude.
fn file_tests() {
    ["apptype", "ascii", "cdf", "compress", "csv", "elf", "encoding", "json", "soft", "tar", "text", "tokens"]
}

fn file() {
    #{
        opts: `
            --help  display this help and exit
            -v, --version  output version information and exit
            -m, --magic-file LIST  use LIST as a colon-separated list of magic number files
            -z, --uncompress  try to look inside compressed files
            -Z, --uncompress-noreport  only print the contents of compressed files
            -b, --brief  do not prepend filenames to output lines
            -c, --checking-printout  print the parsed form of the magic file
            -e, --exclude TEST  exclude TEST from the tests made
            --exclude-quiet TEST  like --exclude, but ignore unknown tests
            -f, --files-from FILE  read the filenames to be examined from FILE
            -F, --separator STRING  use STRING as separator instead of :
            -i, --mime  output MIME type strings
            --apple  output the Apple CREATOR/TYPE
            --extension  output a slash-separated list of extensions
            --mime-type  output the MIME type
            --mime-encoding  output the MIME encoding
            -k, --keep-going  don't stop at the first match
            -l, --list  list magic strength
            -L, --dereference  follow symlinks
            -h, --no-dereference  don't follow symlinks
            -n, --no-buffer  do not buffer output
            -N, --no-pad  do not pad output
            -0, --print0  terminate filenames with ASCII NUL
            -p, --preserve-date  preserve access times on files
            -P, --parameter NAME=VALUE  set file engine parameter limits
            -r, --raw  don't translate unprintable chars to \ooo
            -s, --special-files  treat special (block/char devices) files as ordinary ones
            -S, --no-sandbox  disable system call sandboxing
            -C, --compile  compile file specified by -m
            -d, --debug  print debugging messages
        `,
        values: #{
            "-e": file_tests(),
            "--exclude-quiet": file_tests(),
            "-F": "none",
            "-P": "none",
        },
    }
}

fn ldd() {
    #{
        opts: `
            --help  print this help and exit
            --version  print version information and exit
            -d, --data-relocs  process data relocations
            -r, --function-relocs  process data and function relocations
            -u, --unused  print unused direct dependencies
            -v, --verbose  print all information
        `,
    }
}

fn patch() {
    #{
        opts: `
            -p NUM, --strip=NUM  strip NUM leading components from file names
            -F LINES, --fuzz=LINES  set the fuzz factor to LINES for inexact matching
            -l, --ignore-whitespace  ignore white space changes between patch and input
            -c, --context  interpret the patch as a context difference
            -e, --ed  interpret the patch as an ed script
            -n, --normal  interpret the patch as a normal difference
            -u, --unified  interpret the patch as a unified difference
            -N, --forward  ignore patches that appear to be reversed or already applied
            -R, --reverse  assume patches were created with old and new files swapped
            -i PATCHFILE, --input=PATCHFILE  read the patch from PATCHFILE instead of stdin
            -o FILE, --output=FILE  output patched files to FILE
            -r FILE, --reject-file=FILE  output rejects to FILE
            -D NAME, --ifdef=NAME  make merged if-then-else output using NAME
            --merge[=merge|diff3]  merge using conflict markers instead of creating reject files
            -E, --remove-empty-files  remove output files that are empty after patching
            -Z, --set-utc  set times of patched files, assuming diff uses UTC
            -T, --set-time  set times of patched files, assuming local time
            --quoting-style=WORD  output file names using quoting style WORD
            -b, --backup  back up the original contents of each file
            --backup-if-mismatch  back up if the patch does not match exactly
            --no-backup-if-mismatch  back up mismatches only if otherwise requested
            -V STYLE, --version-control=STYLE  use STYLE version control
            -B PREFIX, --prefix=PREFIX  prepend PREFIX to backup file names
            -Y PREFIX, --basename-prefix=PREFIX  prepend PREFIX to backup file basenames
            -z SUFFIX, --suffix=SUFFIX  append SUFFIX to backup file names
            -g NUM, --get=NUM  get files from RCS etc
            -t, --batch  ask no questions; skip bad-Prereq patches; assume reversed
            -f, --force  like -t, but ignore bad-Prereq patches, and assume unreversed
            -s, --quiet, --silent  work silently unless an error occurs
            --verbose  output extra information about the work being done
            --dry-run  do not actually change any files; just print what would happen
            --posix  conform to the POSIX standard
            -d DIR, --directory=DIR  change the working directory to DIR first
            --reject-format=FORMAT  create context or unified rejects
            --binary  read and write data in binary mode
            --read-only=BEHAVIOR  how to handle read-only input files
            -v, --version  output version info
            --help  output this help
        `,
        values: #{
            "-p": "none", "-F": "none", "-D": "none", "--merge": ["merge", "diff3"],
            "--quoting-style": ["literal", "shell", "shell-always", "c"],
            "-V": ["simple", "numbered", "existing"], "-B": "none", "-Y": "none", "-z": "none", "-g": "none",
            "-d": "dirs", "--reject-format": ["context", "unified"], "--read-only": ["ignore", "warn", "fail"],
        },
    }
}

fn tree() {
    #{
        opts: `
            -a  list all files, also those starting with .
            -d  list directories only
            -l  follow symbolic links to directories
            -f  print the full path prefix of each file
            -x  stay on the current file system
            -L LEVEL  descend at most LEVEL directories
            -R  rerun tree at the maximum depth
            -P PATTERN  list only the files that match PATTERN
            -I PATTERN  do not list the files that match PATTERN
            --gitignore  filter by the .gitignore files
            --ignore-case  ignore case when matching patterns
            --matchdirs  match directory names too with -P
            --metafirst  print the meta-data at the beginning of each line
            --prune  prune empty directories from the output
            --info  print the information in .info files
            --noreport  don't print the file and directory counts at the end
            --charset=CHARSET  use CHARSET for the output
            --filelimit=N  do not descend into directories with more than N entries
            -o FILENAME  write the output to FILENAME
            -q  print non-printable characters as ?
            -N  print non-printable characters as they are
            -Q  quote file names in double quotes
            -p  print the protections of each file
            -u  print the file owner or UID number
            -g  print the file group owner or GID number
            -s  print the size of each file in bytes
            -h  print the sizes in a more human readable way
            --si  like -h, with powers of 1000
            --du  print directory sizes as the sums of their contents
            -D  print the date of last modification
            --timefmt=FORMAT  print and format the time with FORMAT
            -F  append /, =, *, @, | or > as ls -F does
            --inodes  print the inode number of each file
            --device  print the device ID of each file
            -v  sort files by version
            -t  sort files by modification time
            -c  sort files by status change time
            -U  leave files unsorted
            -r  sort in reverse order
            --dirsfirst  list directories before files
            --filesfirst  list files before directories
            --sort=TYPE  sort files by TYPE
            -i  don't print the indentation lines
            -A  print ANSI line graphics indentation lines
            -S  print with CP437 line graphics
            -n  turn colorization off
            -C  turn colorization on
            -X  print an XML representation of the tree
            -J  print a JSON representation of the tree
            -H BASEHREF  print HTML output with BASEHREF as the top directory
            -T TITLE  replace the default HTML title with TITLE
            --nolinks  turn off hyperlinks in HTML output
            --fromfile  read paths from files
            --version  print the version and exit
            --help  print the help and exit
        `,
        values: #{
            "-L": "none", "-P": "none", "-I": "none", "--charset": "none", "--filelimit": "none",
            "--timefmt": "none", "--sort": ["name", "version", "size", "mtime", "ctime"], "-H": "none",
            "-T": "none",
        },
        args: ["dirs"],
    }
}

fn gzip(ext) {
    #{
        opts: `
            -c, --stdout  write on standard output, keep original files unchanged
            -d, --decompress  decompress
            -f, --force  force overwrite of output file and compress links
            -h, --help  give this help
            -k, --keep  keep (don't delete) input files
            -l, --list  list compressed file contents
            -L, --license  display software license
            -n, --no-name  do not save or restore the original name and timestamp
            -N, --name  save or restore the original name and timestamp
            -q, --quiet  suppress all warnings
            -r, --recursive  operate recursively on directories
            --rsyncable  make rsync-friendly archive
            -S, --suffix=SUF  use suffix SUF on compressed files
            --synchronous  synchronous output (safer if system crashes, but slower)
            -t, --test  test compressed file integrity
            -v, --verbose  verbose mode
            -V, --version  display version number
            -1, --fast  compress faster
            -9, --best  compress better
        `,
        values: #{"-S": "none"},
        args: if ext { ["tools:gz"] } else { ["files"] },
        args_if: #{"-d": ["tools:gz"], "-l": ["tools:gz"], "-t": ["tools:gz"]},
    }
}

fn xz(ext) {
    #{
        opts: `
            -z, --compress  force compression
            -d, --decompress, --uncompress  force decompression
            -t, --test  test compressed file integrity
            -l, --list  list information about .xz files
            -k, --keep  keep (don't delete) input files
            -f, --force  force overwrite of output file and (de)compress links
            -c, --stdout, --to-stdout  write to standard output and don't delete input files
            -0  compression preset 0 (fastest)
            -1  compression preset 1
            -2  compression preset 2
            -3  compression preset 3
            -4  compression preset 4
            -5  compression preset 5
            -6  compression preset 6 (the default)
            -7  compression preset 7
            -8  compression preset 8
            -9  compression preset 9 (best)
            -e, --extreme  try to improve compression ratio by using more CPU time
            -T, --threads=NUM  use at most NUM threads (0 for as many as there are cores)
            -F, --format=FORMAT  the file format to encode or decode
            -C, --check=CHECK  the type of integrity check
            -M, --memlimit=LIMIT  set the memory usage limit
            --single-stream  decompress only the first stream
            --no-sparse  do not create sparse files when decompressing
            -S, --suffix=.SUF  use the suffix .SUF on compressed files
            --files[=FILE]  read the file names to process from FILE
            --files0[=FILE]  like --files, with names ending with NUL
            -q, --quiet  suppress warnings; specify twice to suppress errors too
            -v, --verbose  be verbose; specify twice for even more verbose
            -Q, --no-warn  make warnings not affect the exit status
            --robot  use machine-parsable messages
            -h, --help  display the short help and exit
            -H, --long-help  display the long help and exit
            -V, --version  display the version number and exit
        `,
        values: #{
            "-T": "none", "-F": ["auto", "xz", "lzma", "lzip", "raw"], "-C": ["none", "crc32", "crc64", "sha256"],
            "-M": "none", "-S": "none",
        },
        args: if ext { ["tools:xz"] } else { ["files"] },
        args_if: #{"-d": ["tools:xz"], "-l": ["tools:xz"], "-t": ["tools:xz"]},
    }
}

fn zstd(ext) {
    #{
        opts: `
            -o OUTPUT  write the output to the single file OUTPUT
            -k, --keep  preserve the input files (the default)
            --rm  remove the input files after successful (de)compression
            -1  compression level 1 (fastest)
            -3  compression level 3 (the default)
            -9  compression level 9
            -19  compression level 19 (best)
            -d, --decompress, --uncompress  decompress
            -D DICT  use DICT as the dictionary
            -f, --force  disable input and output checks
            -h  display short usage and exit
            -H, --help  display full help and exit
            -V, --version  display the program version and exit
            -c, --stdout  write to standard output and keep the input files
            -v, --verbose  enable verbose output
            -q, --quiet  suppress warnings; pass twice to suppress errors
            --progress  show the progress counter
            --no-progress  hide the progress counter
            -r  operate recursively on directories
            --filelist LIST  read the list of files to operate on from LIST
            --output-dir-flat DIR  store the processed files in DIR
            --output-dir-mirror DIR  store the processed files in DIR, keeping the directory structure
            --check  add XXH64 integrity checksums during compression
            --no-check  don't add integrity checksums
            --ultra  enable levels beyond 19, up to 22
            --fast[=#]  use very fast compression levels
            --adapt  adapt the compression level to I/O conditions
            --long[=#]  enable long distance matching with window log #
            --patch-from=REF  use REF as the reference for a diff
            -T#  spawn # compression threads
            --single-thread  share a single thread for I/O and compression
            --rsyncable  compress using a rsync-friendly method
            --exclude-compressed  only compress files that are not already compressed
            --format=FORMAT  compress to the format FORMAT
            -l, --list  print information about zstd-compressed files
            -t, --test  test compressed file integrity
            --sparse  enable sparse mode
            --no-sparse  disable sparse mode
            --train  create a dictionary from a training set of files
            -b#  benchmark with compression level #
        `,
        values: #{
            "--output-dir-flat": "dirs", "--output-dir-mirror": "dirs",
            "--format": ["zstd", "gzip", "xz", "lzma", "lz4"],
        },
        args: if ext { ["tools:zst"] } else { ["files"] },
        args_if: #{"-d": ["tools:zst"], "-l": ["tools:zst"], "-t": ["tools:zst"]},
    }
}

fn bzip2(ext) {
    #{
        opts: `
            -h, --help  print the help
            -d, --decompress  force decompression
            -z, --compress  force compression
            -k, --keep  keep (don't delete) input files
            -f, --force  overwrite existing output files
            -t, --test  test compressed file integrity
            -c, --stdout  output to standard out
            -q, --quiet  suppress noncritical error messages
            -v, --verbose  be verbose (a 2nd -v gives more)
            -L, --license  display software version and license
            -V, --version  display software version and license
            -s, --small  use less memory (at most 2500k)
            -1, --fast  set block size to 100k
            -9, --best  set block size to 900k
        `,
        args: if ext { ["tools:bz2"] } else { ["files"] },
        args_if: #{"-d": ["tools:bz2"], "-t": ["tools:bz2"]},
    }
}

fn zip() {
    #{
        opts: `
            -f  freshen: only changed files
            -u  update: only changed or new files
            -d  delete entries in the zip file
            -m  move into the zip file (delete the files)
            -r  recurse into directories
            -j  junk (don't record) directory names
            -0  store only
            -1  compress faster
            -9  compress better
            -q  quiet operation
            -v  verbose operation
            -c  add one-line comments
            -z  add a zip file comment
            -@  read names from standard input
            -o  make the zip file as old as the latest entry
            -x  exclude the names that follow
            -i  include only the names that follow
            -F  fix the zip file (-FF try harder)
            -D  do not add directory entries
            -A  adjust self-extracting exe
            -J  junk the zip file prefix (unzipsfx)
            -T  test the zip file integrity
            -X  exclude extra file attributes
            -y  store symbolic links as the link instead of the referenced file
            -e  encrypt
            -n SUFFIXES  don't compress the files with these suffixes
            -P PASSWORD  use PASSWORD to encrypt
            -s SIZE  split the archive into pieces of SIZE
            -b PATH  use PATH for the temporary zip file
            -t DATE  only the files modified on or after DATE
            -h, --help  show the help
            -h2  show the extended help
            -L  show the software license
            -v  show the version (alone)
        `,
        values: #{"-n": "none", "-P": "none", "-s": "none", "-b": "dirs", "-t": "none"},
        args: ["tools:zip", "files"],
    }
}

fn unzip() {
    #{
        opts: `
            -p  extract files to a pipe, no messages
            -l  list files (short format)
            -f  freshen existing files, create none
            -t  test the compressed archive data
            -u  update files, create if necessary
            -z  display the archive comment only
            -v  list verbosely, or show version info
            -T  timestamp the archive to the latest file
            -x  exclude the files that follow
            -d EXDIR  extract files into EXDIR
            -n  never overwrite existing files
            -q  quiet mode (-qq: quieter)
            -o  overwrite files without prompting
            -a  auto-convert any text files
            -j  junk paths (do not make directories)
            -U  use escapes for all non-ASCII Unicode
            -C  match filenames case-insensitively
            -L  make (some) names lowercase
            -X  restore UID/GID info
            -V  retain VMS version numbers
            -K  keep setuid/setgid/tacky permissions
            -M  pipe through more
            -O CHARSET  the character encoding of DOS, Windows and OS/2 archives
            -I CHARSET  the character encoding of UNIX and other archives
            -P PASSWORD  use PASSWORD to decrypt
            -Z  zipinfo mode
        `,
        values: #{"-d": "dirs", "-O": "none", "-I": "none", "-P": "none"},
        args: ["tools:zip", "tools:zip_members"],
    }
}

fn spec(cmd) {
    switch cmd {
        "find" => find(),
        "fd" | "fdfind" => fd(),
        "locate" | "plocate" | "mlocate" => locate(),
        "sed" => sed(),
        "awk" | "nawk" => awk(),
        "mawk" => mawk(),
        "gawk" => gawk(),
        "jq" => jq(),
        "rg" => rg(),
        "less" => less(),
        "more" => more(),
        "file" => file(),
        "ldd" => ldd(),
        "patch" => patch(),
        "tree" => tree(),
        "gzip" => gzip(false),
        "gunzip" | "zcat" => gzip(true),
        "xz" | "lzma" => xz(false),
        "unxz" | "xzcat" | "unlzma" | "lzcat" => xz(true),
        "zstd" => zstd(false),
        "unzstd" | "zstdcat" => zstd(true),
        "bzip2" => bzip2(false),
        "bunzip2" | "bzcat" => bzip2(true),
        "zip" => zip(),
        "unzip" => unzip(),
        _ => throw `no spec for ${cmd}`,
    }
}
