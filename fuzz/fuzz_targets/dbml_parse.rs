#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let parsed = dbml_syntax::parse(text);
        debug_assert_eq!(parsed.syntax().text().to_string(), text);
    }
});
