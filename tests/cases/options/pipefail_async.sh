# set -o pipefail applies to background pipelines as it was when they
# started, as POSIX requires and bash does (zsh ignores it for them).
set -o pipefail
false | true & set +o pipefail; wait $!; echo "started on $?"
(exit 3) | (exit 2) | true & wait $!; echo "started off $?"
set -o pipefail
(exit 3) | (exit 2) | true & wait $!; echo "rightmost $?"
true | true & wait $!; echo "none $?"
# A job's `wait` status with pipefail, found by job number.
false | true &
wait %1; echo "job $?"
# A single command in the background: as always.
(exit 5) & wait $!; echo "single $?"
