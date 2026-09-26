echo "$-"
set -a; echo "$-"; x=1; sh -c 'echo $x'; set +a
set -C -u; echo "$-"
