#!/usr/bin/env python3
"""Sorts the cases of the files whose compare_shells list dash where luish
differs from dash: ext (dash fails, luish does as bash or zsh), B-change (dash
succeeds, luish does as bash or zsh: a deviation) and C-none (as none of the
three).

Usage: classify.py RESULTS.json
"""
import json, sys
data = json.load(open(sys.argv[1]))
def same(a, b): return a['stdout'] == b['stdout'] and a['status'] == b['status']
cats = {}
for c in data:
    if 'dash' not in c['compare'].split(): continue
    r = c['results']; L = r['luish']
    if same(L, r['dash']): continue
    if any(same(L, r[s]) for s in ('bash', 'zsh')):
        k = 'ext' if r['dash']['status'] != 0 else 'B-change'
    else:
        k = 'C-none'
    cats.setdefault(k, []).append(c)
for k, v in sorted(cats.items()):
    print(k, len(v))
    for c in v: print(f"   {c['file']} #{c['n']}: {c['desc']}")
