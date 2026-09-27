//! Building fields during expansion, including IFS field splitting.
//!
//! Splitting happens as expansion results are appended, so each byte only
//! needs to remember whether it was quoted (which matters for globbing).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XChar {
    pub b: u8,
    pub quoted: bool,
}

pub type XField = Vec<XChar>;

#[cfg(test)]
pub fn unquoted(s: &[u8]) -> XField {
    s.iter().map(|&b| XChar { b, quoted: false }).collect()
}

pub fn bytes(f: &[XChar]) -> Vec<u8> {
    f.iter().map(|c| c.b).collect()
}

/// The set of IFS characters.
#[derive(Clone, Copy)]
pub struct IfsSet([u64; 4]);

impl IfsSet {
    pub fn new(ifs: &[u8]) -> IfsSet {
        let mut set = [0u64; 4];
        for &c in ifs {
            set[(c >> 6) as usize] |= 1 << (c & 63);
        }
        IfsSet(set)
    }

    #[inline]
    fn contains(&self, c: u8) -> bool {
        self.0[(c >> 6) as usize] & (1 << (c & 63)) != 0
    }

    fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
}

pub struct Fields {
    pub fields: Vec<XField>,
    pub cur: XField,
    /// The current field exists even if empty (e.g. it contained `""`).
    pub cur_exists: bool,
    /// The last field was ended by IFS whitespace (so an adjacent
    /// non-whitespace IFS character belongs to the same delimiter).
    ws_delim: bool,
    /// `None` disables field splitting.
    ifs: Option<IfsSet>,
    /// The result is a list of fields (command words), even if IFS is empty.
    field_ctx: bool,
}

fn is_ifs_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n')
}

impl Fields {
    pub fn new(ifs: Option<IfsSet>) -> Fields {
        Fields {
            fields: Vec::new(),
            cur: Vec::new(),
            cur_exists: false,
            ws_delim: false,
            field_ctx: ifs.is_some(),
            ifs: ifs.filter(|i| !i.is_empty()),
        }
    }

    /// Whether the result is a list of fields: `$@` and unquoted `$*`
    /// then give separate fields, even when IFS is empty.
    pub fn field_context(&self) -> bool {
        self.field_ctx
    }

    pub fn push_quoted(&mut self, s: &[u8]) {
        self.cur.extend(s.iter().map(|&b| XChar { b, quoted: true }));
        self.cur_exists = true;
        self.ws_delim = false;
    }

    /// Unquoted text that came from the source (never split).
    pub fn push_literal(&mut self, s: &[u8]) {
        self.cur.extend(s.iter().map(|&b| XChar { b, quoted: false }));
        if !s.is_empty() {
            self.ws_delim = false;
        }
    }

    /// The unquoted result of an expansion: subject to field splitting.
    pub fn push_expansion(&mut self, s: &[u8]) {
        let Some(ifs) = self.ifs else {
            self.push_literal(s);
            return;
        };
        for &c in s {
            if !ifs.contains(c) {
                self.cur.push(XChar { b: c, quoted: false });
                self.ws_delim = false;
            } else if is_ifs_ws(c) {
                if !self.cur.is_empty() || self.cur_exists {
                    self.finish();
                    self.ws_delim = true;
                }
            } else if self.ws_delim {
                self.ws_delim = false;
            } else {
                self.finish();
            }
        }
    }

    /// Ends the current field, if it has any content.
    pub fn break_field(&mut self) {
        if !self.cur.is_empty() || self.cur_exists {
            self.finish();
        }
        self.ws_delim = false;
    }

    /// Ends the current field unconditionally.
    pub fn finish(&mut self) {
        self.fields.push(std::mem::take(&mut self.cur));
        self.cur_exists = false;
    }

    pub fn into_fields(mut self) -> Vec<XField> {
        self.break_field();
        self.fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(ifs: &str, s: &str) -> Vec<String> {
        let mut f = Fields::new(Some(IfsSet::new(ifs.as_bytes())));
        f.push_expansion(s.as_bytes());
        f.into_fields()
            .iter()
            .map(|x| String::from_utf8(bytes(x)).unwrap())
            .collect()
    }

    #[test]
    fn ifs_rules() {
        assert_eq!(split(" \t\n", "  a  b "), ["a", "b"]);
        assert_eq!(split(":", "a::b"), ["a", "", "b"]);
        assert_eq!(split(":", ":a:"), ["", "a"]);
        assert_eq!(split(" :", "a : b"), ["a", "b"]);
        assert_eq!(split(" :", " : a"), ["", "a"]);
        assert_eq!(split(" :", "a : : b"), ["a", "", "b"]);
        assert_eq!(split(":", ""), Vec::<String>::new());
    }
}
