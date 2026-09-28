# pipestatus (zsh) and PIPESTATUS (bash): the statuses of the last
# pipeline's commands. zsh's sh emulation has no pipestatus, so the output
# was checked against `zsh -f -o ksharrays`: every pipeline sets it, a
# compound command too, but not an assignment or `[[`.
echo "[${#pipestatus[@]}]"
true | (exit 3) | false
echo "${pipestatus[@]}" "${PIPESTATUS[@]}"
true | (exit 3)
echo $pipestatus ${pipestatus[1]} ${pipestatus[-1]} ${#pipestatus[@]} ${!pipestatus[@]} $((pipestatus[1] + 1))
false
echo "${pipestatus[*]}"
! true | false
echo "${pipestatus[@]}"
! false
echo "${pipestatus[@]}"
(exit 2) | true
st=$?
echo "$st ${pipestatus[@]}"
(exit 2) | true
x=$(exit 4)
echo "${pipestatus[@]}"
(exit 2) | true
[[ a = b ]]
echo "${pipestatus[@]}"
(exit 2) | true
false || echo "${pipestatus[@]}"
(exit 2) | true
if false | true; then :; fi
echo "${pipestatus[@]}"
{ false | true; }
echo "${pipestatus[@]}"
f() { true | (exit 5); }
f
echo "${pipestatus[@]}"
set -o pipefail
false | true
echo "$? ${pipestatus[@]}"
set +o pipefail
false | (exit 3) &
wait
echo "${pipestatus[@]}"
(exit 2) | true
echo "$(echo "${pipestatus[@]}")"
# Assigning makes it an ordinary variable, as for UID.
pipestatus=(7 8)
false | true
echo "${pipestatus[@]}" "${PIPESTATUS[@]}"
unset PIPESTATUS
true | false
echo "[${PIPESTATUS-unset}]"
