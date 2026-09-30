# luish waits for the process of >(...) once the command is over (bash and zsh do not, so
# their output order varies), so its output comes before what follows.
echo one | tee >(cat -n) > /dev/null
echo after
echo two > >(tr a-z A-Z)
echo three > >(tr a-z A-Z)
echo end
