# Unlike dash, `kill %n` signals the job's processes when the job was not
# started under job control (dash signals a nonexistent process group).
sleep 10 &
kill %1; echo "kill: $?"
wait %1; echo "wait: $?"
sleep 10 | sleep 10 &
kill %%; echo "kill: $?"
wait; echo "wait: $?"
kill %3; echo "no such job: $?"
