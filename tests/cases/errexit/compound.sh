# set -e: only simple commands, subshells and pipelines exit on their own
# status (dash's checkexit); compound commands do not.
for c in '{ test no = yes && echo hi; }' 'if true; then false && true; fi' 'for i in 1; do false && true; done' 'case x in x) false && true;; esac' 'while true; do false && true; break; done' '(false && true)' 'f() { false && true; }; f' '{ false && true; } >/dev/null' '{ :; } >/nonexistent/x' 'false && true | cat' '{ false; }' 'if false; then :; fi' 'until false && true; do break; done' '! true'; do
  printf '%s => ' "$c"; $SH -ec "$c; echo reached \$?" 2>/dev/null; echo "exit $?"
done
