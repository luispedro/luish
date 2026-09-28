//! Shell variables.

use crate::hash::HashMap;
use std::ffi::CString;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Var {
    /// `None` for a variable that has attributes (export, readonly) but no
    /// value.
    pub value: Option<Value>,
    pub exported: bool,
    pub readonly: bool,
}

/// The value of a variable: a string, or an array of them (as in zsh and
/// bash).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(Vec<u8>),
    /// Indexed from 0, as in zsh's sh emulation and bash. Unlike bash's,
    /// arrays have no holes. Boxed, so that a `Var` is no larger than a
    /// string one.
    #[allow(clippy::box_collection)]
    Array(Box<Vec<Vec<u8>>>),
    /// An associative array (`typeset -A`).
    Assoc(Box<Assoc>),
}

/// An associative array. The keys keep the order in which they were added,
/// except that removing one moves the last into its place (zsh and bash
/// use their hash order, so scripts can't rely on any).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assoc {
    keys: Vec<Vec<u8>>,
    values: Vec<Vec<u8>>,
    /// The position of each key.
    index: HashMap<Vec<u8>, usize>,
}

impl Assoc {
    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.index.get(key).map(|&i| &self.values[i])
    }

    /// The value at `key`, added empty if there is none.
    pub fn entry(&mut self, key: &[u8]) -> &mut Vec<u8> {
        let i = match self.index.get(key) {
            Some(&i) => i,
            None => {
                self.index.insert(key.to_vec(), self.keys.len());
                self.keys.push(key.to_vec());
                self.values.push(Vec::new());
                self.keys.len() - 1
            }
        };
        &mut self.values[i]
    }

    pub fn insert(&mut self, key: &[u8], value: Vec<u8>) {
        *self.entry(key) = value;
    }

    pub fn remove(&mut self, key: &[u8]) {
        if let Some(i) = self.index.remove(key) {
            self.keys.swap_remove(i);
            self.values.swap_remove(i);
            if let Some(moved) = self.keys.get(i) {
                *self.index.get_mut(moved).unwrap() = i;
            }
        }
    }

    pub fn keys(&self) -> &[Vec<u8>] {
        &self.keys
    }

    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }
}

/// The subscript of an array element: an index, or the key of an
/// associative array.
#[derive(Debug, Clone, PartialEq)]
pub enum Subscript {
    Index(i64),
    Key(Vec<u8>),
}

impl std::fmt::Display for Subscript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Subscript::Index(i) => write!(f, "{i}"),
            Subscript::Key(k) => write!(f, "{}", String::from_utf8_lossy(k)),
        }
    }
}

/// An element of an array assignment after expansion, `x` or
/// `[key]=x`.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
}

impl Value {
    /// The value as a list: a string is an array of one element, and an
    /// associative array gives its values.
    pub fn elements(&self) -> &[Vec<u8>] {
        match self {
            Value::Str(s) => std::slice::from_ref(s),
            Value::Array(a) => a,
            Value::Assoc(h) => h.values(),
        }
    }

    /// The value as an array, converting a string (or the values of an
    /// associative array) to one.
    pub fn make_array(&mut self) -> &mut Vec<Vec<u8>> {
        match self {
            Value::Str(s) => *self = Value::Array(Box::new(vec![std::mem::take(s)])),
            Value::Assoc(h) => *self = Value::Array(Box::new(h.values().to_vec())),
            Value::Array(_) => {}
        }
        let Value::Array(a) = self else { unreachable!() };
        a
    }
}

/// Why assigning to an array element failed.
#[derive(Debug)]
pub enum AssignError {
    Readonly,
    /// A negative index before the start of the array.
    BadSubscript,
}

impl From<ReadonlyError> for AssignError {
    fn from(_: ReadonlyError) -> AssignError {
        AssignError::Readonly
    }
}

#[derive(Debug, Default)]
pub struct Vars {
    map: HashMap<Vec<u8>, Var>,
    specials: Specials,
}

