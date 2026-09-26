# `getopts`

```text
getopts optstring name [argument...]
```

Parse options, one per call.

Each call stores the next option letter in the variable `name`, and its
argument, if it has one, in `OPTARG`. `OPTIND` is the index of the next
argument to look at, and starts at 1. The options are taken from the
arguments, or from the positional parameters if there are none. At the end
of the options, `name` is set to `?` and the exit status is 1.

`optstring` lists the option letters; a letter followed by `:` takes an
argument. An unknown option or a missing argument sets `name` to `?` and
prints a message. If `optstring` starts with `:`, nothing is printed:
`name` is set to `?` for an unknown option and to `:` for a missing
argument, and `OPTARG` is set to the option letter.

```sh
while getopts vo: opt; do
    case $opt in
        v) verbose=1 ;;
        o) output=$OPTARG ;;
        *) exit 2 ;;
    esac
done
shift $((OPTIND - 1))
```
