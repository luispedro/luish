//! `PATH` search, with a cache of found commands (`hash`).

use crate::shell::Shell;
use crate::sys;

fn is_executable_file(path: &[u8]) -> bool {
    sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG) && sys::access(path, libc::X_OK)
}

impl Shell {
    /// Finds a command in `PATH`. Falls back to a non-executable regular
    /// file (so that exec reports "Permission denied").
    pub fn find_in_path(&mut self, name: &[u8]) -> Option<Vec<u8>> {
        if let Some(p) = self.hash.get(name) {
            if is_executable_file(p) {
                return Some(p.clone());
            }
            self.hash.remove(name);
        }
        let found = self.search_path(name)?;
        if is_executable_file(&found) {
            self.hash.insert(name.to_vec(), found.clone());
        }
        Some(found)
    }

    pub fn search_path(&self, name: &[u8]) -> Option<Vec<u8>> {
        let path = self.get_var(b"PATH").unwrap_or_default();
        let mut fallback = None;
        for dir in path.split(|&c| c == b':') {
            let mut cand = if dir.is_empty() { b".".to_vec() } else { dir.to_vec() };
            cand.push(b'/');
            cand.extend_from_slice(name);
            if is_executable_file(&cand) {
                return Some(cand);
            }
            if fallback.is_none() && sys::stat(&cand).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG) {
                fallback = Some(cand);
            }
        }
        fallback
    }
}
