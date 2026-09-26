# echo, printf %b and printf formats follow dash's escapes (including
# Debian's \e), and printf's numeric conversions follow dash's getuintmax.
echo "\e|\1|\01|\001|\0001|\18|\8|\0|\c" | od -An -c
printf '%b\n' '\e|\1|\01|\141|\0141|\18|\558|\0|\"' | od -An -c
printf '\e|\1|\141|\0141|\558|\"|\q\n' | od -An -c
printf '%b|%s\n' 'a\cb' z; echo
printf '%z' 2>/dev/null; echo " st $?"
printf 'a%-zb' 2>/dev/null; echo " st $?"
printf '%d %u %x %o\n' 18446744073709551615 18446744073709551615 -1 -1 2>/dev/null; echo "st $?"
printf '%d\n' 9223372036854775808 -9223372036854775809 2>/dev/null; echo "st $?"
printf '%u\n' -1 -18446744073709551615 -18446744073709551616 2>/dev/null; echo "st $?"
printf '%x %X %o\n' 18446744073709551615 0x10 010; echo "st $?"
printf '-%s\n' a 2>/dev/null; echo "status $?"
printf -v x 2>/dev/null; echo "status $?"
printf -- '-%s\n' b