/// zsh's special parameters that are computed when they are read. They are
/// not in the map (so a plain lookup costs nothing more). As in zsh, one
/// that is unset reads as unset until it is assigned, and assigning to
/// `RANDOM` seeds the generator and to `SECONDS` sets the count. Assigning
/// to the others makes them ordinary variables (where zsh makes them
/// read-only, calls `setuid` or sets `pipestatus` until the next pipeline),
/// so that scripts written for dash that use these names keep working.
/// `pipestatus` (and bash's `PIPESTATUS`) is an array, which the shell
/// computes (`Shell::special_elements`).
///
/// `path` is zsh's array of the directories in `PATH`. An array assignment
/// to it (`path=(...)`, `path+=(...)`, `path[i]=x`, an error in dash) sets
/// `PATH`, while a string assignment makes it an ordinary variable (as for
/// `UID`), so that dash scripts can use the name. It is also made ordinary
/// by `local`, and taken from the environment as an ordinary variable.
/// `dirstack` (zsh's directory stack, without the current directory) is
/// tied to `Shell::dirstack` in the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    Random,
    Seconds,
    EpochSeconds,
    EpochRealtime,
    Uid,
    Euid,
    Gid,
    Egid,
    Histcmd,
    Pipestatus,
    /// bash's name for `pipestatus`.
    PipestatusBash,
    Path,
    Dirstack,
}

pub const SPECIALS: &[(&[u8], Special)] = &[
    (b"RANDOM", Special::Random),
    (b"SECONDS", Special::Seconds),
    (b"EPOCHSECONDS", Special::EpochSeconds),
    (b"EPOCHREALTIME", Special::EpochRealtime),
    (b"UID", Special::Uid),
    (b"EUID", Special::Euid),
    (b"GID", Special::Gid),
    (b"EGID", Special::Egid),
    (b"HISTCMD", Special::Histcmd),
    (b"pipestatus", Special::Pipestatus),
    (b"PIPESTATUS", Special::PipestatusBash),
    (b"path", Special::Path),
    (b"dirstack", Special::Dirstack),
];

impl Special {
    pub fn from_name(name: &[u8]) -> Option<Special> {
        // Most names are rejected on their first byte.
        if !matches!(
            name.first(),
            Some(b'E' | b'G' | b'H' | b'P' | b'R' | b'S' | b'U' | b'd' | b'p')
        ) {
            return None;
        }
        SPECIALS.iter().find(|(n, _)| *n == name).map(|&(_, s)| s)
    }

    fn bit(self) -> u16 {
        1 << self as u16
    }

    pub fn name(self) -> &'static [u8] {
        SPECIALS.iter().find(|&&(_, s)| s == self).unwrap().0
    }

    /// Whether it is an array tied to something else (`path`, `dirstack`):
    /// array assignments change that, other assignments make it ordinary.
    pub fn is_tied(self) -> bool {
        matches!(self, Special::Path | Special::Dirstack)
    }
}

#[derive(Debug, Clone)]
struct Specials {
    /// The specials that are set, one bit each.
    active: u16,
    random: std::cell::Cell<RandomSeed>,
    /// `SECONDS` is `seconds.1` plus the seconds since `seconds.0`.
    seconds: (std::time::Instant, i64),
}

impl Default for Specials {
    fn default() -> Specials {
        Specials {
            active: !0,
            random: std::cell::Cell::new(RandomSeed::Unseeded),
            seconds: (std::time::Instant::now(), 0),
        }
    }
}

/// How `RANDOM`'s generator (the C library's `rand`, as in zsh) was seeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RandomSeed {
    Unseeded,
    /// From the time and process id, when `RANDOM` was first read: a
    /// subshell seeds its own (as bash does, where zsh repeats the parent's
    /// numbers).
    Auto,
    /// By an assignment: subshells continue the sequence.
    Assigned,
}

#[derive(Debug)]
pub struct ReadonlyError;

/// A variable put aside by `local` or a temporary assignment, for
/// `Shell::restore_saved`: its value, and whether its name was a special
/// that was set (as an assignment makes most of them ordinary).
#[derive(Debug)]
pub struct Saved {
    var: Option<Var>,
    special: bool,
}

