# setopt expand.braces: brace expansion, where luish and zsh agree (zsh's
# sh emulation turns it off, with ignore_braces).
# reference: zsh -o noignorebraces
[ -n "$ZSH_NAME" ] || setopt expand.braces
echo 1 f/{a,b} a{,b} {a,b}{1,2} X{a,b}Y{c,d}Z
echo 2 {a,{b,c}}d {x{a,b}} {a}{b,c} {{a,b} }{a,b}
echo 3 {a} {} {a,b {abc} {a-c} {aa..cc} {1.5..3}
echo 4 {0..1} {5..1} {-3..3} {1..10..3} {10..1..3} {1..1}
echo 5 {01..10} {1..010} {001..-3} {-05..5..5}
echo 6 {a..e} {e..a} {x..x} {Z..a}
echo 7 "{a,b}" '{a,b}' {a\,b,c} {"a b",c} {'a,b',c} {a..b,c}
x=b
echo 8 {a,$x} ${x}{1,2} {$(echo 1,2)} {a,b}=c
n=3
echo 9 {1..$n} {$((n - 2))..$n}
for i in {1..2}{a,b}; do echo 10 $i; done
y={a,b}
echo 11 $y
z='{a,b}'
echo 12 $z
case b in {a,b}) echo 13 yes ;; *) echo 13 no ;; esac
mkdir d1 d2
echo 14 d{1,2}* {1..2}*nomatch*
