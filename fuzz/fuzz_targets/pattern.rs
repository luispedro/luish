#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| luish_fuzz::targets::pattern(data));
