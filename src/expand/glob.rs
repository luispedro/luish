//! Pathname expansion.

use super::pattern::{Pattern, has_meta};
use super::split::{XChar, bytes};
use crate::sys;

#[derive(Clone, Copy, Default)]
pub struct GlobOpts {
    /// `setopt globstar`: a `**/` component matches any number of
    /// directories (`***/` also follows symbolic links to directories).
    pub globstar: bool,
    /// Patterns match names with a leading `.` (except `.` and `..`), and
    /// `**/` enters hidden directories (the `D` glob qualifier).
    pub dots: bool,
}

/// Expands a pattern into matching paths, sorted in byte order. Returns an
/// empty list if nothing matches.
pub fn glob(pat: &[XChar], opts: GlobOpts) -> Vec<Vec<u8>> {
    let comps: Vec<&[XChar]> = pat.split(|c| c.b == b'/').collect();
    let (prefix, comps) = if pat.first().is_some_and(|c| c.b == b'/') {
        (b"/".to_vec(), &comps[1..])
    } else {
        (Vec::new(), &comps[..])
    };
    let mut g = Globber { opts, out: Vec::new() };
    g.expand(prefix, comps, false);
    g.out.sort();
    if opts.globstar {
        // Two `**/` can reach the same path in different ways.
        g.out.dedup();
    }
    g.out
}

/// Whether a component is `**` or `***`, unquoted.
fn is_globstar(comp: &[XChar]) -> bool {
    (comp.len() == 2 || comp.len() == 3) && comp.iter().all(|c| c.b == b'*' && !c.quoted)
}

fn join(prefix: &[u8], name: &[u8], last: bool) -> Vec<u8> {
    let mut p = Vec::with_capacity(prefix.len() + name.len() + 1);
    p.extend_from_slice(prefix);
    p.extend_from_slice(name);
    if !last {
        p.push(b'/');
    }
    p
}

struct Globber {
    opts: GlobOpts,
    out: Vec<Vec<u8>>,
}

impl Globber {
    fn expand(&mut self, prefix: Vec<u8>, comps: &[&[XChar]], matched: bool) {
        let Some((comp, rest)) = comps.split_first() else {
            if !prefix.is_empty() && (!matched || sys::lstat(&prefix).is_some()) {
                self.out.push(prefix);
            }
            return;
        };
        if self.opts.globstar && !rest.is_empty() && is_globstar(comp) {
            let follow = comp.len() == 3;
            let mut seen = Vec::new();
            if follow && let Some(st) = sys::stat(if prefix.is_empty() { b"." } else { &prefix }) {
                seen.push((st.st_dev, st.st_ino));
            }
            self.expand(prefix.clone(), rest, true);
            self.descend(prefix, rest, follow, &mut seen);
            return;
        }
        if !has_meta(comp) {
            let p = join(&prefix, &bytes(comp), rest.is_empty());
            // An empty last component comes from a trailing slash.
            self.expand(p, rest, matched);
            return;
        }
        let Some(mut names) = sys::read_dir(&prefix) else {
            return;
        };
        names.sort();
        let pattern = Pattern::new(comp);
        let dot_ok = pattern.starts_with_dot();
        for name in names {
            if name[0] == b'.' && !dot_ok && !(self.opts.dots && name != b"." && name != b"..") {
                continue;
            }
            if !pattern.matches(&name) {
                continue;
            }
            let p = join(&prefix, &name, rest.is_empty());
            if !rest.is_empty() && !sys::is_dir(&p[..p.len() - 1]) {
                continue;
            }
            self.expand(p, rest, true);
        }
    }

    /// The directories below `prefix`, for `**/` (and `***/` if `follow`):
    /// matches `rest` in each of them. `seen` holds the device and inode of
    /// the directories followed to get here, to stop at symbolic link loops.
    fn descend(&mut self, prefix: Vec<u8>, rest: &[&[XChar]], follow: bool, seen: &mut Vec<(u64, u64)>) {
        let Some(mut entries) = sys::read_dir_typed(&prefix) else {
            return;
        };
        entries.sort();
        for (name, kind) in entries {
            if name == b"." || name == b".." || (name[0] == b'.' && !self.opts.dots) {
                continue;
            }
            let mut p = join(&prefix, &name, true);
            let is_dir = match kind {
                libc::DT_DIR => true,
                libc::DT_LNK if follow => sys::is_dir(&p),
                libc::DT_UNKNOWN if follow => sys::is_dir(&p),
                libc::DT_UNKNOWN => sys::lstat(&p).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR),
                _ => false,
            };
            if !is_dir {
                continue;
            }
            let id = if follow {
                let Some(st) = sys::stat(&p) else { continue };
                let id = (st.st_dev, st.st_ino);
                if seen.contains(&id) {
                    continue;
                }
                seen.push(id);
                true
            } else {
                false
            };
            p.push(b'/');
            self.expand(p.clone(), rest, true);
            self.descend(p, rest, follow, seen);
            if id {
                seen.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::split::unquoted;
    use super::*;

    #[test]
    fn globstar_component() {
        assert!(is_globstar(&unquoted(b"**")));
        assert!(is_globstar(&unquoted(b"***")));
        assert!(!is_globstar(&unquoted(b"*")));
        assert!(!is_globstar(&unquoted(b"a**")));
        let mut q = unquoted(b"**");
        q[0].quoted = true;
        assert!(!is_globstar(&q));
    }
}
