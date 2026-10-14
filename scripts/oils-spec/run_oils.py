#!/usr/bin/env python3
"""Runs the Oils spec tests under luish and other shells, and writes the results as JSON.

Usage: run_oils.py OILS_DIR OUT.json LABEL=PATH... [-- FILE...]

LABEL is luish, dash, bash or zsh (analyse.py and show.py expect these four).
FILEs are names in OILS_DIR/spec (all *.test.sh if none). Each case runs as
Oils' own runner runs it: on standard input, in a fresh directory that is also
$TMP, with spec/bin first in PATH, LC_ALL=C.UTF-8, REPO_ROOT and SH (the full
path of the shell), and a timeout of 5 s. In the output, the temporary
directory's name is replaced by $TMP.
"""
import json, os, re, shutil, signal, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor

oils = os.path.abspath(sys.argv[1])
out = sys.argv[2]
rest = sys.argv[3:]
if '--' in rest:
    i = rest.index('--')
    shells, files = rest[:i], rest[i + 1:]
else:
    shells, files = rest, None
shells = [s.split('=', 1) for s in shells]
spec = os.path.join(oils, 'spec')
if not files:
    files = sorted(f for f in os.listdir(spec) if f.endswith('.test.sh'))

# Oils' helpers (spec/bin/argv.py ...) want python2: a python2 that is python3
# prints the same for ASCII arguments, and the same for every shell.
tmproot = tempfile.mkdtemp(prefix='oils-spec-')
shim = os.path.join(tmproot, 'shim')
os.makedirs(shim)
os.symlink(shutil.which('python3') or '/usr/bin/python3', os.path.join(shim, 'python2'))
PATH = f'{spec}/bin:{shim}:/usr/bin:/bin'

BLOCK = re.compile(r'^## (?:(OK|BUG|N-I) ([\w./-]+) )?(STDOUT|STDERR):\s*$')
LINE = re.compile(r'^## (?:(OK|BUG|N-I) ([\w./-]+) )?(stdout|stderr|stdout-json|stderr-json|status):\s?(.*)$')


def parse(path):
    lines = open(path, 'rb').read().decode('utf-8', 'surrogateescape').split('\n')
    header, cases, cur, block = {}, [], None, None
    for ln in lines:
        if block is not None:
            if ln.startswith('## END'):
                key, buf = block
                cur['expect'].setdefault(key, {})[key[2]] = ''.join(l + '\n' for l in buf)
                block = None
            else:
                block[1].append(ln)
            continue
        if ln.startswith('#### '):
            cur = {'desc': ln[5:].strip(), 'code': [], 'expect': {}}
            cases.append(cur)
            continue
        if cur is None:
            m = re.match(r'^## (\w+): (.*)$', ln)
            if m:
                header[m.group(1)] = m.group(2).strip()
            continue
        m = BLOCK.match(ln)
        if m:
            block = ((m.group(1), m.group(2), m.group(3).lower()), [])
            continue
        m = LINE.match(ln)
        if m:
            kind, who, what, val = m.groups()
            key = (kind, who, None)
            if what == 'status':
                v = int(val)
            elif what.endswith('-json'):
                v = json.loads(val)
                what = what[:-5]
            else:
                v = val + '\n'
            cur['expect'].setdefault(key, {})[what] = v
            continue
        if ln.startswith('## '):
            continue
        cur['code'].append(ln)
    for c in cases:
        c['code'] = '\n'.join(c['code']).rstrip('\n') + '\n'
    return header, cases


def expected(case, label):
    """Expected stdout/status for a shell label (None for the default)."""
    res = {'stdout': None, 'status': 0}
    for (kind, who, _), vals in case['expect'].items():
        if kind is None:
            for k in ('stdout', 'status'):
                if k in vals:
                    res[k] = vals[k]
    if label:
        for (kind, who, _), vals in case['expect'].items():
            if kind and label in who.split('/'):
                for k in ('stdout', 'status'):
                    if k in vals:
                        res[k] = vals[k]
    return res


def run(label, sh, code, tmp):
    os.makedirs(tmp)
    env = {'PATH': PATH, 'LC_ALL': 'C.UTF-8', 'REPO_ROOT': oils, 'TMP': tmp, 'SH': sh}
    p = subprocess.Popen([sh], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         cwd=tmp, env=env, start_new_session=True)
    try:
        o, e = p.communicate(code.encode('utf-8', 'surrogateescape'), timeout=5)
        st = p.returncode
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        o, e = p.communicate()
        st = 'timeout'
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    shutil.rmtree(tmp, ignore_errors=True)
    if isinstance(st, int) and st < 0:
        st = 128 - st
    o = o.decode('utf-8', 'surrogateescape').replace(tmp, '$TMP')
    return {'stdout': o, 'status': st, 'stderr': e.decode('utf-8', 'surrogateescape')[:2000]}


def job(arg):
    fname, n, case = arg
    res = {}
    for label, sh in shells:
        res[label] = run(label, sh, case['code'], os.path.join(tmproot, f'{fname}-{n}-{label}'))
    return res


work, meta = [], []
for f in files:
    header, cases = parse(os.path.join(spec, f))
    for n, c in enumerate(cases):
        work.append((f, n, c))
        meta.append({'file': f, 'n': n, 'desc': c['desc'], 'code': c['code'],
                     'compare': header.get('compare_shells', ''),
                     'expect': {lab: expected(c, lab) for lab in [None, 'dash', 'bash', 'zsh']}})

with ThreadPoolExecutor(max_workers=os.cpu_count()) as ex:
    for m, r in zip(meta, ex.map(job, work)):
        m['results'] = r

with open(out, 'w') as f:
    json.dump(meta, f, indent=0, default=str)
shutil.rmtree(tmproot, ignore_errors=True)
print(f'{len(meta)} cases', file=sys.stderr)
