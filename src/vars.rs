//! Shell variables.

use crate::hash::HashMap;
use std::ffi::CString;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Var {
    /// `None` for a variable that has attributes (export, readonly) but no
    /// value.
    pub value: Option<Vec<u8>>,
    pub exported: bool,
    pub readonly: bool,
}

#[derive(Debug, Default)]
pub struct Vars {
    map: HashMap<Vec<u8>, Var>,
}

#[derive(Debug)]
pub struct ReadonlyError;

impl Vars {
    pub fn from_env() -> Vars {
        let mut map = HashMap::default();
        for (k, v) in std::env::vars_os() {
            use std::os::unix::ffi::OsStrExt;
            let k = k.as_bytes().to_vec();
            if !crate::lexer::is_valid_name(&k) {
                continue;
            }
            map.insert(
                k,
                Var {
                    value: Some(v.as_bytes().to_vec()),
                    exported: true,
                    readonly: false,
                },
            );
        }
        Vars { map }
    }

    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.map.get(name).and_then(|v| v.value.as_deref())
    }

    pub fn set(&mut self, name: &[u8], value: Vec<u8>) -> Result<(), ReadonlyError> {
        match self.map.get_mut(name) {
            Some(v) if v.readonly => Err(ReadonlyError),
            Some(v) => {
                v.value = Some(value);
                Ok(())
            }
            None => {
                self.map.insert(
                    name.to_vec(),
                    Var {
                        value: Some(value),
                        ..Var::default()
                    },
                );
                Ok(())
            }
        }
    }

    pub fn unset(&mut self, name: &[u8]) -> Result<(), ReadonlyError> {
        match self.map.get(name) {
            Some(v) if v.readonly => Err(ReadonlyError),
            _ => {
                self.map.remove(name);
                Ok(())
            }
        }
    }

    pub fn entry(&mut self, name: &[u8]) -> &mut Var {
        self.map.entry(name.to_vec()).or_default()
    }

    /// Replaces a variable wholesale (used to restore saved variables).
    pub fn restore(&mut self, name: &[u8], var: Option<Var>) {
        match var {
            Some(v) => {
                self.map.insert(name.to_vec(), v);
            }
            None => {
                self.map.remove(name);
            }
        }
    }

    pub fn take(&self, name: &[u8]) -> Option<Var> {
        self.map.get(name).cloned()
    }

    /// A copy of all the variables, for `changes_since`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn snapshot(&self) -> Vars {
        Vars { map: self.map.clone() }
    }

    /// The variables that differ from `snapshot`, with what they were
    /// there (`None` for those that didn't exist), for `restore`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn changes_since(&self, snapshot: &Vars) -> Vec<(Vec<u8>, Option<Var>)> {
        let mut changed: Vec<_> = (self.map.iter())
            .filter(|(k, v)| snapshot.map.get(*k) != Some(*v))
            .map(|(k, _)| (k.clone(), snapshot.map.get(k).cloned()))
            .collect();
        changed.extend(
            (snapshot.map.iter())
                .filter(|(k, _)| !self.map.contains_key(*k))
                .map(|(k, v)| (k.clone(), Some(v.clone()))),
        );
        changed
    }

    /// The names of the variables that are set.
    pub fn names(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.map.iter().filter(|(_, v)| v.value.is_some()).map(|(k, _)| k)
    }

    /// All variables, sorted by name.
    pub fn sorted(&self) -> Vec<(&Vec<u8>, &Var)> {
        let mut v: Vec<_> = self.map.iter().collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v
    }

    /// `NAME=value` strings for the environment of a new program.
    pub fn environ(&self) -> Vec<CString> {
        self.map
            .iter()
            .filter(|(_, v)| v.exported)
            .filter_map(|(k, v)| {
                let value = v.value.as_ref()?;
                let mut s = Vec::with_capacity(k.len() + value.len() + 1);
                s.extend_from_slice(k);
                s.push(b'=');
                s.extend_from_slice(value);
                CString::new(s).ok()
            })
            .collect()
    }
}
