#!/usr/bin/env python3
"""Summarises the output of run_oils.py by spec file.

Usage: analyse.py RESULTS.json dash|other [-v]

dash: the files whose compare_shells list dash; a case differs if luish's
stdout or status isn't dash's. other: the other files; a case differs if
luish matches neither bash nor zsh nor the expected (default) output.
-v prints each case that differs, with the output of every shell.
"""
import json, sys
from collections import Counter

data = json.load(open(sys.argv[1]))
mode = sys.argv[2]
verbose = '-v' in sys.argv


def same(a, b):
    return a['stdout'] == b['stdout'] and a['status'] == b['status']


def spec_ok(r, e):
    return (e['stdout'] is None or r['stdout'] == e['stdout']) and r['status'] == e['status']


bad = []
cnt = Counter()
for c in data:
    has_dash = 'dash' in c['compare'].split()
    if (mode == 'dash') != has_dash:
        continue
    r = c['results']
    L = r['luish']
    cnt['total'] += 1
    if mode == 'dash':
        if same(L, r['dash']):
            cnt['same'] += 1
            continue
        bad.append(c)
    else:
        if any(same(L, r[s]) for s in ('bash', 'zsh')):
            cnt['same as bash or zsh'] += 1
            continue
        if spec_ok(L, c['expect']['null']):
            cnt['spec default'] += 1
            continue
        bad.append(c)

print(dict(cnt), 'differ:', len(bad))
byfile = Counter(c['file'] for c in bad)
for f, n in byfile.most_common():
    print(f'{n:4} {f}')
if verbose:
    for c in bad:
        print('=' * 70)
        print(f"{c['file']} #{c['n']}: {c['desc']}")
        print(c['code'].rstrip())
        for s in ('luish', 'dash', 'bash', 'zsh'):
            x = c['results'][s]
            print(f'--- {s}: status {x["status"]}')
            print(x['stdout'][:600].rstrip())
            if s == 'luish' and x['stderr']:
                print('    stderr:', x['stderr'][:300].rstrip())
