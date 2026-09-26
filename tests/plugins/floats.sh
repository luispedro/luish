# Rhai's floating-point numbers are available.
cat > f.rhai <<'P'
let started = timestamp();
print(1.5 * 2.0);
print(sqrt(16.0));
print(to_int(7.9));
print(started.elapsed >= 0.0);
P
__luish_internal plugin load ./f.rhai
