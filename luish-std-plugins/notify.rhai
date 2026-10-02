// A desktop notification when a command that took a while ends, with the
// terminal's OSC 777 (foot, GNOME Terminal and other VTE ones that have the
// patch for it, WezTerm, Ghostty, rxvt-unicode with its notify extension).
// $LUISH_NOTIFY_AFTER is the least time in seconds (10 by default).
// Terminals that notify by themselves from the prompt marks (kitty,
// Ghostty) don't need this.

let started = ();
let command = "";

sh::hook("preexec", |line| {
    started = timestamp();
    command = line;
});

sh::hook("precmd", |status| {
    if started == () {
        return;
    }
    let took = started.elapsed;
    started = ();
    let after = 10.0;
    try {
        after = parse_float(sh::getvar("LUISH_NOTIFY_AFTER") ?? "10");
    } catch {
    }
    if took < after || sh::run("[ -t 1 ]") != 0 {
        return;
    }
    // The text can't hold control characters, nor the title a `;`.
    let text = command;
    for c in ["\n", "\t", "\r"] {
        text.replace(c, " ");
    }
    for c in ["\x1b", "\x07"] {
        text.replace(c, "");
    }
    text.trim();
    if text.len > 80 {
        text = text.sub_string(0, 79) + "…";
    }
    let title = if status == 0 { "Done" } else { `Failed (status ${status})` };
    let body = `${text} (${took.to_int()} s)`;
    sh::write(1, "\x1b]777;notify;" + title + ";" + body + "\x07");
});
