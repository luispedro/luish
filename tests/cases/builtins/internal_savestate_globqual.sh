# `__luish_internal savestate` sets glob.bare_qualifiers before the functions, so
# that functions with glob qualifiers read back, even if the option was
# turned off after they were defined. (Whether the qualifier is used
# depends on the option when the function runs.)
mkdir d; touch file
setopt bareglobqual
f() { echo *(/); }
unsetopt glob.bare_qualifiers
__luish_internal savestate > state
$SH -c '. ./state; f; unsetopt | grep bare_qualifiers'
grep -e bare_qualifiers -e '^f()' state
setopt bareglobqual
g() { echo *(.N); }
__luish_internal savestate > state
$SH -c '. ./state; f; g; setopt | grep bare_qualifiers'
grep -e bare_qualifiers -e '^[fg]()' state
