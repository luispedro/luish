mkdir d; touch d/a.txt d/b.txt d/c.log d/.hidden
cd d
echo *.txt
echo *
echo .*
echo ?.log
echo [ab].txt [!a].txt
echo nomatch*
echo "*.txt" '*'
x='*.txt'; echo $x "$x"
set -f; echo *; set +f
echo */ 2>/dev/null
cd ..; echo d/*.txt
echo d/[[:alpha:]].log
echo [ a[ [x '[' "[*"
touch '[' 'a[b'; echo [ a[* [[]
