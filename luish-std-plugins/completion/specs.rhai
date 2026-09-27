// The commands that the completion plugin knows, as specs for `lib.rhai`
// (see there for their fields). `spec(cmd)` builds a command's spec when it
// is needed. A command added here also goes in the list in extension.rhai.

fn when() {
    ["always", "auto", "never"]
}

fn backup() {
    [
        #{value: "none", desc: "never make backups"},
        #{value: "numbered", desc: "make numbered backups"},
        #{value: "existing", desc: "numbered if numbered backups exist, simple otherwise"},
        #{value: "simple", desc: "always make simple backups"},
    ]
}

fn update() {
    [
        #{value: "all", desc: "replace every file (the default)"},
        #{value: "none", desc: "replace no file"},
        #{value: "none-fail", desc: "replace no file, and fail if one exists"},
        #{value: "older", desc: "replace files older than the source"},
    ]
}

fn time_style() {
    ["full-iso", "long-iso", "iso", "locale", "+"]
}

fn cat() {
    #{opts: `
        -A, --show-all  equivalent to -vET
        -b, --number-nonblank  number nonempty output lines, overrides -n
        -e  equivalent to -vE
        -E, --show-ends  display $ at end of each line
        -n, --number  number all output lines
        -s, --squeeze-blank  suppress repeated empty output lines
        -t  equivalent to -vT
        -T, --show-tabs  display TAB characters as ^I
        -u  (ignored)
        -v, --show-nonprinting  use ^ and M- notation, except for LFD and TAB
        --help  display help and exit
        --version  output version information and exit
    `}
}

fn ls() {
    #{
        opts: `
            -a, --all  do not ignore entries starting with .
            -A, --almost-all  do not list implied . and ..
            --author  with -l, print the author of each file
            -b, --escape  print C-style escapes for nongraphic characters
            --block-size=SIZE  with -l, scale sizes by SIZE when printing them
            -B, --ignore-backups  do not list implied entries ending with ~
            -c  with -lt, sort by and show ctime
            -C  list entries by columns
            --color[=when()]  color the output
            -d, --directory  list directories themselves, not their contents
            -D, --dired  generate output designed for Emacs' dired mode
            -f  list all entries in directory order
            -F, --classify[=when()]  append an indicator (one of */=>@|) to entries
            --file-type  likewise, except do not append *
            --format=WORD  the layout: across, commas, horizontal, long, single-column, verbose, vertical
            --full-time  like -l --time-style=full-iso
            -g  like -l, but do not list the owner
            --group-directories-first  group directories before files
            -G, --no-group  in a long listing, don't print group names
            -h, --human-readable  with -l and -s, print sizes like 1K 234M 2G
            --si  likewise, but use powers of 1000 not 1024
            -H, --dereference-command-line  follow symbolic links listed on the command line
            --dereference-command-line-symlink-to-dir  follow symbolic links to directories on the command line
            --hide=PATTERN  do not list implied entries matching shell PATTERN
            --hyperlink[=when()]  hyperlink file names
            --indicator-style=WORD  append indicators of style WORD to entry names
            -i, --inode  print the index number of each file
            -I, --ignore=PATTERN  do not list implied entries matching shell PATTERN
            -k, --kibibytes  default to 1024-byte blocks for file system usage
            -l  use a long listing format
            -L, --dereference  show information for the file a symbolic link references
            -m  fill width with a comma separated list of entries
            -n, --numeric-uid-gid  like -l, but list numeric user and group IDs
            -N, --literal  print entry names without quoting
            -o  like -l, but do not list group information
            -p  append / indicator to directories
            -q, --hide-control-chars  print ? instead of nongraphic characters
            --show-control-chars  show nongraphic characters as-is
            -Q, --quote-name  enclose entry names in double quotes
            --quoting-style=WORD  use quoting style WORD for entry names
            -r, --reverse  reverse order while sorting
            -R, --recursive  list subdirectories recursively
            -s, --size  print the allocated size of each file, in blocks
            -S  sort by file size, largest first
            --sort=WORD  sort by WORD instead of name
            --time=WORD  select which timestamp to show or sort by
            --time-style=time_style()  the time/date format with -l
            -t  sort by time, newest first
            -T, --tabsize=COLS  assume tab stops at each COLS instead of 8
            -u  with -lt, sort by and show access time
            -U  do not sort; list entries in directory order
            -v  natural sort of (version) numbers within text
            -w, --width=COLS  set output width to COLS (0 means no limit)
            -x  list entries by lines instead of by columns
            -X  sort alphabetically by entry extension
            -Z, --context  print any security context of each file
            --zero  end each output line with NUL, not newline
            -1  list one file per line
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--color": when(),
            "--classify": when(),
            "--hyperlink": when(),
            "--format": ["across", "commas", "horizontal", "long", "single-column", "verbose", "vertical"],
            "--indicator-style": ["none", "slash", "file-type", "classify"],
            "--quoting-style": [
                "literal", "locale", "shell", "shell-always", "shell-escape", "shell-escape-always", "c", "escape",
            ],
            "--sort": ["none", "size", "time", "version", "extension", "width"],
            "--time": ["atime", "access", "use", "ctime", "status", "mtime", "modification", "birth", "creation"],
            "--time-style": time_style(),
            "--block-size": "none",
            "--hide": "none",
            "--ignore": "none",
            "--tabsize": "none",
            "--width": "none",
        },
    }
}

// The options that cp, mv and ln share.
fn copy_opts() {
    `
        --backup[=CONTROL]  make a backup of each existing destination file
        -b  like --backup but does not accept an argument
        -f, --force  do not prompt before overwriting
        -i, --interactive  prompt before overwrite
        -S, --suffix=SUFFIX  override the usual backup suffix
        -t, --target-directory=DIRECTORY  copy, move or link all SOURCE arguments into DIRECTORY
        -T, --no-target-directory  treat DEST as a normal file
        -v, --verbose  explain what is being done
        --help  display help and exit
        --version  output version information and exit
    `
}

fn copy_values() {
    #{"--backup": backup(), "--suffix": "none", "--target-directory": "dirs", "--update": update()}
}

fn cp() {
    #{
        opts: copy_opts() + `
            -a, --archive  same as -dR --preserve=all
            --attributes-only  don't copy the file data, just the attributes
            --copy-contents  copy contents of special files when recursive
            -d  same as --no-dereference --preserve=links
            --debug  explain how a file is copied (implies -v)
            -H  follow command-line symbolic links in SOURCE
            -l, --link  hard link files instead of copying
            -L, --dereference  always follow symbolic links in SOURCE
            -n, --no-clobber  do not overwrite an existing file
            -P, --no-dereference  never follow symbolic links in SOURCE
            -p  same as --preserve=mode,ownership,timestamps
            --preserve[=ATTR_LIST]  preserve the specified attributes
            --no-preserve=ATTR_LIST  don't preserve the specified attributes
            --parents  use full source file name under DIRECTORY
            -R, -r, --recursive  copy directories recursively
            --reflink[=when()]  control clone/CoW copies
            --remove-destination  remove each existing destination file before copying
            --sparse=when()  control creation of sparse files
            --strip-trailing-slashes  remove any trailing slashes from each SOURCE
            -s, --symbolic-link  make symbolic links instead of copying
            --update[=update()]  control which existing files are updated
            -u  equivalent to --update=older
            -x, --one-file-system  stay on this file system
            -Z  set SELinux security context of destination to the default
            --context[=CTX]  like -Z, or set the SELinux or SMACK context to CTX
        `,
        values: copy_values() + #{
            "--preserve": ["mode", "ownership", "timestamps", "context", "links", "xattr", "all"],
            "--no-preserve": ["mode", "ownership", "timestamps", "context", "links", "xattr", "all"],
            "--reflink": ["always", "auto", "never"],
            "--sparse": ["always", "auto", "never"],
            "--context": "none",
        },
    }
}

fn mv() {
    #{
        opts: copy_opts() + `
            --debug  explain how a file is copied (implies -v)
            -n, --no-clobber  do not overwrite an existing file
            --no-copy  do not copy if renaming fails
            --strip-trailing-slashes  remove any trailing slashes from each SOURCE
            --update[=update()]  control which existing files are updated
            -u  equivalent to --update=older
            -Z, --context  set SELinux security context of destination to the default
        `,
        values: copy_values(),
    }
}

fn ln() {
    #{
        opts: copy_opts() + `
            -d, -F, --directory  allow the superuser to attempt to hard link directories
            -L, --logical  dereference TARGETs that are symbolic links
            -n, --no-dereference  treat LINK_NAME as a normal file if it is a symbolic link to a directory
            -P, --physical  make hard links directly to symbolic links
            -r, --relative  with -s, create links relative to link location
            -s, --symbolic  make symbolic links instead of hard links
        `,
        values: copy_values(),
    }
}

