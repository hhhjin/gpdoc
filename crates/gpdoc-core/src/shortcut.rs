/// A single logical keystroke. The primary modifier is Command on macOS and Control elsewhere.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Shortcut {
    key: String,
    pub primary: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Shortcut {
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
}

impl std::str::FromStr for Shortcut {
    type Err = crate::Error;
    fn from_str(binding: &str) -> Result<Self, Self::Err> {
        let invalid = || crate::Error::InvalidSchema(format!("invalid shortcut '{binding}'"));
        // A trailing minus is a key, including in "Mod--".
        let (modifiers, key) = match binding {
            "-" => ("", "-"),
            s if s.starts_with('-') => return Err(invalid()),
            s if s.ends_with("--") => (&s[..s.len() - 2], "-"),
            s => s.rsplit_once('-').unwrap_or(("", s)),
        };
        if key.is_empty()
            || !(key.chars().all(char::is_alphanumeric)
                || key.chars().count() == 1 && !key.chars().any(char::is_whitespace))
        {
            return Err(invalid());
        }
        let mut shortcut = Self::new(key);
        if !modifiers.is_empty() {
            for modifier in modifiers.split('-') {
                let flag = match modifier.to_ascii_lowercase().as_str() {
                    "mod" => &mut shortcut.primary,
                    "alt" => &mut shortcut.alt,
                    "shift" => &mut shortcut.shift,
                    _ => return Err(invalid()),
                };
                if *flag {
                    return Err(invalid());
                }
                *flag = true;
            }
        }
        Ok(shortcut)
    }
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
