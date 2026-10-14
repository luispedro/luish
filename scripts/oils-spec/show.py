#!/usr/bin/env python3
"""Prints cases side by side: the code, and each shell's status and stdout
(and luish's stderr).

Usage: show.py RESULTS.json FILE#N...   (e.g. alias.test.sh#26)
"""
import json, sys
data = json.load(open(sys.argv[1]))
want = set(a for a in sys.argv[2:])
for c in data:
    if f"{c['file']}#{c['n']}" not in want: continue
    print('=' * 70)
    print(f"{c['file']} #{c['n']}: {c['desc']}")
    print(c['code'].rstrip())
    for s in ('luish', 'dash', 'bash', 'zsh'):
        x = c['results'][s]
        print(f'--- {s}: status {x["status"]}')
        if x['stdout']: print(x['stdout'][:500].rstrip())
        if s in ('luish',) and x['stderr']: print('    stderr:', x['stderr'][:400].rstrip())