/// Sets element `i` of an array (counting from the end if it is negative),
/// adding empty elements up to it; a string becomes an array. With
/// `append`, the value is appended to the element.
pub fn set_index(value: &mut Option<Value>, i: i64, v: Vec<u8>, append: bool) -> Result<(), AssignError> {
    let len = value.as_ref().map_or(0, |v| v.elements().len());
    let i = if i < 0 { i + len as i64 } else { i };
    if i < 0 {
        return Err(AssignError::BadSubscript);
    }
    let i = i as usize;
    let a = value.get_or_insert_with(|| Value::Array(Box::default())).make_array();
    if i >= a.len() {
        a.resize(i + 1, Vec::new());
    }
    if append {
        a[i].extend_from_slice(&v);
    } else {
        a[i] = v;
    }
    Ok(())
}

impl Vars {
    pub fn from_env() -> Vars {
        let mut map = HashMap::default();
        let mut specials = Specials::default();
        for (k, v) in std::env::vars_os() {
            use std::os::unix::ffi::OsStrExt;
            let k = k.as_bytes().to_vec();
            if !crate::lexer::is_valid_name(&k) {
                continue;
            }
            // As in zsh, the specials ignore the environment, except the
            // tied arrays, which aren't special in zsh's sh emulation.
            match Special::from_name(&k) {
                Some(s) if s.is_tied() => specials.active &= !s.bit(),
                Some(_) => continue,
                None => {}
            }
            map.insert(
                k,
                Var {
                    value: Some(Value::Str(v.as_bytes().to_vec())),
                    exported: true,
                    readonly: false,
                },
            );
        }
        Vars { map, specials }
    }

    /// The special parameter `name` names, if it is set.
    pub fn special(&self, name: &[u8]) -> Option<Special> {
        Special::from_name(name).filter(|s| self.specials.active & s.bit() != 0)
    }

    /// The value of a special (`HISTCMD` and `pipestatus` are left to the
    /// caller, which knows about the history and pipelines).
    pub fn special_value(&self, s: Special) -> Vec<u8> {
        let n: i64 = match s {
            Special::Random => {
                if self.specials.random.get() == RandomSeed::Unseeded {
                    let t = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    let seed = t.as_secs() as u32 ^ t.subsec_nanos() ^ std::process::id();
                    // SAFETY: the shell has a single thread.
                    unsafe { libc::srand(seed) };
                    self.specials.random.set(RandomSeed::Auto);
                }
                // SAFETY: as for `srand`.
                (unsafe { libc::rand() } & 0x7fff).into()
            }
            Special::Seconds => {
                let (since, base) = self.specials.seconds;
                base + since.elapsed().as_secs() as i64
            }
            Special::EpochSeconds | Special::EpochRealtime => {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                if s == Special::EpochRealtime {
                    // Microseconds, as in bash (zsh's is a float), so that
                    // `${EPOCHREALTIME%.*}` and `${EPOCHREALTIME#*.}` work in
                    // integer arithmetic.
                    return format!("{}.{:06}", t.as_secs(), t.subsec_micros()).into_bytes();
                }
                t.as_secs() as i64
            }
            Special::Uid => crate::sys::getuid().into(),
            Special::Euid => crate::sys::geteuid().into(),
            Special::Gid => crate::sys::getgid().into(),
            Special::Egid => crate::sys::getegid().into(),
            Special::Histcmd | Special::Pipestatus | Special::PipestatusBash | Special::Path | Special::Dirstack => 0,
        };
        n.to_string().into_bytes()
    }

    /// Assigns to a special: seeds `RANDOM` or sets `SECONDS`, or makes the
    /// others ordinary variables (returning false).
    fn assign_special(&mut self, s: Special, value: &[u8]) -> bool {
        let n = || crate::expand::arith::parse_number(value).unwrap_or(0);
        match s {
            Special::Random => {
                // SAFETY: as in `special_value`.
                unsafe { libc::srand(n() as u32) };
                self.specials.random.set(RandomSeed::Assigned);
            }
            Special::Seconds => self.specials.seconds = (std::time::Instant::now(), n()),
            _ => {
                // Not in the map, which now has the ordinary variable.
                self.specials.active &= !s.bit();
                return false;
            }
        }
        self.specials.active |= s.bit();
        true
    }

