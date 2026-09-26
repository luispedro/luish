i=0; s=
while [ $i -lt 13 ]; do s="$s$s$i"; i=$((i+1)); done
cat <<EOF | wc -c
$s
EOF
