parse() {
  OPTIND=1
  while getopts "ab:c" opt "$@"; do
    case $opt in
      a) echo "a" ;;
      b) echo "b=$OPTARG" ;;
      c) echo "c" ;;
      \?) echo "bad" ;;
    esac
  done
  shift $((OPTIND-1)); echo "rest: $*"
}
parse -a -b val -c file1 file2
parse -ab x -- -a
parse -acbval rest
parse -z 2>/dev/null
parse
while getopts ":x:" o -x; do echo "$o $OPTARG"; done
