# `__luish_internal savestate` sets bareglobqual before the functions, so
# that functions with glob qualifiers read back, even if the option was
# turned off after they were defined. (Whether the qualifier is used
# depends on the option when the function runs.)
mkdir d; touch file
setopt bareglobqual
f() { echo *(/); }
unsetopt bareglobqual
__luish_internal savestate > state
$SH -c '. ./state; f; unsetopt | grep bareglobqual'
grep -e bareglobqual -e '^f()' state
setopt bareglobqual
g() { echo *(.N); }
__luish_internal savestate > state
$SH -c '. ./state; f; g; setopt | grep bareglobqual'
grep -e bareglobqual -e '^[fg]()' state