fn rm() {
    #{
        opts: `
            -f, --force  ignore nonexistent files and arguments, never prompt
            -i  prompt before every removal
            -I  prompt once before removing more than three files, or when removing recursively
            --interactive[=when()]  prompt according to when(): never, once (-I), or always (-i)
            --one-file-system  skip directories on other file systems
            --no-preserve-root  do not treat / specially
            --preserve-root[=all]  do not remove / (the default)
            -r, -R, --recursive  remove directories and their contents recursively
            -d, --dir  remove empty directories
            -v, --verbose  explain what is being done
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--interactive": ["never", "once", "always"], "--preserve-root": ["all"]},
    }
}

fn mkdir() {
    #{
        opts: `
            -m, --mode=MODE  set file mode (as in chmod), not a=rwx - umask
            -p, --parents  make parent directories as needed, and no error if existing
            -v, --verbose  print a message for each created directory
            -Z  set SELinux security context of each created directory to the default
            --context[=CTX]  like -Z, or set the SELinux or SMACK context to CTX
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--mode": "mode", "--context": "none"},
        args: ["dirs"],
    }
}

fn touch() {
    #{
        opts: `
            -a  change only the access time
            -c, --no-create  do not create any files
            -d, --date=STRING  parse STRING and use it instead of current time
            -f  (ignored)
            -h, --no-dereference  affect each symbolic link instead of the file it references
            -m  change only the modification time
            -r, --reference=FILE  use this file's times instead of current time
            -t STAMP  use [[CC]YY]MMDDhhmm[.ss] instead of current time
            --time=WORD  change the specified time: access, atime, use, modify or mtime
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--date": "none", "-t": "none", "--time": ["access", "atime", "use", "modify", "mtime"]},
    }
}

// The options that chmod, chown and chgrp share.
fn owner_opts() {
    `
        -c, --changes  like verbose but report only when a change is made
        -f, --silent, --quiet  suppress most error messages
        -v, --verbose  output a diagnostic for every file processed
        --no-preserve-root  do not treat / specially (the default)
        --preserve-root  fail to operate recursively on /
        -R, --recursive  change files and directories recursively
        --help  display help and exit
        --version  output version information and exit
    `
}

// The options of chown and chgrp for symbolic links.
fn link_opts() {
    `
        --dereference  affect the referent of each symbolic link (the default)
        -h, --no-dereference  affect symbolic links instead of any referenced file
        -H  with -R, traverse a symbolic link to a directory on the command line
        -L  with -R, traverse every symbolic link to a directory
        -P  with -R, do not traverse any symbolic links (the default)
    `
}

fn chmod() {
    #{
        opts: owner_opts() + `
            --reference=RFILE  use RFILE's mode instead of specifying MODE values
        `,
        args: ["mode", "files"],
        args_if: #{"--reference": ["files"]},
    }
}

fn chown() {
    #{
        opts: owner_opts() + link_opts() + `
            --from=CURRENT_OWNER:CURRENT_GROUP  change only files with this owner and group
            --reference=RFILE  use RFILE's owner and group
        `,
        values: #{"--from": "owner"},
        args: ["owner", "files"],
        args_if: #{"--reference": ["files"]},
    }
}

fn chgrp() {
    #{
        opts: owner_opts() + link_opts() + `
            --reference=RFILE  use RFILE's group
        `,
        args: ["groups", "files"],
        args_if: #{"--reference": ["files"]},
    }
}

fn head() {
    #{
        opts: `
            -c, --bytes=NUM  print the first NUM bytes of each file (with -, all but the last NUM)
            -n, --lines=NUM  print the first NUM lines instead of the first 10 (with -, all but the last NUM)
            -q, --quiet, --silent  never print headers giving file names
            -v, --verbose  always print headers giving file names
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--bytes": "none", "--lines": "none"},
    }
}

fn tail() {
    #{
        opts: `
            -c, --bytes=NUM  output the last NUM bytes (with +, starting with byte NUM)
            -f  output appended data as the file grows
            --follow[=HOW]  output appended data as the file grows, following its name or descriptor
            -F  same as --follow=name --retry
            -n, --lines=NUM  output the last NUM lines, instead of the last 10 (with +, starting with line NUM)
            --max-unchanged-stats=N  with --follow=name, reopen a file that hasn't changed after N checks
            --pid=PID  with -f, terminate after process ID PID dies
            -q, --quiet, --silent  never output headers giving file names
            --retry  keep trying to open a file if it is inaccessible
            -s, --sleep-interval=N  with -f, sleep for about N seconds between checks
            -v, --verbose  always output headers giving file names
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--bytes": "none",
            "--lines": "none",
            "--follow": ["name", "descriptor"],
            "--max-unchanged-stats": "none",
            "--pid": "pids",
            "--sleep-interval": "none",
        },
    }
}

fn wc() {
    #{
        opts: `
            -c, --bytes  print the byte counts
            -m, --chars  print the character counts
            -l, --lines  print the newline counts
            --files0-from=F  read the names of the files from F, separated by NULs
            -L, --max-line-length  print the maximum display width
            -w, --words  print the word counts
            --total=when()  when to print a line with total counts
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--total": ["auto", "always", "only", "never"]},
    }
}

fn sort() {
    #{
        opts: `
            -b, --ignore-leading-blanks  ignore leading blanks
            -d, --dictionary-order  consider only blanks and alphanumeric characters
            -f, --ignore-case  fold lower case to upper case characters
            -g, --general-numeric-sort  compare according to general numerical value
            -i, --ignore-nonprinting  consider only printable characters
            -M, --month-sort  compare (unknown) < JAN < ... < DEC
            -h, --human-numeric-sort  compare human readable numbers (e.g., 2K 1G)
            -n, --numeric-sort  compare according to string numerical value
            -R, --random-sort  shuffle, but group identical keys
            --random-source=FILE  get random bytes from FILE
            -r, --reverse  reverse the result of comparisons
            --sort=WORD  sort according to WORD
            -V, --version-sort  natural sort of (version) numbers within text
            --batch-size=NMERGE  merge at most NMERGE inputs at once
            -c, --check[=HOW]  check for sorted input; do not sort
            -C  like -c, but do not report the first bad line
            --compress-program=PROG  compress temporaries with PROG
            --debug  annotate the part of the line used to sort
            --files0-from=F  read the names of the files from F, separated by NULs
            -k, --key=KEYDEF  sort via a key; KEYDEF gives location and type
            -m, --merge  merge already sorted files; do not sort
            -o, --output=FILE  write result to FILE instead of standard output
            -s, --stable  stabilize sort by disabling last-resort comparison
            -S, --buffer-size=SIZE  use SIZE for main memory buffer
            -t, --field-separator=SEP  use SEP instead of non-blank to blank transition
            -T, --temporary-directory=DIR  use DIR for temporaries, not $TMPDIR or /tmp
            --parallel=N  change the number of sorts run concurrently to N
            -u, --unique  output only the first of an equal run
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--sort": ["general-numeric", "human-numeric", "month", "numeric", "random", "version"],
            "--check": ["diagnose-first", "quiet", "silent"],
            "--batch-size": "none",
            "--compress-program": "none",
            "--key": "none",
            "--buffer-size": "none",
            "--field-separator": "none",
            "--temporary-directory": "dirs",
            "--parallel": "none",
        },
    }
}

fn uniq() {
    #{
        opts: `
            -c, --count  prefix lines by the number of occurrences
            -d, --repeated  only print duplicate lines, one for each group
            -D  print all duplicate lines
            --all-repeated[=METHOD]  like -D, but allow separating groups with an empty line
            -f, --skip-fields=N  avoid comparing the first N fields
            --group[=METHOD]  show all items, separating groups with an empty line
            -i, --ignore-case  ignore differences in case when comparing
            -s, --skip-chars=N  avoid comparing the first N characters
            -u, --unique  only print unique lines
            -z, --zero-terminated  line delimiter is NUL, not newline
            -w, --check-chars=N  compare no more than N characters in lines
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--all-repeated": ["none", "prepend", "separate"],
            "--group": ["separate", "prepend", "append", "both"],
            "--skip-fields": "none",
            "--skip-chars": "none",
            "--check-chars": "none",
        },
        args: ["files", "files", "none"],
    }
}

fn cut() {
    #{
        opts: `
            -b, --bytes=LIST  select only these bytes
            -c, --characters=LIST  select only these characters
            -d, --delimiter=DELIM  use DELIM instead of TAB for field delimiter
            -f, --fields=LIST  select only these fields
            -n  (ignored)
            --complement  complement the set of selected bytes, characters or fields
            -s, --only-delimited  do not print lines not containing delimiters
            --output-delimiter=STRING  use STRING as the output delimiter
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--bytes": "none",
            "--characters": "none",
            "--delimiter": "none",
            "--fields": "none",
            "--output-delimiter": "none",
        },
    }
}

fn du() {
    #{
        opts: `
            -0, --null  end each output line with NUL, not newline
            -a, --all  write counts for all files, not just directories
            --apparent-size  print apparent sizes rather than device usage
            -B, --block-size=SIZE  scale sizes by SIZE before printing them
            -b, --bytes  equivalent to --apparent-size --block-size=1
            -c, --total  produce a grand total
            -D, --dereference-args  dereference only symbolic links listed on the command line
            -d, --max-depth=N  print the total for a directory only if it is N or fewer levels below
            --files0-from=F  summarize the files named in F, separated by NULs
            -H  equivalent to --dereference-args (-D)
            -h, --human-readable  print sizes in human readable format (e.g., 1K 234M 2G)
            --inodes  list inode usage information instead of block usage
            -k  like --block-size=1K
            -L, --dereference  dereference all symbolic links
            -l, --count-links  count sizes many times if hard linked
            -m  like --block-size=1M
            -P, --no-dereference  don't follow any symbolic links (the default)
            -S, --separate-dirs  for directories do not include size of subdirectories
            --si  like -h, but use powers of 1000 not 1024
            -s, --summarize  display only a total for each argument
            -t, --threshold=SIZE  exclude entries smaller than SIZE (or greater, if negative)
            --time[=WORD]  show the time of the last modification (or WORD) of any file in the directory
            --time-style=STYLE  show times using STYLE
            -X, --exclude-from=FILE  exclude files that match any pattern in FILE
            --exclude=PATTERN  exclude files that match PATTERN
            -x, --one-file-system  skip directories on different file systems
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--block-size": "none",
            "--max-depth": "none",
            "--threshold": "none",
            "--time": ["atime", "access", "use", "ctime", "status"],
            "--time-style": time_style(),
            "--exclude": "none",
        },
    }
}

