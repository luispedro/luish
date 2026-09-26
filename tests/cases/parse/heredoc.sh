x=world
cat <<EOF
hello $x
  indented \$x `echo cmd`
EOF
cat <<'EOF'
literal $x `echo no`
EOF
cat <<-EOF
	tab stripped $x
		two tabs
	EOF
cat <<A; cat <<B
first
A
second
B
y=$(cat <<EOF
inside $x
EOF
)
echo "$y"
cat <<"E O F"
quoted delim
E O F
cat << EOF | tr a-z A-Z
piped
EOF
cat <<EOF
backslash\
continued
EOF
