//! Pathname expansion.

use super::pattern::{Pattern, has_meta};
use super::split::{XChar, bytes};
use crate::sys;

/// Expands a pattern into matching paths, sorted in byte order. Returns an
/// empty list if nothing matches.
pub fn glob(pat: &[XChar]) -> Vec<Vec<u8>> {
    let comps: Vec<&[XChar]> = pat.split(|c| c.b == b'/').collect();
    let mut out = Vec::new();
    let (prefix, comps) = if pat.first().is_some_and(|c| c.b == b'/') {
        (b"/".to_vec(), &comps[1..])
    } else {
        (Vec::new(), &comps[..])
    };
    expand(prefix, comps, false, &mut out);
    out.sort();
    out
}

fn expand(prefix: Vec<u8>, comps: &[&[XChar]], matched: bool, out: &mut Vec<Vec<u8>>) {
    let Some((comp, rest)) = comps.split_first() else {
        if !matched || sys::lstat(&prefix).is_some() {
            out.push(prefix);
        }
        return;
    };
    let join = |prefix: &[u8], name: &[u8], last: bool| {
        let mut p = prefix.to_vec();
        p.extend_from_slice(name);
        if !last {
            p.push(b'/');
        }
        p
    };
    if !has_meta(comp) {
        let p = join(&prefix, &bytes(comp), rest.is_empty());
        // An empty last component comes from a trailing slash.
        expand(p, rest, matched, out);
        return;
    }
    let dir = if prefix.is_empty() {
        b".".to_vec()
    } else {
        prefix.clone()
    };
    let Some(mut names) = sys::read_dir(&dir) else {
        return;
    };
    names.sort();
    let pattern = Pattern::new(comp);
    let dot_ok = pattern.starts_with_dot();
    for name in names {
        if name[0] == b'.' && !dot_ok {
            continue;
        }
        if !pattern.matches(&name) {
            continue;
        }
        let p = join(&prefix, &name, rest.is_empty());
        if !rest.is_empty() && !sys::is_dir(&p[..p.len() - 1]) {
            continue;
        }
        expand(p, rest, true, out);
    }
}
