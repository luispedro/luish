# sh::expand_prompt expands `%` sequences as print -P does, whether or not
# prompt.percent is on; parameters are not expanded.
mkdir -p a/b
cd a/b
cat > p.rhai <<'P'
print(sh::expand_prompt("[%~|%1~|%%|%(?.ok.bad)|$x]"));
let c = sh::expand_prompt("%F{red}r%f");
c.replace("\x1b", "^[");
print(c);
print(sh::expand_prompt("%[bogus]x"));
P
__luish_internal plugin load ./p.rhai 2>&1 | sed "/^  /d; s/.*: unknown/unknown/; s/;.*//"
