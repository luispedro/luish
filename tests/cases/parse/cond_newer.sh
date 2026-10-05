# In `[[ ... ]]`, as in `test` and bash, a file that exists is newer than
# one that doesn't (in zsh, both files of -nt and -ot must exist).
touch -d '2020-01-01 00:00:00.1' old
touch -d '2020-01-01 00:00:00.2' new
for e in 'old -nt nonexistent' 'nonexistent -nt old' 'nonexistent -nt nonexistent2' \
    'old -ot nonexistent' 'nonexistent -ot old' 'nonexistent -ot nonexistent2' \
    'new -nt old' 'old -nt new' 'old -nt old' 'old -ot new' 'new -ot old'; do
  eval "[[ $e ]]" && echo "$e: true" || echo "$e: false"
done
