command shift 5 2>/dev/null; echo "survived $?"
readonly r=1; command export r=2 2>/dev/null; echo "survived2 $?"
