// Rust's Unicode alphabetic and numeric predicates are not const. Generate their
// non-ASCII ranges with this toolchain so key literals and user settings agree.
use std::{env, fmt::Write, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let mut source =
        String::from("const fn is_alphanumeric(c: char) -> bool {\nmatch c as u32 {\n");
    source.push_str("0..=127 => c.is_ascii_alphanumeric(),\n");
    let mut start = None;
    // The final out-of-range code point closes any pending range.
    for code in 128..=u32::from(char::MAX) + 1 {
        if char::from_u32(code).is_some_and(char::is_alphanumeric) {
            start.get_or_insert(code);
        } else if let Some(first) = start.take() {
            writeln!(source, "{first}..={} => true,", code - 1).unwrap();
        }
    }
    source.push_str("_ => false,\n}\n}\n");
    let out = env::var_os("OUT_DIR").unwrap();
    fs::write(Path::new(&out).join("shortcut_alphanumeric.rs"), source).unwrap();
}
