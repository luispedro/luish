# A file that exists is newer than one that doesn't, as in POSIX.1-2024 and Debian's dash
# 0.5.12-12 (whose patch Ubuntu 24.04's dash, used by CI, lacks; hence the .expected file).
touch f
[ f -nt nonexistent ]; echo $?
[ nonexistent -ot f ]; echo $?
[ nonexistent -nt f ]; echo $?
[ f -ot nonexistent ]; echo $?
[ nonexistent -nt nonexistent2 ]; echo $?
[ nonexistent -ot nonexistent2 ]; echo $?
