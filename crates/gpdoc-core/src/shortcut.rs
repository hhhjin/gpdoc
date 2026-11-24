/// A single logical keystroke. The primary modifier is Command on macOS and Control elsewhere.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Shortcut {
    key: String,
    pub primary: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Shortcut {
    /// Construct a keystroke from its key alone. Parse strings such as `"Mod-b"` with `str::parse`.
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_ascii_lowercase(),
            primary: false,
            alt: false,
            shift: false,
        }
    }

    pub fn primary(key: &str) -> Self {
        Self {
            primary: true,
            ..Self::new(key)
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn has_valid_key(&self) -> bool {
        valid_key(&self.key)
    }
}

/// Borrow the key during const evaluation. Allocate its normalized name when used.
#[doc(hidden)]
pub struct ShortcutParts<'a> {
    key: &'a str,
    primary: bool,
    alt: bool,
    shift: bool,
}

impl<'a> ShortcutParts<'a> {
    #[doc(hidden)]
    pub const fn parse(mut binding: &'a str) -> Option<Self> {
        let mut parts = Self {
            key: "",
            primary: false,
            alt: false,
            shift: false,
        };
        // A lone minus is the key, including after modifiers in "Mod--".
        while !matches!(binding.as_bytes(), [b'-']) {
            let bytes = binding.as_bytes();
            let mut end = 0;
            while end < bytes.len() && bytes[end] != b'-' {
                end += 1;
            }
            if end == bytes.len() {
                break;
            }
            let (modifier, rest) = binding.split_at(end);
            let flag = if modifier.eq_ignore_ascii_case("mod") {
                &mut parts.primary
            } else if modifier.eq_ignore_ascii_case("alt") {
                &mut parts.alt
            } else if modifier.eq_ignore_ascii_case("shift") {
                &mut parts.shift
            } else {
                return None;
            };
            if *flag {
                return None;
            }
            *flag = true;
            binding = rest.split_at(1).1;
        }
        if !valid_key(binding) {
            return None;
        }
        parts.key = binding;
        Some(parts)
    }
}

impl From<ShortcutParts<'_>> for Shortcut {
    fn from(parts: ShortcutParts<'_>) -> Self {
        Self {
            key: parts.key.to_ascii_lowercase(),
            primary: parts.primary,
            alt: parts.alt,
            shift: parts.shift,
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/shortcut_alphanumeric.rs"));

// str::chars is not const. The input is a nonempty str, so it starts with a
// complete UTF-8 scalar and its leading byte determines the number of bytes.
const fn take_char(text: &str) -> (char, &str) {
    let bytes = text.as_bytes();
    if bytes[0].is_ascii() {
        return (bytes[0] as char, text.split_at(1).1);
    }
    let width = bytes[0].leading_ones() as usize;
    let mut scalar = (bytes[0] & (0x7f >> width)) as u32;
    let mut i = 1;
    while i < width {
        scalar = (scalar << 6) | (bytes[i] & 0x3f) as u32;
        i += 1;
    }
    (char::from_u32(scalar).unwrap(), text.split_at(width).1)
}

const fn valid_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let (first, mut rest) = take_char(key);
    if rest.is_empty() {
        return !first.is_whitespace();
    }
    if !is_alphanumeric(first) {
        return false;
    }
    while !rest.is_empty() {
        let (next, tail) = take_char(rest);
        if !is_alphanumeric(next) {
            return false;
        }
        rest = tail;
    }
    true
}

impl std::str::FromStr for Shortcut {
    type Err = crate::Error;

    fn from_str(binding: &str) -> Result<Self, Self::Err> {
        ShortcutParts::parse(binding)
            .map(Self::from)
            .ok_or_else(|| crate::Error::InvalidSchema(format!("invalid shortcut '{binding}'")))
    }
}

/// Construct a shortcut from a checked literal. Parse user input with `str::parse`.
///
/// ```
/// assert_eq!(gpdoc_core::key!("Shift-Mod-B"), "Mod-Shift-b".parse().unwrap());
/// ```
///
/// ```compile_fail,E0080
/// let _ = gpdoc_core::key!("Mood-b");
/// ```
#[macro_export]
macro_rules! key {
    ($key:literal) => {{
        const PARTS: $crate::ShortcutParts<'static> = match $crate::ShortcutParts::parse($key) {
            ::core::option::Option::Some(parts) => parts,
            ::core::option::Option::None => {
                ::core::panic!("{}", ::core::concat!("invalid shortcut '", $key, "'"))
            }
        };
        $crate::Shortcut::from(PARTS)
    }};
}

impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.primary {
            f.write_str("Mod-")?;
        }
        if self.alt {
            f.write_str("Alt-")?;
        }
        if self.shift {
            f.write_str("Shift-")?;
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_settings_use_the_same_key_grammar() {
        for (input, shortcut, normalized) in [
            (
                "aLt-sHift-mOd-B",
                key!("aLt-sHift-mOd-B"),
                "Mod-Alt-Shift-b",
            ),
            ("Mod--", key!("Mod--"), "Mod--"),
            ("-", key!("-"), "-"),
            ("{", key!("{"), "{"),
            ("Alt-한글", key!("Alt-한글"), "Alt-한글"),
            ("Mod-😀", key!("Mod-😀"), "Mod-😀"),
            ("é三𝟙", key!("é三𝟙"), "é三𝟙"),
        ] {
            assert_eq!(input.parse::<Shortcut>().unwrap(), shortcut);
            assert_eq!(shortcut.to_string(), normalized);
        }
        for invalid in [
            "",
            "--",
            "-a",
            "mod-",
            "mod---",
            "mod--b",
            "mod-MOD-b",
            "Mood-b",
            "a😀",
            "e\u{301}",
            "\u{a0}",
            "Shift-\u{3000}",
        ] {
            assert!(invalid.parse::<Shortcut>().is_err(), "{invalid:?}");
        }
    }
}
