#!/bin/sh
# Runs a fuzz target (DEVELOPING.md, Fuzzing): fuzz/run.sh TARGET [SECONDS]
# [LIBFUZZER-OPTION...]. Without SECONDS it runs until stopped. New inputs
# go to fuzz/corpus/TARGET, crashes to fuzz/artifacts/TARGET.
#
# The seeds are the differential tests for the targets that read scripts,
# and fuzz/seeds/TARGET for the others.
set -e
cd "$(dirname "$0")/.."
target=${1:?usage: fuzz/run.sh TARGET [SECONDS] [LIBFUZZER-OPTION...]}
shift
time=
if [ $# -gt 0 ]; then
    time=-max_total_time=$1
    shift
fi
case $target in
parse | unparse | highlight) seeds=tests/cases ;;
*) seeds=fuzz/seeds/$target ;;
esac
mkdir -p fuzz/corpus/$target
exec cargo +nightly fuzz run "$target" fuzz/corpus/$target "$seeds" -- -max_len=4096 $time "$@"
