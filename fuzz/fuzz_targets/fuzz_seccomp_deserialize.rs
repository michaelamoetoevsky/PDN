#![no_main]

use libfuzzer_sys::fuzz_target;
use proot_compat::SeccompProfile;
use serde_json;

fuzz_target!(|data: &[u8]| {
    // Fuzz SeccompProfile deserialization
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<SeccompProfile>(s);
    }
});