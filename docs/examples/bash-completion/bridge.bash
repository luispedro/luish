# Completes a command's arguments with bash-completion, for the plugin in
# plugin.rhai:
#
#   bash bridge.bash INDEX WORD...
#
# The words are the command's, unquoted, starting with its name. The one at
# INDEX (counting from 0) is being completed, and ends at the cursor.
#
# Prints the start of that word that the matches leave alone (up to its last
# `=` or `:`), then the matches, one per line, each followed by a tab if no
# space should follow it. Exits with 1 if bash-completion has nothing for the
# command, and with 2 if the command asks for filenames.

for f in "${BASH_COMPLETION_SCRIPT-}" \
    /usr/share/bash-completion/bash_completion \
    /usr/local/share/bash-completion/bash_completion \
    /opt/homebrew/share/bash-completion/bash_completion \
    "$HOME/.nix-profile/share/bash-completion/bash_completion"; do
    [[ -n $f && -r $f ]] && break
done
source "$f" || exit 1

index=$1
shift
cmd=$1

# Bash splits the line into COMP_WORDS at the characters of COMP_WORDBREAKS
# too, so `--opt=val` is `--opt`, `=` and `val`. Of those, only `=` and `:`
# can be in a word here (the others are quotes or end words).
COMP_WORDS=()
COMP_LINE=
for ((i = 0; i < $#; i++)); do
    w=${@:i+1:1}
    COMP_LINE+=${COMP_LINE:+ }$w
    if ((i == index)); then
        cur=$w
        COMP_POINT=${#COMP_LINE}
    fi
    [[ -z $w ]] && COMP_WORDS+=("")
    while [[ -n $w ]]; do
        if [[ $w == [=:]* ]]; then
            t=${w%%[!=:]*}
        else
            t=${w%%[=:]*}
        fi
        COMP_WORDS+=("$t")
        w=${w#"$t"}
    done
    ((i == index)) && COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
done
COMP_TYPE=9
COMP_KEY=9
prefix=${cur%"${cur##*[=:]}"}

spec=$(complete -p -- "$cmd" 2>/dev/null)
if [[ -z $spec ]]; then
    if declare -F _comp_load >/dev/null; then
        _comp_load -- "$cmd"
    else
        __load_completion "$cmd"
    fi
    spec=$(complete -p -- "$cmd" 2>/dev/null) || exit 1
fi

# The options (`-o`), the function (`-F`), the command (`-C`), and what is
# left for compgen.
opts=()
func=
command=
args=()
eval "set -- ${spec#complete }"
while (($# > 1)); do
    case $1 in
    -o) opts+=("$2"); shift ;;
    -F) func=$2; shift ;;
    -C) command=$2; shift ;;
    -[AGWXPS]) args+=("$1" "$2"); shift ;;
    -D | -E | -I | --) ;;
    *) args+=("$1") ;;
    esac
    shift
done

# The builtin works only while bash is completing.
compopt() {
    while (($#)); do
        case $1 in
        -o) opts+=("$2"); shift ;;
        +o) for i in "${!opts[@]}"; do [[ ${opts[i]} == "$2" ]] && unset 'opts[i]'; done; shift ;;
        esac
        shift
    done
}

has() {
    local o
    for o in "${opts[@]}"; do [[ $o == "$1" ]] && return; done
    return 1
}

word=${COMP_WORDS[COMP_CWORD]}
prev=${COMP_WORDS[COMP_CWORD-1]}
COMPREPLY=()
((${#args[@]})) && mapfile -t COMPREPLY < <(compgen "${args[@]}" -- "$word")
[[ -n $func ]] && "$func" "$cmd" "$word" "$prev"
[[ -n $command ]] && mapfile -t -O "${#COMPREPLY[@]}" COMPREPLY < <(eval "$command" '"$cmd" "$word" "$prev"')
if ((${#COMPREPLY[@]} == 0)) && has dirnames; then
    mapfile -t COMPREPLY < <(compgen -d -- "$word")
    opts+=(filenames)
fi
if ((${#COMPREPLY[@]} == 0)) && { has default || has bashdefault; }; then
    exit 2
fi

printf '%s\n' "$prefix"
declare -A seen
for m in "${COMPREPLY[@]}"; do
    [[ -n ${seen[x$m]-} ]] && continue
    seen[x$m]=1
    if [[ $m == *' ' ]]; then
        printf '%s\n' "${m% }"
    elif has filenames && [[ -d ${m/#\~/$HOME} ]]; then
        printf '%s/\t\n' "${m%/}"
    elif has nospace; then
        printf '%s\t\n' "$m"
    else
        printf '%s\n' "$m"
    fi
done
