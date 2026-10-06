//! A bad network, for testing the SSH mode: with `LUISH_CHAOS=LEVEL` (or
//! `LEVEL:SEED`) in its environment, `luish --serve` passes what it sends
//! to the client and what it receives from it through `Chaos`, after the
//! greeting. Bytes are never lost or reordered, as over ssh, but:
//!
//! - level 1 cuts them into pieces at random places, and delays each piece
//!   by 20 to 200 ms (jitter, with the order kept);
//! - level 2 also stalls now and then (nothing passes for 0.5 to 3 s), and
//!   at random drops the connection: nothing passes any more, and after 2
//!   to 10 s (as when ssh's `ServerAliveInterval` gives up) the server
//!   exits, which hangs up the shell.
//!
//! The seed is printed, so that a run's choices can be made again (the
//! timing of reads still differs).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// One in this many pieces is followed by a stall, at level 2.
const STALL_ODDS: u64 = 50;
/// One in this many pieces drops the connection, at level 2.
const DROP_ODDS: u64 = 300;

pub struct Chaos {
    level: u8,
    rng: u64,
    /// To the client, and from it.
    out: Queue,
    inp: Queue,
    /// Once the connection has been dropped: when the server gives up.
    dropped: Option<Instant>,
}

/// Bytes on their way, each piece with the time it arrives.
#[derive(Default)]
struct Queue {
    pieces: VecDeque<(Instant, Vec<u8>)>,
}

/// What has arrived: bytes for the client, and from it.
#[derive(Default)]
pub struct Arrived {
    pub out: Vec<u8>,
    pub inp: Vec<u8>,
    /// The connection was dropped, and the server should now exit.
    pub gone: bool,
}

impl Chaos {
    /// The chaos `LUISH_CHAOS` asks for (and unsets it, so that the shell's
    /// commands don't inherit it), or None. Err with a bad value.
    pub fn from_env() -> Result<Option<Chaos>, ()> {
        let Some(v) = std::env::var_os("LUISH_CHAOS") else {
            return Ok(None);
        };
        // SAFETY: single-threaded, before anything reads the environment.
        unsafe { libc::unsetenv(c"LUISH_CHAOS".as_ptr()) };
        let v = v.to_str().ok_or(())?;
        let (level, seed) = match v.split_once(':') {
            Some((l, s)) => (l, Some(s.parse::<u64>().map_err(|_| ())?)),
            None => (v, None),
        };
        let level = match level {
            "" | "0" => return Ok(None),
            "1" => 1,
            "2" => 2,
            _ => return Err(()),
        };
        let seed = seed.unwrap_or_else(|| {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            t.as_nanos() as u64 ^ (crate::sys::getpid() as u64) << 32
        });
        let msg = format!("luish --serve: chaos level {level} (LUISH_CHAOS={level}:{seed})\r\n");
        crate::sys::write_all(2, msg.as_bytes());
        Ok(Some(Chaos::new(level, seed)))
    }

    fn new(level: u8, seed: u64) -> Chaos {
        Chaos {
            level,
            // Never 0, which xorshift would keep.
            rng: seed | 1,
            out: Queue::default(),
            inp: Queue::default(),
            dropped: None,
        }
    }

    /// xorshift64*.
    fn rand(&mut self) -> u64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A number in `lo..hi`.
    fn between(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.rand() % (hi - lo)
    }

    /// Bytes for the client.
    pub fn send(&mut self, data: &[u8], now: Instant) {
        self.schedule(true, data, now);
    }

    /// Bytes from the client.
    pub fn receive(&mut self, data: &[u8], now: Instant) {
        self.schedule(false, data, now);
    }

