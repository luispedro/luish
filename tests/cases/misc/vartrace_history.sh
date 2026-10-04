# `vars.trace_history` keeps the last 100 changes of each variable, with
# their values; `vars.trace` keeps them only for PATH, MANPATH, PS1, RPROMPT
# and RPS1, and the last change of the others.
cat > h.sh <<'X'
P=1
P=2
PATH=/x:$PATH
PATH=$PATH:/y
unset P
P=3
where -a P PATH
i=0
while [ "$i" -lt 105 ]; do i=$((i + 1)); done
where -a i | sed -n '1,2p;$p'
where -a i | wc -l
X
env PATH=/usr/bin:/bin $SH -o vars.trace ./h.sh
echo "-- history"
env PATH=/usr/bin:/bin $SH -o vars.trace_history ./h.sh
# Turning history on keeps what was recorded.
$SH -o vars.trace -c 'P=1; P=2; setopt vars.trace_history; P=3; where -a P'
