# A here-document in $(...) in a subscript, with no end: the subscript is
# read again as a bad substitution, which took time exponential in how
# many there are. A syntax error at the end (as in dash).
echo start
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
pr "${a[$(echo <<E
