# ${x#pat} and friends with patterns of fixed and minimum lengths.
s=abcabc
for p in '?' '??' 'a?c' 'abc' '*' 'a*' '*c' '*b*' '[ab]' '[!a]?' '???????' '' 'x'; do
	printf '%s: %s %s %s %s\n' "$p" "${s#$p}" "${s##$p}" "${s%$p}" "${s%%$p}"
done
e=
printf '[%s] [%s] [%s] [%s]\n' "${e#?}" "${e##*}" "${e%?}" "${e%%}"
q='a*'; printf '%s %s\n' "${s#"$q"}" "${s#$q}"
t='a*c'; printf '%s %s\n' "${t#"a*"}" "${t%"*c"}"
b='a\bc'; printf '%s %s\n' "${b#a\\}" "${b%\\bc}"
x=abc; printf '%s\n' "${x#$((x=1))}" "$x"
