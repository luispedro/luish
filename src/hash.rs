//! A fast hash for the shell's tables (variables, functions, aliases, the
//! command cache). Their keys are short names chosen by the script, so
//! std's SipHash, which resists flooding by untrusted keys, costs time for
//! nothing: a variable lookup is on the hot path of almost every command.

use std::hash::{BuildHasherDefault, Hasher};

/// A multiply per 8 bytes, as in rustc's `FxHasher`, but keeping both halves
/// of the 128-bit product: a plain multiply only carries a byte's
/// influence towards the high bits, so names that differ only in an early
/// byte would end up in the same bucket.
#[derive(Default, Clone, Copy)]
pub struct FastHasher {
    hash: u64,
}

const SEED: u64 = 0xf135_7aea_2e62_a9c5;

impl FastHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        let p = u128::from(self.hash ^ word) * u128::from(SEED);
        self.hash = (p as u64) ^ ((p >> 64) as u64);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while let Some((word, rest)) = bytes.split_first_chunk::<8>() {
            self.add(u64::from_le_bytes(*word));
            bytes = rest;
        }
        if !bytes.is_empty() {
            let mut word = [0u8; 8];
            word[..bytes.len()].copy_from_slice(bytes);
            self.add(u64::from_le_bytes(word));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i.into());
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FastHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, BuildHasherDefault};

    fn h(s: &[u8]) -> u64 {
        BuildHasherDefault::<FastHasher>::default().hash_one(s)
    }

    #[test]
    fn distinct() {
        // Names that differ only in their last byte, or only in length.
        for fmt in [|i| format!("sieve_{i}"), |i| format!("{i}_sieve_x")] {
            let names: Vec<Vec<u8>> = (0..1000).map(|i| fmt(i).into_bytes()).collect();
            for shift in [0, 57] {
                let mut buckets: Vec<u64> = names.iter().map(|n| (h(n) >> shift) & 127).collect();
                buckets.sort();
                buckets.dedup();
                assert!(buckets.len() > 100, "{} buckets", buckets.len());
            }
        }
        assert_ne!(h(b"a"), h(b"a\0"));
        assert_ne!(h(b""), h(b"\0"));
    }
}