fn df() {
    #{
        opts: `
            -a, --all  include pseudo, duplicate, inaccessible file systems
            -B, --block-size=SIZE  scale sizes by SIZE before printing them
            -h, --human-readable  print sizes in powers of 1024 (e.g., 1023M)
            -H, --si  print sizes in powers of 1000 (e.g., 1.1G)
            -i, --inodes  list inode information instead of block usage
            -k  like --block-size=1K
            -l, --local  limit listing to local file systems
            --no-sync  do not invoke sync before getting usage info (the default)
            --output[=FIELD_LIST]  use the output format defined by FIELD_LIST
            -P, --portability  use the POSIX output format
            --sync  invoke sync before getting usage info
            --total  produce a grand total
            -t, --type=TYPE  limit listing to file systems of type TYPE
            -T, --print-type  print file system type
            -x, --exclude-type=TYPE  limit listing to file systems not of type TYPE
            -v  (ignored)
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--block-size": "none",
            "--output": [
                "source", "fstype", "itotal", "iused", "iavail", "ipcent", "size", "used", "avail", "pcent", "file",
                "target",
            ],
            "--type": "none",
            "--exclude-type": "none",
        },
    }
}

fn stat() {
    #{
        opts: `
            -L, --dereference  follow links
            -f, --file-system  display file system status instead of file status
            --cached=MODE  specify how to use cached attributes
            -c, --format=FORMAT  use the specified FORMAT instead of the default
            --printf=FORMAT  like --format, but interpret backslash escapes, and no trailing newline
            -t, --terse  print the information in terse form
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--cached": ["always", "never", "default"], "--format": "none", "--printf": "none"},
    }
}

fn tee() {
    #{
        opts: `
            -a, --append  append to the given files, do not overwrite
            -i, --ignore-interrupts  ignore interrupt signals
            -p  diagnose errors writing to non-pipes
            --output-error[=MODE]  set behavior on write error
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--output-error": ["warn", "warn-nopipe", "exit", "exit-nopipe"]},
    }
}

fn readlink() {
    #{opts: `
        -f, --canonicalize  canonicalize by following every symlink; all but the last component must exist
        -e, --canonicalize-existing  canonicalize by following every symlink; all components must exist
        -m, --canonicalize-missing  canonicalize by following every symlink, without requirements on existence
        -n, --no-newline  do not output the trailing delimiter
        -q, --quiet  suppress most error messages (the default)
        -s, --silent  suppress most error messages (the default)
        -v, --verbose  report error messages
        -z, --zero  end each output line with NUL, not newline
        --help  display help and exit
        --version  output version information and exit
    `}
}

fn realpath() {
    #{
        opts: `
            -e, --canonicalize-existing  all components of the path must exist
            -m, --canonicalize-missing  no path components need exist or be a directory
            -L, --logical  resolve .. components before symlinks
            -P, --physical  resolve symlinks as encountered (the default)
            -q, --quiet  suppress most error messages
            --relative-to=DIR  print the resolved path relative to DIR
            --relative-base=DIR  print absolute paths unless paths below DIR
            -s, --strip, --no-symlinks  don't expand symlinks
            -z, --zero  end each output line with NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--relative-to": "dirs", "--relative-base": "dirs"},
    }
}

fn basename() {
    #{
        opts: `
            -a, --multiple  support multiple arguments and treat each as a NAME
            -s, --suffix=SUFFIX  remove a trailing SUFFIX; implies -a
            -z, --zero  end each output line with NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--suffix": "none"},
    }
}

fn dirname() {
    #{opts: `
        -z, --zero  end each output line with NUL, not newline
        --help  display help and exit
        --version  output version information and exit
    `}
}

fn date() {
    #{
        opts: `
            -d, --date=STRING  display the time described by STRING, not now
            --debug  annotate the parsed date
            -f, --file=DATEFILE  like --date, once for each line of DATEFILE
            -I, --iso-8601[=FMT]  output date/time in ISO 8601 format
            --resolution  output the available resolution of timestamps
            -R, --rfc-email  output date and time in RFC 5322 format
            --rfc-3339=FMT  output date/time in RFC 3339 format
            -r, --reference=FILE  display the last modification time of FILE
            -s, --set=STRING  set the time described by STRING
            -u, --utc, --universal  print or set Coordinated Universal Time (UTC)
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--date": "none",
            "--iso-8601": ["date", "hours", "minutes", "seconds", "ns"],
            "--rfc-3339": ["date", "seconds", "ns"],
            "--set": "none",
        },
        // +FORMAT
        args: ["none"],
    }
}

fn dd() {
    #{
        opts: `
            --help  display help and exit
            --version  output version information and exit
        `,
        args: ["dd"],
    }
}

fn install() {
    #{
        opts: copy_opts() + `
            -c  (ignored)
            -C, --compare  don't change the destination file if it has the same contents and attributes
            -d, --directory  treat all arguments as directory names; create all components of them
            -D  create all leading components of DEST except the last
            --debug  explain how a file is copied (implies -v)
            -g, --group=GROUP  set group ownership, instead of the process's current group
            -m, --mode=MODE  set permission mode (as in chmod), instead of rwxr-xr-x
            -o, --owner=OWNER  set ownership (super-user only)
            -p, --preserve-timestamps  apply the access and modification times of SOURCE files to the copies
            -s, --strip  strip symbol tables
            --strip-program=PROGRAM  the program used to strip binaries
            --preserve-context  preserve SELinux security context
            -Z  set SELinux security context of destination to the default
            --context[=CTX]  like -Z, or set the SELinux or SMACK context to CTX
        `,
        values: copy_values() + #{
            "--group": "groups",
            "--mode": "mode",
            "--owner": "users",
            "--strip-program": "none",
            "--context": "none",
        },
    }
}

fn mktemp() {
    #{
        opts: `
            -d, --directory  create a directory, not a file
            -u, --dry-run  do not create anything; merely print a name (unsafe)
            -q, --quiet  suppress diagnostics about file/dir-creation failure
            --suffix=SUFF  append SUFF to TEMPLATE
            -p DIR, --tmpdir[=DIR]  interpret TEMPLATE relative to DIR ($TMPDIR, or else /tmp, by default)
            -t  interpret TEMPLATE as a single file name component, relative to a directory
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--suffix": "none", "--tmpdir": "dirs"},
        // TEMPLATE
        args: ["none"],
    }
}

fn nl() {
    let style = [
        #{value: "a", desc: "number all lines"},
        #{value: "t", desc: "number only nonempty lines"},
        #{value: "n", desc: "number no lines"},
        #{value: "p", desc: "number only lines that match a regular expression (pBRE)", suffix: ""},
    ];
    #{
        opts: `
            -b, --body-numbering=STYLE  use STYLE for numbering body lines
            -d, --section-delimiter=CC  use CC for logical page delimiters
            -f, --footer-numbering=STYLE  use STYLE for numbering footer lines
            -h, --header-numbering=STYLE  use STYLE for numbering header lines
            -i, --line-increment=NUMBER  line number increment at each line
            -l, --join-blank-lines=NUMBER  group of NUMBER empty lines counted as one
            -n, --number-format=FORMAT  insert line numbers according to FORMAT
            -p, --no-renumber  do not reset line numbers for each section
            -s, --number-separator=STRING  add STRING after (possible) line number
            -v, --starting-line-number=NUMBER  first line number for each section
            -w, --number-width=NUMBER  use NUMBER columns for line numbers
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--body-numbering": style,
            "--footer-numbering": style,
            "--header-numbering": style,
            "--section-delimiter": "none",
            "--line-increment": "none",
            "--join-blank-lines": "none",
            "--number-format": [
                #{value: "ln", desc: "left justified, no leading zeros"},
                #{value: "rn", desc: "right justified, no leading zeros"},
                #{value: "rz", desc: "right justified, leading zeros"},
            ],
            "--number-separator": "none",
            "--starting-line-number": "none",
            "--number-width": "none",
        },
    }
}

