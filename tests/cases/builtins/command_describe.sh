# command -v/-V and type follow dash's describe_command: command -v/-V
# describe only the first name, "not found" goes to stdout, a path is found
# if the file exists, hashed commands are "tracked aliases", and command -p
# uses the default PATH (for lookup only).
myfunc() { echo x; }
alias ll='ls -l'
command -v echo myfunc ZZZ for; echo "status=$?"
command -v ZZZ echo; echo "status=$?"
command -V nonexistent; echo "status=$?"
command -V ll echo myfunc for :; echo "status=$?"
command -v ll; command -v :; command -v if
mkdir -p d
touch d/non-executable d/executable
chmod +x d/executable
command -v d/non-executable; echo "status=$?"
command -v d/executable; echo "status=$?"
command -v d/missing; echo "status=$?"
type d/non-executable d/executable d/missing; echo "status=$?"
type cat; cat </dev/null; type cat; command -V cat; command -pV cat; command -v cat
hash | grep -c cat
type myfunc echo : for ll
PATH=/nowhere command -pv ls
PATH=/nowhere command -p ls -d d
PATH=/nowhere; command -p env | grep -c '^PATH=/nowhere$'
