# Extension code has no size limits, but a call that raises the shell's
# peak memory use by more than 256 MiB gets a warning (once: the peak only
# grows). Building a long list is linear (with Rhai's size limits it was
# quadratic, and stopped at 2^20 elements).
cat > m.rhai <<'P'
sh::builtin("list", |argv| { let a = []; for i in 0..200000 { a.push(i); } a.len - 200000 });
sh::builtin("grow", |argv| { let s = "x"; for i in 0..28 { s += s; } 0 });
P
__luish_internal plugin load ./m.rhai
list; echo "list: $?"
grow 2>err; echo "grow: $?"
sed -n 's/[0-9][0-9]* MiB/N MiB/g; s/^[^:]*: [0-9]*: //p' err
grow 2>err; echo "again: $?"
cat err
