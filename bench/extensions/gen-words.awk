# Writes `lines` lines of about ten words drawn, with a skew towards the
# first, from a vocabulary of made-up words (deterministic).
BEGIN {
	split("b c d f g k l m n p r s t v z", C, " ")
	split("a e i o u", V, " ")
	seed = 12345
	for (i = 0; i < 3000; i++) {
		w = ""
		for (j = 0; j < 2 + i % 3; j++) w = w C[1 + int(rnd() * 15)] V[1 + int(rnd() * 5)]
		vocab[i] = w
	}
	for (l = 0; l < lines; l++) {
		n = 6 + int(rnd() * 9)
		line = ""
		for (k = 0; k < n; k++) {
			r = rnd()
			line = line (k ? " " : "") vocab[int(r * r * r * 3000)]
		}
		print line
	}
}
function rnd() {
	seed = (seed * 1103515245 + 12345) % 2147483648
	return int(seed / 65536) % 32768 / 32768
}