    /// In a forked child: `RANDOM` seeds itself anew, unless it was
    /// assigned.
    pub fn child_reset(&mut self) {
        if self.specials.random.get() == RandomSeed::Auto {
            self.specials.random.set(RandomSeed::Unseeded);
        }
    }

    /// Makes a special an ordinary variable.
    pub fn deactivate(&mut self, s: Special) {
        self.specials.active &= !s.bit();
    }

    /// The names of the specials that are set.
    pub fn special_names(&self) -> impl Iterator<Item = &'static [u8]> + '_ {
        SPECIALS
            .iter()
            .filter(|(_, s)| self.specials.active & s.bit() != 0)
            .map(|&(n, _)| n)
    }

    /// The value of a variable as a string: an array's first element, so
    /// an empty array is unset, as in zsh's sh emulation and bash.
    pub fn get(&self, name: &[u8]) -> Option<&[u8]> {
        match self.map.get(name)?.value.as_ref()? {
            Value::Str(s) => Some(s),
            Value::Array(a) => a.first().map(|s| &s[..]),
            Value::Assoc(h) => h.get(b"0").map(|s| &s[..]),
        }
    }

    pub fn get_value_mut(&mut self, name: &[u8]) -> Option<&mut Value> {
        self.map.get_mut(name).and_then(|v| v.value.as_mut())
    }

    pub fn var(&self, name: &[u8]) -> Option<&Var> {
        self.map.get(name)
    }

    pub fn is_assoc(&self, name: &[u8]) -> bool {
        matches!(self.get_value(name), Some(Value::Assoc(_)))
    }

    pub fn get_value(&self, name: &[u8]) -> Option<&Value> {
        self.map.get(name).and_then(|v| v.value.as_ref())
    }

    /// Assigns a whole value (an array), replacing the old one.
    pub fn set_value(&mut self, name: &[u8], value: Value) -> Result<(), ReadonlyError> {
        let var = self.entry(name);
        if var.readonly {
            return Err(ReadonlyError);
        }
        var.value = Some(value);
        // A special such as `RANDOM` becomes an ordinary variable.
        if let Some(s) = Special::from_name(name) {
            self.specials.active &= !s.bit();
        }
        Ok(())
    }

    /// Assigns to an element of an array. With an index, `i`, (counting
    /// from the end if it is negative), the variable is made an array, and
    /// the elements between the end and `i` are made empty. With a key, it
    /// is made an associative array. With `append`, the value is appended
    /// to the element.
    pub fn set_element(
        &mut self,
        name: &[u8],
        sub: &Subscript,
        value: Vec<u8>,
        append: bool,
    ) -> Result<(), AssignError> {
        if let Some(s) = Special::from_name(name) {
            self.specials.active &= !s.bit();
        }
        let var = self.entry(name);
        if var.readonly {
            return Err(AssignError::Readonly);
        }
        let i = match sub {
            Subscript::Index(i) => *i,
            Subscript::Key(k) => {
                if !matches!(var.value, Some(Value::Assoc(_))) {
                    var.value = Some(Value::Assoc(Box::default()));
                }
                let Some(Value::Assoc(h)) = &mut var.value else {
                    unreachable!()
                };
                let v = h.entry(k);
                if append {
                    v.extend_from_slice(&value);
                } else {
                    *v = value;
                }
                return Ok(());
            }
        };
        set_index(&mut var.value, i, value, append)
    }

    /// Appends elements to an array (`a+=(x y)`), making the variable an
    /// array.
    pub fn append_elements(&mut self, name: &[u8], items: Vec<Vec<u8>>) -> Result<(), ReadonlyError> {
        if let Some(s) = Special::from_name(name) {
            self.specials.active &= !s.bit();
        }
        let var = self.entry(name);
        if var.readonly {
            return Err(ReadonlyError);
        }
        let a = var
            .value
            .get_or_insert_with(|| Value::Array(Box::default()))
            .make_array();
        a.extend(items);
        Ok(())
    }