fn od() {
    #{
        opts: `
            -A, --address-radix=RADIX  output format for file offsets
            --endian=ORDER  swap input bytes according to the specified order
            -j, --skip-bytes=BYTES  skip BYTES input bytes first
            -N, --read-bytes=BYTES  limit dump to BYTES input bytes
            -S BYTES, --strings[=BYTES]  show only NUL terminated strings of at least BYTES graphic chars
            -t, --format=TYPE  select output format or formats
            -v, --output-duplicates  do not use * to mark line suppression
            -w, --width[=BYTES]  output BYTES bytes per output line
            --traditional  accept arguments in the traditional form
            -a  same as -t a, select named characters, ignoring high-order bit
            -b  same as -t o1, select octal bytes
            -c  same as -t c, select printable characters or backslash escapes
            -d  same as -t u2, select unsigned decimal 2-byte units
            -f  same as -t fF, select floats
            -i  same as -t dI, select decimal ints
            -l  same as -t dL, select decimal longs
            -o  same as -t o2, select octal 2-byte units
            -s  same as -t d2, select decimal 2-byte units
            -x  same as -t x2, select hexadecimal 2-byte units
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--address-radix": [
                #{value: "d", desc: "decimal"},
                #{value: "o", desc: "octal"},
                #{value: "x", desc: "hexadecimal"},
                #{value: "n", desc: "none"},
            ],
            "--endian": ["big", "little"],
            "--skip-bytes": "none",
            "--read-bytes": "none",
            "-S": "none",
            "--format": [
                #{value: "a", desc: "named characters, ignoring high-order bit"},
                #{value: "c", desc: "printable characters or backslash escapes"},
                #{value: "d", desc: "signed decimal", suffix: ""},
                #{value: "f", desc: "floating point", suffix: ""},
                #{value: "o", desc: "octal", suffix: ""},
                #{value: "u", desc: "unsigned decimal", suffix: ""},
                #{value: "x", desc: "hexadecimal", suffix: ""},
            ],
            "--width": "none",
        },
    }
}

fn paste() {
    #{
        opts: `
            -d, --delimiters=LIST  reuse characters from LIST instead of TABs
            -s, --serial  paste one file at a time instead of in parallel
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--delimiters": "none"},
    }
}

fn comm() {
    #{
        opts: `
            -1  suppress column 1 (lines unique to FILE1)
            -2  suppress column 2 (lines unique to FILE2)
            -3  suppress column 3 (lines that appear in both files)
            --check-order  check that the input is correctly sorted
            --nocheck-order  do not check that the input is correctly sorted
            --output-delimiter=STR  separate columns with STR
            --total  output a summary
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--output-delimiter": "none"},
        args: ["files", "files", "none"],
    }
}

fn join() {
    #{
        opts: `
            -a FILENUM  also print unpairable lines from file FILENUM (1 or 2)
            -e STRING  replace missing (empty) input fields with STRING
            -i, --ignore-case  ignore differences in case when comparing fields
            -j FIELD  equivalent to -1 FIELD -2 FIELD
            -o FORMAT  obey FORMAT while constructing output line
            -t CHAR  use CHAR as input and output field separator
            -v FILENUM  like -a FILENUM, but suppress joined output lines
            -1 FIELD  join on this FIELD of file 1
            -2 FIELD  join on this FIELD of file 2
            --check-order  check that the input is correctly sorted
            --nocheck-order  do not check that the input is correctly sorted
            --header  treat the first line in each file as field headers
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "-a": ["1", "2"],
            "-e": "none",
            "-j": "none",
            "-o": "none",
            "-t": "none",
            "-v": ["1", "2"],
            "-1": "none",
            "-2": "none",
        },
        args: ["files", "files", "none"],
    }
}

fn tr() {
    #{
        opts: `
            -c, -C, --complement  use the complement of ARRAY1
            -d, --delete  delete characters in ARRAY1, do not translate
            -s, --squeeze-repeats  squeeze repeats of the characters in the last ARRAY into one
            -t, --truncate-set1  first truncate ARRAY1 to length of ARRAY2
            --help  display help and exit
            --version  output version information and exit
        `,
        args: [[
            "[:alnum:]", "[:alpha:]", "[:blank:]", "[:cntrl:]", "[:digit:]", "[:graph:]", "[:lower:]", "[:print:]",
            "[:punct:]", "[:space:]", "[:upper:]", "[:xdigit:]",
        ]],
    }
}

fn seq() {
    #{
        opts: `
            -f, --format=FORMAT  use printf style floating-point FORMAT
            -s, --separator=STRING  use STRING to separate numbers (default: newline)
            -w, --equal-width  equalize width by padding with leading zeroes
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--format": "none", "--separator": "none"},
        args: ["none"],
    }
}

fn shuf() {
    #{
        opts: `
            -e, --echo  treat each ARG as an input line
            -i, --input-range=LO-HI  treat each number LO through HI as an input line
            -n, --head-count=COUNT  output at most COUNT lines
            -o, --output=FILE  write result to FILE instead of standard output
            --random-source=FILE  get random bytes from FILE
            -r, --repeat  output lines can be repeated
            -z, --zero-terminated  line delimiter is NUL, not newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--input-range": "none", "--head-count": "none"},
        args_if: #{"-e": ["none"], "-i": ["none"]},
    }
}

fn split() {
    #{
        opts: `
            -a, --suffix-length=N  generate suffixes of length N (default 2)
            --additional-suffix=SUFFIX  append an additional SUFFIX to file names
            -b, --bytes=SIZE  put SIZE bytes per output file
            -C, --line-bytes=SIZE  put at most SIZE bytes of records per output file
            -d  use numeric suffixes starting at 0, not alphabetic
            --numeric-suffixes[=FROM]  same as -d, but allow setting the start value
            -x  use hex suffixes starting at 0, not alphabetic
            --hex-suffixes[=FROM]  same as -x, but allow setting the start value
            -e, --elide-empty-files  do not generate empty output files with -n
            --filter=COMMAND  write to shell COMMAND; file name is $FILE
            -l, --lines=NUMBER  put NUMBER lines/records per output file
            -n, --number=CHUNKS  generate CHUNKS output files
            -t, --separator=SEP  use SEP instead of newline as the record separator
            -u, --unbuffered  immediately copy input to output with -n r/...
            --verbose  print a diagnostic just before each output file is opened
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{
            "--suffix-length": "none",
            "--additional-suffix": "none",
            "--bytes": "none",
            "--line-bytes": "none",
            "--numeric-suffixes": "none",
            "--hex-suffixes": "none",
            "--filter": "none",
            "--lines": "none",
            "--number": "none",
            "--separator": "none",
        },
        // FILE, then PREFIX
        args: ["files", "none"],
    }
}

fn tac() {
    #{
        opts: `
            -b, --before  attach the separator before instead of after
            -r, --regex  interpret the separator as a regular expression
            -s, --separator=STRING  use STRING as the separator instead of newline
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--separator": "none"},
    }
}

fn truncate() {
    #{
        opts: `
            -c, --no-create  do not create any files
            -o, --io-blocks  treat SIZE as number of IO blocks instead of bytes
            -r, --reference=RFILE  base size on RFILE
            -s, --size=SIZE  set or adjust the file size by SIZE bytes
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--size": "none"},
    }
}

// md5sum, sha256sum and the like.
fn checksum() {
    #{opts: `
        -b, --binary  read in binary mode
        -c, --check  read checksums from the files and check them
        --tag  create a BSD-style checksum
        -t, --text  read in text mode (the default)
        -z, --zero  end each output line with NUL, not newline
        --ignore-missing  don't fail or report status for missing files
        --quiet  don't print OK for each successfully verified file
        --status  don't output anything, the status code shows success
        --strict  exit non-zero for improperly formatted checksum lines
        -w, --warn  warn about improperly formatted checksum lines
        --help  display help and exit
        --version  output version information and exit
    `}
}

fn base64() {
    #{
        opts: `
            -d, --decode  decode data
            -i, --ignore-garbage  when decoding, ignore non-alphabet characters
            -w, --wrap=COLS  wrap encoded lines after COLS characters (default 76, 0 for none)
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--wrap": "none"},
    }
}

