# As in dash, only ! negates a bracket expression; ^ is an ordinary member.
touch _G _^
echo _[^z] _[!z]
echo _[^\[z] _[^\]z] _[^[z]
case G in [^z]) echo neg;; *) echo no;; esac
case ^ in [^z]) echo member;; esac
x='[foo^]'
echo "${x#*[^o]}" "${x%[^f]*}"