    /// Assigns the pairs of keys and values to an associative array, or
    /// with `append` adds them to it.
    pub fn set_pairs(
        &mut self,
        name: &[u8],
        pairs: Vec<(Vec<u8>, Vec<u8>)>,
        append: bool,
    ) -> Result<(), ReadonlyError> {
        if let Some(s) = Special::from_name(name) {
            self.specials.active &= !s.bit();
        }
        let var = self.entry(name);
        if var.readonly {
            return Err(ReadonlyError);
        }
        if !append || !matches!(var.value, Some(Value::Assoc(_))) {
            var.value = Some(Value::Assoc(Box::default()));
        }
        let Some(Value::Assoc(h)) = &mut var.value else {
            unreachable!()
        };
        for (k, v) in pairs {
            h.insert(&k, v);
        }
        Ok(())
    }

    pub fn set(&mut self, name: &[u8], value: Vec<u8>) -> Result<(), ReadonlyError> {
        if let Some(s) = Special::from_name(name)
            && (self.specials.active & s.bit() != 0 || matches!(s, Special::Random | Special::Seconds))
        {
            if self.map.get(name).is_some_and(|v| v.readonly) {
                return Err(ReadonlyError);
            }
            if self.assign_special(s, &value) {
                return Ok(());
            }
        }
        match self.map.get_mut(name) {
            Some(v) if v.readonly => Err(ReadonlyError),
            Some(v) => {
                // As in ksh and bash, `a=x` sets the first element of an array.
                match &mut v.value {
                    Some(Value::Array(a)) if a.is_empty() => a.push(value),
                    Some(Value::Array(a)) => a[0] = value,
                    Some(Value::Assoc(h)) => h.insert(b"0", value),
                    _ => v.value = Some(Value::Str(value)),
                }
                Ok(())
            }
            None => {
                self.map.insert(
                    name.to_vec(),
                    Var {
                        value: Some(Value::Str(value)),
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
                if let Some(s) = Special::from_name(name) {
                    self.specials.active &= !s.bit();
                }
                Ok(())
            }
        }
    }

    pub fn entry(&mut self, name: &[u8]) -> &mut Var {
        // Not `map.entry`, which would copy the name even if it is there.
        if !self.map.contains_key(name) {
            self.map.insert(name.to_vec(), Var::default());
        }
        self.map.get_mut(name).unwrap()
    }

    /// Replaces a variable wholesale (used to restore saved variables).
    pub fn restore(&mut self, name: Vec<u8>, var: Option<Var>) {
        match var {
            Some(v) => {
                self.map.insert(name, v);
            }
            None => {
                self.map.remove(&name);
            }
        }
    }

    pub fn take(&self, name: &[u8]) -> Option<Var> {
        self.map.get(name).cloned()
    }

    /// Saves a variable, to put back with `restore_saved`.
    pub fn save(&self, name: &[u8]) -> Saved {
        Saved {
            var: self.take(name),
            special: self.special(name).is_some(),
        }
    }

    /// Puts back a variable saved with `save`, and a special's tie.
    pub fn restore_saved(&mut self, name: Vec<u8>, saved: Saved) {
        if let Some(s) = Special::from_name(&name) {
            if saved.special {
                self.specials.active |= s.bit();
            } else {
                self.specials.active &= !s.bit();
            }
        }
        self.restore(name, saved.var);
    }

    /// A copy of all the variables, for `changes_since`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    pub fn snapshot(&self) -> Vars {
        Vars {
            map: self.map.clone(),
            specials: self.specials.clone(),
        }
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
                // Arrays aren't exported, as in zsh and bash.
                let Some(Value::Str(value)) = v.value.as_ref() else {
                    return None;
                };
                let mut s = Vec::with_capacity(k.len() + value.len() + 1);
                s.extend_from_slice(k);
                s.push(b'=');
                s.extend_from_slice(value);
                CString::new(s).ok()
            })
            .collect()
    }
}