fn shred() {
    #{
        opts: `
            -f, --force  change permissions to allow writing if necessary
            -n, --iterations=N  overwrite N times instead of the default (3)
            --random-source=FILE  get random bytes from FILE
            -s, --size=N  shred this many bytes (suffixes like K, M, G accepted)
            -u  deallocate and remove file after overwriting
            --remove[=HOW]  like -u, but with control on HOW to delete
            -v, --verbose  show progress
            -x, --exact  do not round file sizes up to the next full block
            -z, --zero  add a final overwrite with zeros to hide shredding
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--iterations": "none", "--size": "none", "--remove": ["unlink", "wipe", "wipesync"]},
    }
}

fn rmdir() {
    #{
        opts: `
            --ignore-fail-on-non-empty  ignore each failure to remove a non-empty directory
            -p, --parents  remove DIRECTORY and its ancestors
            -v, --verbose  output a diagnostic for every directory processed
            --help  display help and exit
            --version  output version information and exit
        `,
        args: ["dirs"],
    }
}

fn uname() {
    #{
        opts: `
            -a, --all  print all information
            -s, --kernel-name  print the kernel name
            -n, --nodename  print the network node hostname
            -r, --kernel-release  print the kernel release
            -v, --kernel-version  print the kernel version
            -m, --machine  print the machine hardware name
            -p, --processor  print the processor type (non-portable)
            -i, --hardware-platform  print the hardware platform (non-portable)
            -o, --operating-system  print the operating system
            --help  display help and exit
            --version  output version information and exit
        `,
        args: ["none"],
    }
}

fn id() {
    #{
        opts: `
            -a  ignored, for compatibility with other versions
            -Z, --context  print only the security context of the process
            -g, --group  print only the effective group ID
            -G, --groups  print all group IDs
            -n, --name  print a name instead of a number, for -ugG
            -r, --real  print the real ID instead of the effective ID, with -ugG
            -u, --user  print only the effective user ID
            -z, --zero  delimit entries with NUL characters, not whitespace
            --help  display help and exit
            --version  output version information and exit
        `,
        args: ["users"],
    }
}

fn fold() {
    #{
        opts: `
            -b, --bytes  count bytes rather than columns
            -s, --spaces  break at spaces
            -w, --width=WIDTH  use WIDTH columns instead of 80
            --help  display help and exit
            --version  output version information and exit
        `,
        values: #{"--width": "none"},
    }
}

fn cmp() {
    #{
        opts: `
            -b, --print-bytes  print differing bytes
            -i, --ignore-initial=SKIP  skip the first SKIP bytes of both inputs (or SKIP1:SKIP2)
            -l, --verbose  output byte numbers and differing byte values
            -n, --bytes=LIMIT  compare at most LIMIT bytes
            -s, --quiet, --silent  suppress all normal output
            --help  display help and exit
            -v, --version  output version information and exit
        `,
        values: #{"--ignore-initial": "none", "--bytes": "none"},
        // FILE1 FILE2, then SKIP1 SKIP2
        args: ["files", "files", "none"],
    }
}

fn diff3() {
    #{
        opts: `
            -A, --show-all  output all changes, bracketing conflicts
            -e, --ed  output ed script incorporating changes from OLDFILE to YOURFILE into MYFILE
            -E, --show-overlap  like -e, but bracket conflicts
            -3, --easy-only  like -e, but incorporate only nonoverlapping changes
            -x, --overlap-only  like -e, but incorporate only overlapping changes
            -X  like -x, but bracket conflicts
            -i  append w and q commands to ed scripts
            -m, --merge  output the actual merged file
            -a, --text  treat all files as text
            --strip-trailing-cr  strip trailing carriage return on input
            -T, --initial-tab  make tabs line up by prepending a tab
            --diff-program=PROGRAM  use PROGRAM to compare files
            -L, --label=LABEL  use LABEL instead of file name (up to three times)
            --help  display help and exit
            -v, --version  output version information and exit
        `,
        values: #{"--diff-program": "none", "--label": "none"},
    }
}

fn sdiff() {
    #{
        opts: `
            -o, --output=FILE  operate interactively, sending output to FILE
            -i, --ignore-case  consider upper- and lower-case to be the same
            -E, --ignore-tab-expansion  ignore changes due to tab expansion
            -Z, --ignore-trailing-space  ignore white space at line end
            -b, --ignore-space-change  ignore changes in the amount of white space
            -W, --ignore-all-space  ignore all white space
            -B, --ignore-blank-lines  ignore changes whose lines are all blank
            -I, --ignore-matching-lines=RE  ignore changes all whose lines match RE
            --strip-trailing-cr  strip trailing carriage return on input
            -a, --text  treat all files as text
            -w, --width=NUM  output at most NUM (default 130) print columns
            -l, --left-column  output only the left column of common lines
            -s, --suppress-common-lines  do not output common lines
            -t, --expand-tabs  expand tabs to spaces in output
            --tabsize=NUM  tab stops at every NUM (default 8) print columns
            -d, --minimal  try hard to find a smaller set of changes
            -H, --speed-large-files  assume large files, many scattered small changes
            --diff-program=PROGRAM  use PROGRAM to compare files
            --help  display help and exit
            -v, --version  output version information and exit
        `,
        values: #{
            "--ignore-matching-lines": "none",
            "--width": "none",
            "--tabsize": "none",
            "--diff-program": "none",
        },
    }
}

fn grep() {
    #{
        opts: `
            -E, --extended-regexp  PATTERNS are extended regular expressions
            -F, --fixed-strings  PATTERNS are strings
            -G, --basic-regexp  PATTERNS are basic regular expressions
            -P, --perl-regexp  PATTERNS are Perl regular expressions
            -e, --regexp=PATTERNS  use PATTERNS for matching
            -f, --file=FILE  take PATTERNS from FILE
            -i, --ignore-case  ignore case distinctions in patterns and data
            --no-ignore-case  do not ignore case distinctions (the default)
            -w, --word-regexp  match only whole words
            -x, --line-regexp  match only whole lines
            -z, --null-data  a data line ends in 0 byte, not newline
            -s, --no-messages  suppress error messages
            -v, --invert-match  select non-matching lines
            -V, --version  display version information and exit
            --help  display help and exit
            -m, --max-count=NUM  stop after NUM selected lines
            -b, --byte-offset  print the byte offset with output lines
            -n, --line-number  print line number with output lines
            --line-buffered  flush output on every line
            -H, --with-filename  print file name with output lines
            -h, --no-filename  suppress the file name prefix on output
            --label=LABEL  use LABEL as the standard input file name prefix
            -o, --only-matching  show only nonempty parts of lines that match
            -q, --quiet, --silent  suppress all normal output
            --binary-files=TYPE  assume that binary files are TYPE
            -a, --text  equivalent to --binary-files=text
            -I  equivalent to --binary-files=without-match
            -d, --directories=ACTION  how to handle directories
            -D, --devices=ACTION  how to handle devices, FIFOs and sockets
            -r, --recursive  like --directories=recurse
            -R, --dereference-recursive  likewise, but follow all symlinks
            --include=GLOB  search only files that match GLOB
            --exclude=GLOB  skip files that match GLOB
            --exclude-from=FILE  skip files that match any file pattern from FILE
            --exclude-dir=GLOB  skip directories that match GLOB
            -L, --files-without-match  print only names of files with no selected lines
            -l, --files-with-matches  print only names of files with selected lines
            -c, --count  print only a count of selected lines per file
            -T, --initial-tab  make tabs line up (if needed)
            -Z, --null  print 0 byte after file name
            -B, --before-context=NUM  print NUM lines of leading context
            -A, --after-context=NUM  print NUM lines of trailing context
            -C, --context=NUM  print NUM lines of output context
            --group-separator=SEP  print SEP on line between matches with context
            --no-group-separator  do not print separator for matches with context
            --color[=when()], --colour[=when()]  use markers to highlight the matching strings
            -U, --binary  do not strip CR characters at EOL (MSDOS/Windows)
        `,
        values: #{
            "--regexp": "none",
            "--max-count": "none",
            "--label": "none",
            "--binary-files": ["binary", "text", "without-match"],
            "--directories": ["read", "recurse", "skip"],
            "--devices": ["read", "skip"],
            "--include": "none",
            "--exclude": "none",
            "--exclude-dir": "none",
            "--before-context": "none",
            "--after-context": "none",
            "--context": "none",
            "--group-separator": "none",
            "--color": when(),
        },
        // The first argument is the pattern, unless -e or -f gives it.
        args: ["none", "files"],
        args_if: #{"-e": ["files"], "-f": ["files"]},
    }
}

fn diff() {
    #{
        opts: `
            --normal  output a normal diff (the default)
            -q, --brief  report only when files differ
            -s, --report-identical-files  report when two files are the same
            -c  output 3 lines of copied context
            -C NUM, --context[=NUM]  output NUM (default 3) lines of copied context
            -u  output 3 lines of unified context
            -U NUM, --unified[=NUM]  output NUM (default 3) lines of unified context
            -e, --ed  output an ed script
            -n, --rcs  output an RCS format diff
            -y, --side-by-side  output in two columns
            -W, --width=NUM  output at most NUM (default 130) print columns
            --left-column  output only the left column of common lines
            --suppress-common-lines  do not output common lines
            -p, --show-c-function  show which C function each change is in
            -F, --show-function-line=RE  show the most recent line matching RE
            --label=LABEL  use LABEL instead of file name and timestamp
            -t, --expand-tabs  expand tabs to spaces in output
            -T, --initial-tab  make tabs line up by prepending a tab
            --tabsize=NUM  tab stops every NUM (default 8) print columns
            --suppress-blank-empty  suppress space or tab before empty output lines
            -l, --paginate  pass output through pr to paginate it
            -r, --recursive  recursively compare any subdirectories found
            --no-dereference  don't follow symbolic links
            -N, --new-file  treat absent files as empty
            --unidirectional-new-file  treat absent first files as empty
            --ignore-file-name-case  ignore case when comparing file names
            --no-ignore-file-name-case  consider case when comparing file names
            -x, --exclude=PAT  exclude files that match PAT
            -X, --exclude-from=FILE  exclude files that match any pattern in FILE
            -S, --starting-file=FILE  start with FILE when comparing directories
            --from-file=FILE1  compare FILE1 to all operands
            --to-file=FILE2  compare all operands to FILE2
            -i, --ignore-case  ignore case differences in file contents
            -E, --ignore-tab-expansion  ignore changes due to tab expansion
            -Z, --ignore-trailing-space  ignore white space at line end
            -b, --ignore-space-change  ignore changes in the amount of white space
            -w, --ignore-all-space  ignore all white space
            -B, --ignore-blank-lines  ignore changes where lines are all blank
            -I, --ignore-matching-lines=RE  ignore changes where all lines match RE
            -a, --text  treat all files as text
            --strip-trailing-cr  strip trailing carriage return on input
            -D, --ifdef=NAME  output merged file with #ifdef NAME diffs
            --GTYPE-group-format=GFMT  format GTYPE input groups with GFMT
            --line-format=LFMT  format all input lines with LFMT
            --LTYPE-line-format=LFMT  format LTYPE input lines with LFMT
            -d, --minimal  try hard to find a smaller set of changes
            --horizon-lines=NUM  keep NUM lines of the common prefix and suffix
            --speed-large-files  assume large files and many scattered small changes
            --color[=when()]  color output
            --palette=PALETTE  the colors to use when --color is active
            --help  display help and exit
            -v, --version  output version information and exit
        `,
        values: #{
            "-C": "none",
            "-U": "none",
            "--width": "none",
            "--show-function-line": "none",
            "--label": "none",
            "--tabsize": "none",
            "--exclude": "none",
            "--ignore-matching-lines": "none",
            "--ifdef": "none",
            "--horizon-lines": "none",
            "--color": when(),
            "--palette": "none",
        },
    }
}

fn tar() {
    #{
        opts: `
            -A, --catenate, --concatenate  append tar files to an archive
            -c, --create  create a new archive
            -d, --diff, --compare  find differences between archive and file system
            --delete  delete from the archive
            -r, --append  append files to the end of an archive
            -t, --list  list the contents of an archive
            --test-label  test the archive volume label and exit
            -u, --update  only append files newer than copy in archive
            -x, --extract, --get  extract files from an archive
            -C, --directory=DIR  change to directory DIR
            -f, --file=ARCHIVE  use archive file or device ARCHIVE
            -a, --auto-compress  use archive suffix to determine the compression program
            -I, --use-compress-program=PROG  filter through PROG
            -j, --bzip2  filter the archive through bzip2
            -J, --xz  filter the archive through xz
            --lzip  filter the archive through lzip
            --lzma  filter the archive through lzma
            --zstd  filter the archive through zstd
            -z, --gzip, --gunzip, --ungzip  filter the archive through gzip
            -Z, --compress, --uncompress  filter the archive through compress
            --exclude=PATTERN  exclude files, given as a PATTERN
            --exclude-from=FILE, -X FILE  exclude patterns listed in FILE
            --exclude-vcs  exclude version control system directories
            --exclude-vcs-ignores  read exclude patterns from the VCS ignore files
            -T, --files-from=FILE  get names to extract or create from FILE
            --null  -T reads null-terminated names
            -h, --dereference  follow symlinks; archive and dump the files they point to
            --hard-dereference  follow hard links; archive and dump the files they refer to
            -k, --keep-old-files  don't replace existing files when extracting
            --keep-newer-files  don't replace existing files that are newer than their archive copies
            --skip-old-files  don't replace existing files when extracting, silently skip over them
            --overwrite  overwrite existing files when extracting
            --remove-files  remove files after adding them to the archive
            -m, --touch  don't extract file modified time
            -p, --preserve-permissions, --same-permissions  extract information about file permissions
            --same-owner  try extracting files with the same ownership as exists in the archive
            --no-same-owner  extract files as yourself
            --owner=NAME  force NAME as owner for added files
            --group=NAME  force NAME as group for added files
            --mode=CHANGES  force (symbolic) mode CHANGES for added files
            --mtime=DATE-OR-FILE  set mtime for added files from DATE-OR-FILE
            -N, --newer=DATE-OR-FILE, --after-date=DATE-OR-FILE  only store files newer than DATE-OR-FILE
            --one-file-system  stay in local file system when creating archive
            -P, --absolute-names  don't strip leading / from file names
            --strip-components=NUMBER  strip NUMBER leading components from file names on extraction
            --transform=EXPRESSION, --xform=EXPRESSION  use sed replace EXPRESSION to transform file names
            -H, --format=FORMAT  create archive of the given format
            -O, --to-stdout  extract files to standard output
            -S, --sparse  handle sparse files efficiently
            -W, --verify  attempt to verify the archive after writing it
            --checkpoint[=NUMBER]  display progress messages every NUMBERth record
            --totals[=SIGNAL]  print total bytes after processing the archive
            -v, --verbose  verbosely list files processed
            -w, --interactive, --confirmation  ask for confirmation for every action
            --wildcards  use wildcards
            --no-wildcards  match names exactly
            -?, --help  give this help list
            --usage  give a short usage message
            --version  print program version
        `,
        values: #{
            "--directory": "dirs",
            "--use-compress-program": "none",
            "--exclude": "none",
            "--owner": "users",
            "--group": "groups",
            "--mode": "mode",
            "--strip-components": "none",
            "--transform": "none",
            "--format": ["gnu", "oldgnu", "pax", "posix", "ustar", "v7"],
            "--checkpoint": "none",
            "--totals": "signals",
        },
        args: ["members"],
    }
}

fn make() {
    #{
        opts: `
            -b, -m  ignored for compatibility
            -B, --always-make  unconditionally make all targets
            -C DIRECTORY, --directory=DIRECTORY  change to DIRECTORY before doing anything
            -d  print lots of debugging information
            --debug[=FLAGS]  print various types of debugging information
            -e, --environment-overrides  environment variables override makefiles
            -E STRING, --eval=STRING  evaluate STRING as a makefile statement
            -f FILE, --file=FILE, --makefile=FILE  read FILE as a makefile
            -h, --help  print the help and exit
            -i, --ignore-errors  ignore errors from recipes
            -I DIRECTORY, --include-dir=DIRECTORY  search DIRECTORY for included makefiles
            -j, --jobs[=N]  allow N jobs at once; infinite jobs with no argument
            -k, --keep-going  keep going when some targets can't be made
            -l, --load-average[=N], --max-load[=N]  don't start multiple jobs unless load is below N
            -L, --check-symlink-times  use the latest mtime between symlinks and target
            -n, --just-print, --dry-run, --recon  don't run the recipes; just print them
            -o FILE, --old-file=FILE, --assume-old=FILE  consider FILE to be very old and don't remake it
            -O, --output-sync[=TYPE]  synchronize output of parallel jobs by TYPE
            -p, --print-data-base  print make's internal database
            -q, --question  run no recipe; exit status says if up to date
            -r, --no-builtin-rules  disable the built-in implicit rules
            -R, --no-builtin-variables  disable the built-in variable settings
            -s, --silent, --quiet  don't echo recipes
            --no-silent  echo recipes (disable --silent mode)
            -S, --no-keep-going, --stop  turns off -k
            -t, --touch  touch targets instead of remaking them
            --trace  print tracing information
            -v, --version  print the version number of make and exit
            -w, --print-directory  print the current directory
            --no-print-directory  turn off -w, even if it was turned on implicitly
            -W FILE, --what-if=FILE, --new-file=FILE, --assume-new=FILE  consider FILE to be infinitely new
            --warn-undefined-variables  warn when an undefined variable is referenced
        `,
        values: #{
            "--directory": "dirs",
            "--include-dir": "dirs",
            "--eval": "none",
            "--debug": ["a", "b", "v", "i", "j", "m", "p", "w", "n"],
            "--jobs": "none",
            "--load-average": "none",
            "--output-sync": ["none", "line", "target", "recurse"],
        },
        args: ["targets"],
    }
}

fn rsync() {
    #{
        opts: `
            -v, --verbose  increase verbosity
            --info=FLAGS  fine-grained informational verbosity
            --debug=FLAGS  fine-grained debug verbosity
            -q, --quiet  suppress non-error messages
            -c, --checksum  skip based on checksum, not mod-time and size
            -a, --archive  archive mode, the same as -rlptgoD (no -A, -X, -U, -N, -H)
            -r, --recursive  recurse into directories
            -R, --relative  use relative path names
            --no-implied-dirs  don't send implied dirs with --relative
            -b, --backup  make backups (see --suffix and --backup-dir)
            --backup-dir=DIR  make backups into hierarchy based in DIR
            --suffix=SUFFIX  backup suffix (default ~ w/o --backup-dir)
            -u, --update  skip files that are newer on the receiver
            --inplace  update destination files in-place
            --append  append data onto shorter files
            -d, --dirs  transfer directories without recursing
            --mkpath  create destination's missing path components
            -l, --links  copy symlinks as symlinks
            -L, --copy-links  transform symlink into referent file/dir
            --copy-unsafe-links  only "unsafe" symlinks are transformed
            --safe-links  ignore symlinks that point outside the tree
            -k, --copy-dirlinks  transform symlink to dir into referent dir
            -K, --keep-dirlinks  treat symlinked dir on receiver as dir
            -H, --hard-links  preserve hard links
            -p, --perms  preserve permissions
            -E, --executability  preserve executability
            --chmod=CHMOD  affect file and/or directory permissions
            -A, --acls  preserve ACLs (implies --perms)
            -X, --xattrs  preserve extended attributes
            -o, --owner  preserve owner (super-user only)
            -g, --group  preserve group
            --devices  preserve device files (super-user only)
            --specials  preserve special files
            -D  same as --devices --specials
            -t, --times  preserve modification times
            -U, --atimes  preserve access (use) times
            -N, --crtimes  preserve create times (newness)
            -O, --omit-dir-times  omit directories from --times
            --super  receiver attempts super-user activities
            --fake-super  store/recover privileged attrs using xattrs
            -S, --sparse  turn sequences of nulls into sparse blocks
            -n, --dry-run  perform a trial run with no changes made
            -W, --whole-file  copy files whole (w/o delta-xfer algorithm)
            -x, --one-file-system  don't cross filesystem boundaries
            -B, --block-size=SIZE  force a fixed checksum block-size
            -e, --rsh=COMMAND  specify the remote shell to use
            --rsync-path=PROGRAM  specify the rsync to run on remote machine
            --existing  skip creating new files on receiver
            --ignore-existing  skip updating files that exist on receiver
            --remove-source-files  sender removes synchronized files (non-dir)
            --delete  delete extraneous files from destination dirs
            --delete-before  receiver deletes before transfer, not during
            --delete-during  receiver deletes during the transfer
            --delete-delay  find deletions during, delete after
            --delete-after  receiver deletes after transfer, not during
            --delete-excluded  also delete excluded files from destination dirs
            --ignore-errors  delete even if there are I/O errors
            --force  force deletion of directories even if not empty
            --max-delete=NUM  don't delete more than NUM files
            --max-size=SIZE  don't transfer any file larger than SIZE
            --min-size=SIZE  don't transfer any file smaller than SIZE
            --partial  keep partially transferred files
            --partial-dir=DIR  put a partially transferred file into DIR
            --delay-updates  put all updated files into place at end
            -m, --prune-empty-dirs  prune empty directory chains from file-list
            --numeric-ids  don't map uid/gid values by user/group name
            --timeout=SECONDS  set I/O timeout in seconds
            -I, --ignore-times  don't skip files that match size and time
            --size-only  skip files that match in size
            --modify-window=NUM  set the accuracy for mod-time comparisons
            -T, --temp-dir=DIR  create temporary files in directory DIR
            -y, --fuzzy  find similar file for basis if no dest file
            --compare-dest=DIR  also compare destination files relative to DIR
            --copy-dest=DIR  ... and include copies of unchanged files
            --link-dest=DIR  hardlink to files in DIR when unchanged
            -z, --compress  compress file data during the transfer
            --compress-choice=STR  choose the compression algorithm
            -C, --cvs-exclude  auto-ignore files in the same way CVS does
            -f, --filter=RULE  add a file-filtering RULE
            -F  same as --filter='dir-merge /.rsync-filter', repeated: --filter='- .rsync-filter'
            --exclude=PATTERN  exclude files matching PATTERN
            --exclude-from=FILE  read exclude patterns from FILE
            --include=PATTERN  don't exclude files matching PATTERN
            --include-from=FILE  read include patterns from FILE
            --files-from=FILE  read list of source-file names from FILE
            -0, --from0  all *-from/filter files are delimited by 0s
            -s, --secluded-args  use the protocol to safely send the args
            --address=ADDRESS  bind address for outgoing socket to daemon
            --port=PORT  specify double-colon alternate port number
            --blocking-io  use blocking I/O for the remote shell
            --stats  give some file-transfer stats
            -8, --8-bit-output  leave high-bit chars unescaped in output
            -h, --human-readable  output numbers in a human-readable format
            --progress  show progress during transfer
            -P  same as --partial --progress
            -i, --itemize-changes  output a change-summary for all updates
            -M, --remote-option=OPTION  send OPTION to the remote side only
            --out-format=FORMAT  output updates using the specified FORMAT
            --log-file=FILE  log what we're doing to the specified FILE
            --list-only  list the files instead of copying them
            --bwlimit=RATE  limit socket I/O bandwidth
            --stop-after=MINS  stop rsync after MINS minutes have elapsed
            --write-batch=FILE  write a batched update to FILE
            --read-batch=FILE  read a batched update from FILE
            --password-file=FILE  read daemon-access password from FILE
            -4, --ipv4  prefer IPv4
            -6, --ipv6  prefer IPv6
            -V, --version  print the version and other info and exit
            --help  show help and exit
        `,
        values: #{
            "--info": "none",
            "--debug": "none",
            "--backup-dir": "dirs",
            "--suffix": "none",
            "--chmod": "none",
            "--block-size": "none",
            "--rsh": "none",
            "--rsync-path": "none",
            "--max-delete": "none",
            "--max-size": "none",
            "--min-size": "none",
            "--partial-dir": "dirs",
            "--timeout": "none",
            "--modify-window": "none",
            "--temp-dir": "dirs",
            "--compare-dest": "dirs",
            "--copy-dest": "dirs",
            "--link-dest": "dirs",
            "--compress-choice": ["zstd", "lz4", "zlibx", "zlib", "none"],
            "--filter": "none",
            "--exclude": "none",
            "--include": "none",
            "--address": "none",
            "--port": "none",
            "--remote-option": "none",
            "--out-format": "none",
            "--bwlimit": "none",
            "--stop-after": "none",
        },
        args: ["remote"],
    }
}

fn man() {
    #{
        opts: `
            -a, --all  display all the pages that match, one after the other
            -C, --config-file=FILE  use this user configuration file
            -d, --debug  emit debugging messages
            -f, --whatis  equivalent to whatis
            -k, --apropos  equivalent to apropos
            -K, --global-apropos  search for text in all pages
            -l, --local-file  interpret PAGE arguments as local filenames
            -L, --locale=LOCALE  define the locale for this particular man search
            -M, --manpath=PATH  set search path for manual pages to PATH
            -P, --pager=PAGER  use program PAGER to display output
            -S, -s, --sections=LIST  use colon separated section list
            -w, --where, --path, --location  print physical location of man page(s)
            -W, --where-cat, --location-cat  print physical location of cat file(s)
            -H, --html[=BROWSER]  use www-browser or BROWSER to display HTML output
            -Z, --ditroff  use groff and force it to produce ditroff
            --help  give this help list
            -V, --version  print program version
        `,
        values: #{
            "--locale": "none",
            "--manpath": "none",
            "--pager": "none",
            "--sections": "sections",
            "--html": "none",
        },
        args: ["manpages"],
        args_if: #{"-l": ["files"], "-K": ["none"], "-k": ["none"], "-f": ["manpages"]},
    }
}

// The options that ssh, scp and sftp share.
fn ssh_opts() {
    `
        -4  use IPv4 addresses only
        -6  use IPv6 addresses only
        -A  enable forwarding of the authentication agent connection
        -C  compress all data
        -c CIPHER_SPEC  select the ciphers for encrypting the session
        -F CONFIGFILE  use an alternative per-user configuration file
        -i IDENTITY_FILE  select the file from which the identity (private key) is read
        -J DESTINATION  connect through the jump host DESTINATION
        -o OPTION  give options in the format of the configuration file
        -q  quiet mode
        -v  verbose mode (can be repeated)
    `
}

fn ssh_options() {
    [
        #{value: "StrictHostKeyChecking=", suffix: ""},
        #{value: "UserKnownHostsFile=", suffix: ""},
        #{value: "IdentityFile=", suffix: ""},
        #{value: "IdentitiesOnly=", suffix: ""},
        #{value: "User=", suffix: ""},
        #{value: "Port=", suffix: ""},
        #{value: "ProxyJump=", suffix: ""},
        #{value: "ProxyCommand=", suffix: ""},
        #{value: "ServerAliveInterval=", suffix: ""},
        #{value: "ServerAliveCountMax=", suffix: ""},
        #{value: "ConnectTimeout=", suffix: ""},
        #{value: "ForwardAgent=", suffix: ""},
        #{value: "BatchMode=", suffix: ""},
        #{value: "PasswordAuthentication=", suffix: ""},
        #{value: "PubkeyAuthentication=", suffix: ""},
        #{value: "LogLevel=", suffix: ""},
        #{value: "ControlMaster=", suffix: ""},
        #{value: "ControlPath=", suffix: ""},
        #{value: "ControlPersist=", suffix: ""},
        #{value: "SendEnv=", suffix: ""},
        #{value: "SetEnv=", suffix: ""},
        #{value: "RequestTTY=", suffix: ""},
    ]
}

fn ssh_values() {
    #{"-c": "none", "-J": "hosts", "-o": ssh_options()}
}

fn ssh() {
    #{
        opts: ssh_opts() + `
            -a  disable forwarding of the authentication agent connection
            -B BIND_INTERFACE  bind to the address of BIND_INTERFACE
            -b BIND_ADDRESS  use BIND_ADDRESS as the source address
            -D PORT  dynamic application-level port forwarding (SOCKS)
            -E LOG_FILE  append debug logs to LOG_FILE instead of standard error
            -e ESCAPE_CHAR  set the escape character (default ~)
            -f  go to background just before command execution
            -G  print the configuration after evaluating Host and Match blocks, and exit
            -g  allow remote hosts to connect to local forwarded ports
            -K  enable GSSAPI-based authentication
            -k  disable forwarding of GSSAPI credentials
            -L LOCAL_FORWARD  forward a local port or socket to the remote side
            -l LOGIN_NAME  the user to log in as on the remote machine
            -M  master mode for connection sharing
            -m MAC_SPEC  the MAC (message authentication code) algorithms
            -N  do not execute a remote command
            -n  redirect stdin from /dev/null
            -O CTL_CMD  control an active connection multiplexing master process
            -p PORT  the port to connect to on the remote host
            -Q QUERY_OPTION  list the algorithms supported
            -R REMOTE_FORWARD  forward a remote port or socket to the local side
            -S CTL_PATH  the location of a control socket for connection sharing
            -s  request invocation of a subsystem on the remote system
            -T  disable pseudo-terminal allocation
            -t  force pseudo-terminal allocation
            -V  display the version number and exit
            -W HOST:PORT  forward standard input and output to HOST on PORT
            -w LOCAL_TUN[:REMOTE_TUN]  request tunnel device forwarding
            -X  enable X11 forwarding
            -x  disable X11 forwarding
            -Y  enable trusted X11 forwarding
            -y  send log information using the syslog system module
        `,
        values: ssh_values() + #{
            "-B": "none",
            "-b": "none",
            "-D": "none",
            "-e": "none",
            "-L": "none",
            "-l": "users",
            "-m": "none",
            "-O": ["check", "forward", "cancel", "exit", "stop"],
            "-p": "none",
            "-Q": ["cipher", "cipher-auth", "mac", "kex", "key", "key-cert", "key-plain", "sig", "protocol-version"],
            "-R": "none",
            "-W": "none",
            "-w": "none",
        },
        // The host, then the command to run there.
        args: ["hosts", "none"],
    }
}

fn scp() {
    #{
        opts: ssh_opts() + `
            -3  copy between two remote hosts through the local host
            -B  batch mode (don't ask for passwords)
            -D SFTP_SERVER_PATH  connect directly to a local SFTP server program
            -l LIMIT  limit the used bandwidth, in Kbit/s
            -O  use the legacy SCP protocol instead of SFTP
            -P PORT  the port to connect to on the remote host
            -p  preserve modification times, access times and modes
            -R  copy from the remote host to the other one directly
            -r  copy directories recursively
            -S PROGRAM  the program to use for the encrypted connection
            -s  use the SFTP protocol (the default)
            -T  disable strict filename checking
        `,
        values: ssh_values() + #{"-l": "none", "-P": "none", "-S": "none"},
        args: ["remote"],
    }
}

fn sftp() {
    #{
        opts: ssh_opts() + `
            -a  attempt to continue interrupted transfers
            -B BUFFER_SIZE  the size of the buffer for transfers
            -b BATCHFILE  read commands from BATCHFILE
            -D SFTP_SERVER_PATH  connect directly to a local SFTP server program
            -f  flush files to disk after transfer
            -l LIMIT  limit the used bandwidth, in Kbit/s
            -N  disable quiet mode
            -P PORT  the port to connect to on the remote host
            -p  preserve modification times, access times and modes
            -R NUM_REQUESTS  the number of requests that may be outstanding
            -r  copy directories recursively
            -S PROGRAM  the program to use for the encrypted connection
            -s SUBSYSTEM  the SSH2 subsystem or the path of the sftp server on the remote host
        `,
        values: ssh_values() + #{"-B": "none", "-l": "none", "-P": "none", "-R": "none", "-S": "none", "-s": "none"},
        args: ["hosts", "none"],
    }
}

fn pgrep(kill) {
    let opts = `
        -e, --echo  display what is killed
        -c, --count  count of matching processes
        -f, --full  use the full process name to match
        -g, --pgroup=PGID  match listed process group IDs
        -G, --group=GID  match real group IDs
        -i, --ignore-case  match case insensitively
        -n, --newest  select most recently started
        -o, --oldest  select least recently started
        -O, --older=SECS  select where older than seconds
        -P, --parent=PPID  match only child processes of the given parent
        -s, --session=SID  match session IDs
        -t, --terminal=TTY  match by controlling terminal
        -u, --euid=ID  match by effective IDs
        -U, --uid=ID  match by real IDs
        -x, --exact  match exactly with the command name
        -F, --pidfile=FILE  read PIDs from file
        -L, --logpidfile  fail if PID file is not locked
        -r, --runstates=STATE  match runstates [D,S,Z,...]
        -A, --ignore-ancestors  exclude our ancestors from results
        --cgroup=GRP  match by cgroup v2 names
        --ns=PID  match the processes that belong to the same namespace as PID
        --nslist=NS  list which namespaces will be considered for the --ns option
        -h, --help  display help and exit
        -V, --version  output version information and exit
    `;
    opts += if kill {
        `
            --signal=SIGNAL  signal to send (either number or name)
            --inverse  negate the matching
            -H, --require-handler  match only if signal handler is present
            -q, --queue=VALUE  integer value to be sent with the signal
        `
    } else {
        `
            -d, --delimiter=DELIM  specify output delimiter
            -l, --list-name  list PID and process name
            -a, --list-full  list PID and full command line
            -v, --inverse  negate the matching
            -w, --lightweight  list all TID
        `
    };
    #{
        opts: opts,
        values: #{
            "--pgroup": "none",
            "--group": "groups",
            "--older": "none",
            "--parent": "pids",
            "--session": "none",
            "--terminal": "none",
            "--euid": "users",
            "--uid": "users",
            "--runstates": "none",
            "--cgroup": "none",
            "--ns": "pids",
            "--nslist": ["ipc", "mnt", "net", "pid", "user", "uts"],
            "--signal": "signals",
            "--queue": "none",
            "--delimiter": "none",
        },
        args: ["processes"],
    }
}

fn killall() {
    #{
        opts: `
            -e, --exact  require exact match for very long names
            -I, --ignore-case  case insensitive process name match
            -g, --process-group  kill process group instead of process
            -y, --younger-than=TIME  kill processes younger than TIME
            -o, --older-than=TIME  kill processes older than TIME
            -i, --interactive  ask for confirmation before killing
            -l, --list  list all known signal names
            -q, --quiet  don't print complaints
            -r, --regexp  interpret NAME as an extended regular expression
            -s, --signal=SIGNAL  send this signal instead of SIGTERM
            -u, --user=USER  kill only process(es) running as USER
            -v, --verbose  report if the signal was successfully sent
            -V, --version  display version information
            -w, --wait  wait for processes to die
            -n, --ns=PID  match processes that belong to the same namespaces as PID
        `,
        values: #{
            "--younger-than": "none",
            "--older-than": "none",
            "--signal": "signals",
            "--user": "users",
            "--ns": "pids",
        },
        args: ["processes"],
    }
}

fn spec(cmd) {
    switch cmd {
        "cat" => cat(),
        "ls" => ls(),
        "cp" => cp(),
        "mv" => mv(),
        "rm" => rm(),
        "mkdir" => mkdir(),
        "ln" => ln(),
        "touch" => touch(),
        "chmod" => chmod(),
        "chown" => chown(),
        "chgrp" => chgrp(),
        "head" => head(),
        "tail" => tail(),
        "wc" => wc(),
        "sort" => sort(),
        "uniq" => uniq(),
        "cut" => cut(),
        "du" => du(),
        "df" => df(),
        "stat" => stat(),
        "tee" => tee(),
        "readlink" => readlink(),
        "realpath" => realpath(),
        "basename" => basename(),
        "dirname" => dirname(),
        "date" => date(),
        "dd" => dd(),
        "install" => install(),
        "mktemp" => mktemp(),
        "nl" => nl(),
        "od" => od(),
        "paste" => paste(),
        "comm" => comm(),
        "join" => join(),
        "tr" => tr(),
        "seq" => seq(),
        "shuf" => shuf(),
        "split" => split(),
        "tac" => tac(),
        "truncate" => truncate(),
        "md5sum" | "sha1sum" | "sha224sum" | "sha256sum" | "sha384sum" | "sha512sum" | "b2sum" => checksum(),
        "base64" | "base32" => base64(),
        "shred" => shred(),
        "rmdir" => rmdir(),
        "uname" => uname(),
        "id" => id(),
        "fold" => fold(),
        "cmp" => cmp(),
        "diff3" => diff3(),
        "sdiff" => sdiff(),
        "grep" | "egrep" | "fgrep" => grep(),
        "diff" => diff(),
        "tar" => tar(),
        "make" => make(),
        "rsync" => rsync(),
        "man" => man(),
        "ssh" => ssh(),
        "scp" => scp(),
        "sftp" => sftp(),
        "pkill" => pgrep(true),
        "pgrep" => pgrep(false),
        "killall" => killall(),
        _ => #{},
    }
}
