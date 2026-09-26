# setopt bareglobqual: zsh's glob qualifiers, `(...)` at the end of a
# word. zsh is run without ksharrays, which in sh emulation makes its
# subscripts start at 0. (In a directory of its own, since the test's own
# files are in the current one.)
# reference: zsh +o shglob -o bareglobqual +o ksharrays
[ -n "$ZSH_VERSION" ] || setopt bareglobqual
umask 022
mkdir t; cd t
mkdir -p d/sub .hid
touch x.md d/f9 d/f10 d/f100 .hid/h .dotfile
printf 1234567890 > big
printf %2000s '' > big2k
chmod +x d/f9
chmod 640 d/f10
ln -s d lnk
ln -s nowhere broken
mkfifo fifo
touch -m -d '2001-01-01 00:00:00' old
echo types: *(/) / *(.) / *(@) / *(p) / *(-/) / *(-.) / *(-@)
echo negation: *(^/) / *(^@) / *(^-@) / d/*(^*.)
echo alternatives: *(/,p) / *(.,@)
echo dots: *(D) / *(D/)
echo empty: nomatch*(N) end / nomatch*(.) / *(%)
echo marks: *(M) / *(-M) / *(T) / *(-T) / d/*(T)
echo perms: d/*(*) / d/*(r) / d/*(A) / d/*(^A) / d/*(R) / d/*(E,X)
echo owner: *(U) / *(^U)
echo size: *(L10) / *(L+5) / *(L-1.) / *(Lk1.) / *(Lk2.) / *(Lk-1.)
echo time: *(m+300) / *(^m+300) / *(Mm+3) / *(mh-1^@)
echo sort: d/*(.) / d/*(.n) / d/*(.on) / d/*(.On) / d/*(.nOn)
echo slices: d/*(n[1]) / d/*(n[-1]) / d/*(n[2,3]) / d/*(n[2,-1]) / d/*(n[5]) / d/*(n[-9,1])
echo modifiers: d/*(:t) / d/*(n:t:u) / *.md(:r) / *.md(:e) / d/*(n[1]:h) / x.md(:h)
echo plain: x.md(N) nothing(N) x.md(/N) x.md(/) end
x=d
echo expansions: $x/*(/) "$x"/*(.n) ${x}(N/)
y='*(/)'
echo from a variable: $y
IFS=:; w='d:x.md:none'; echo split: $w(N); unset IFS
echo in a field: $(echo *(/))
echo assignment: z=*(/) "$(z=*(/); echo $z)"
case '*(/)' in *'(/)') echo case: matches ;; esac
cat <<X
here-doc: *(/) $(echo *(/))
X
set -f; echo noglob: *(/) *(N); set +f
f() { echo function: *(/); }
f
g() { echo definitions: ok; }
g
(echo subshell: *(/))
echo arithmetic: $((2*(3+4)))
for f in d/*(.n); do printf '%s;' "$f"; done; echo

# Sorting by size, time and links, with times set explicitly. zsh leaves
# ties in no particular order, so no two files share a key: the extra hard
# links are outside the directory.
mkdir s; cd s
printf 1 > a; printf 123 > b; printf 12 > c
touch -m -d '2020-01-02' a; touch -m -d '2020-01-03' b; touch -m -d '2020-01-01' c
ln b ../../b.l; ln c ../../c.l1; ln c ../../c.l2
echo sort: *(oL) / *(OL) / *(om) / *(Om) / *(om[1]) / *(Ol) / *(ol)
cd ..

# With globstar.
[ -n "$ZSH_VERSION" ] || setopt globstar
echo recursive: **/*(.)
# Only one file per directory, since zsh sorts files at the same depth in
# no particular order.
mkdir -p o/p/q; touch o/1 o/p/2 o/p/q/3
echo depth: o/**/*(.od) / o/**/*(.Od)
echo recursive: **/*(/) / **/*(D/) / **/f*(*)