    fn schedule(&mut self, out: bool, mut data: &[u8], now: Instant) {
        while !data.is_empty() && self.dropped.is_none() {
            // A third of the time, a piece of a random length.
            let n = match self.rand() % 3 {
                0 => self.between(1, data.len() as u64 + 1) as usize,
                _ => data.len(),
            };
            let mut delay = self.between(20, 200);
            if self.level >= 2 {
                if self.rand().is_multiple_of(STALL_ODDS) {
                    delay += self.between(500, 3000);
                }
                if self.rand().is_multiple_of(DROP_ODDS) {
                    if !cfg!(test) {
                        crate::sys::write_all(2, b"luish --serve: chaos: dropping the connection\r\n");
                    }
                    self.dropped = Some(now + Duration::from_millis(self.between(2000, 10000)));
                    return;
                }
            }
            let q = if out { &mut self.out } else { &mut self.inp };
            // Never before the piece in front of it.
            let at = q
                .pieces
                .back()
                .map_or(now, |p| p.0)
                .max(now + Duration::from_millis(delay));
            q.pieces.push_back((at, data[..n].to_vec()));
            data = &data[n..];
        }
    }

    /// What has arrived by `now`.
    pub fn arrived(&mut self, now: Instant) -> Arrived {
        let mut a = Arrived::default();
        if let Some(t) = self.dropped {
            a.gone = now >= t;
            return a;
        }
        for (q, buf) in [(&mut self.out, &mut a.out), (&mut self.inp, &mut a.inp)] {
            while let Some((at, _)) = q.pieces.front()
                && *at <= now
            {
                buf.extend_from_slice(&q.pieces.pop_front().unwrap().1);
            }
        }
        a
    }

    /// How long until something more arrives, in milliseconds (rounded
    /// up), or None if nothing is on its way.
    pub fn wait(&self, now: Instant) -> Option<i32> {
        let next = match self.dropped {
            Some(t) => t,
            None => [&self.out, &self.inp]
                .iter()
                .filter_map(|q| q.pieces.front().map(|p| p.0))
                .min()?,
        };
        let us = next.saturating_duration_since(now).as_micros();
        Some(us.div_ceil(1000).min(i32::MAX as u128) as i32)
    }

    /// Whether bytes for the client are still on their way.
    pub fn sending(&self) -> bool {
        self.dropped.is_none() && !self.out.pieces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `data` through, a message at a time, until all has arrived or
    /// the connection is dropped. Returns what arrived each way.
    fn run(c: &mut Chaos, msgs: &[&[u8]]) -> (Vec<u8>, Vec<u8>, bool) {
        let mut now = Instant::now();
        let (mut out, mut inp) = (Vec::new(), Vec::new());
        for m in msgs {
            c.send(m, now);
            c.receive(m, now);
            now += Duration::from_millis(5);
        }
        while let Some(w) = c.wait(now) {
            now += Duration::from_millis(w as u64);
            let a = c.arrived(now);
            out.extend(a.out);
            inp.extend(a.inp);
            if a.gone {
                return (out, inp, true);
            }
        }
        (out, inp, false)
    }

    #[test]
    fn order_kept() {
        let msgs: Vec<Vec<u8>> = (0..200).map(|i| format!("message {i};").into_bytes()).collect();
        let msgs: Vec<&[u8]> = msgs.iter().map(Vec::as_slice).collect();
        let all = msgs.concat();
        for seed in 0..20 {
            let mut c = Chaos::new(1, seed);
            let (out, inp, gone) = run(&mut c, &msgs);
            assert!(!gone);
            assert_eq!(out, all);
            assert_eq!(inp, all);
        }
    }

    #[test]
    fn delays_and_pieces() {
        let mut c = Chaos::new(1, 7);
        let now = Instant::now();
        c.send(&[b'x'; 1000], now);
        // Nothing arrives at once, and something within 200 ms.
        assert!(c.arrived(now).out.is_empty());
        let w = c.wait(now).unwrap();
        assert!((20..=200).contains(&w), "{w}");
        // Some message is cut.
        let mut cut = false;
        for i in 0..50u8 {
            c.send(&[i; 100], now);
            cut |= c.out.pieces.back().unwrap().1.len() < 100;
        }
        assert!(cut);
    }

    #[test]
    fn level_two_drops() {
        let msgs: Vec<&[u8]> = vec![b"some bytes"; 5000];
        let mut c = Chaos::new(2, 1);
        let (out, inp, gone) = run(&mut c, &msgs);
        assert!(gone);
        // What came before the drop came whole and in order.
        let all = msgs.concat();
        assert!(all.starts_with(&out) && all.starts_with(&inp));
        // Nothing passes once dropped.
        c.send(b"more", Instant::now());
        assert!(!c.sending());
    }
}
