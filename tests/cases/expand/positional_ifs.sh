# $@ and $* with -, +, :-, :+, # under unset, empty and non-empty IFS (dash rules).
c() { printf '%s' "$#"; [ $# -gt 0 ] && printf ':%s' "$@"; printf ' '; }
for args in 'set --' 'set -- "" ""' 'set -- a b'; do
for ifs in unset '' '-'; do
  eval "$args"
  if [ "$ifs" = unset ]; then unset IFS; else IFS=$ifs; fi
  printf '%-14s IFS=%-6s' "$args" "$ifs"
  c $@; c ${@-m}; c ${@+p}; c ${@:-m}; c ${@:+p}; c $*; c ${*-m}; c ${*+p}; c ${*:-m}; c ${*:+p}
  printf '| '
  c "$@"; c "${@-m}"; c "${@:-m}"; c "${@:+p}"; c "$*"; c "${*-m}"; c "${*:-m}"; c "${*:+p}"
  x=$@ y=${@:-m} z=${*:+p} w=${#*}; printf '| %s,%s,%s,%s\n' "$x" "$y" "$z" "$w"
done; done
unset IFS
# From pnut (Oils bug #2141): "$@" and $@ keep their fields with IFS empty.
res=0
sum() {
  set $@ $res
  res=$(($1 + $2))
  echo "$1 + $2 = $res"
  res=$3
}
sum 12 30
IFS=
sum 12 30
set -- a 'b c'
printf '[%s]\n' $@ $* "$@"
set -u
sum 12 30
