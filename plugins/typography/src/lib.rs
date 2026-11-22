//! Smart punctuation, symbols, fractions and multiplication signs while typing.
use gpdoc_plugin::prelude::*;
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct TypographyOptions {
    /// Tiptap rule name mapped to a replacement. Use None to disable that rule.
    pub replacements: BTreeMap<String, Option<String>>,
    pub rtl_quotes: bool,
}

pub fn typography(options: TypographyOptions) -> Plugin {
    let rules = [
        ("emDash", r"--$", "—"),
        ("ellipsis", r"\.\.\.$", "…"),
        (
            "openDoubleQuote",
            r#"(?:^|[\s{\[(<'"‘“])(")$"#,
            if options.rtl_quotes { "”" } else { "“" },
        ),
        (
            "closeDoubleQuote",
            r#""$"#,
            if options.rtl_quotes { "“" } else { "”" },
        ),
        (
            "openSingleQuote",
            r#"(?:^|[\s{\[(<'"‘“])(')$"#,
            if options.rtl_quotes { "’" } else { "‘" },
        ),
        (
            "closeSingleQuote",
            "'$",
            if options.rtl_quotes { "‘" } else { "’" },
        ),
        ("leftArrow", "<-$", "←"),
        ("rightArrow", "->$", "→"),
        ("copyright", r"\(c\)$", "©"),
        ("trademark", r"\(tm\)$", "™"),
        ("servicemark", r"\(sm\)$", "℠"),
        ("registeredTrademark", r"\(r\)$", "®"),
        ("oneHalf", r"(?:^|\s)(1/2)\s$", "½"),
        ("plusMinus", r"\+/-$", "±"),
        ("notEqual", "!=$", "≠"),
        ("laquo", "<<$", "«"),
        ("raquo", ">>$", "»"),
        ("multiplication", r"[0-9]+\s?([*x])\s?[0-9]+$", "×"),
        ("superscriptTwo", r"\^2$", "²"),
        ("superscriptThree", r"\^3$", "³"),
        ("oneQuarter", r"(?:^|\s)(1/4)\s$", "¼"),
        ("threeQuarters", r"(?:^|\s)(3/4)\s$", "¾"),
    ]
    .into_iter()
    .filter_map(|(name, pattern, default)| {
        let replacement = match options.replacements.get(name) {
            Some(value) => value.clone()?,
            None => default.to_owned(),
        };
        Some((
            Regex::new(pattern).expect("typography rule regex"),
            replacement,
        ))
    })
    .collect::<Vec<_>>();
    Plugin::new("typography", |p| {
        p.input_rule(move |ctx| {
            let Selection::Text { anchor, head } = ctx.selection else {
                return Ok(None);
            };
            if anchor != head || head != ctx.inserted.end || ctx.inserted.is_empty() {
                return Ok(None);
            }
            let pos = ctx.document.resolve(head)?;
            if pos.parent().kind == "codeBlock"
                || pos.marks(ctx.schema).iter().any(|m| m.kind == "code")
            {
                return Ok(None);
            }
            let prefix = pos
                .parent()
                .text_between(0, pos.parent_offset, "", "\u{fffc}")?;
            for (regex, replacement) in &rules {
                if let Some(capture) = regex.captures(&prefix) {
                    let matched = capture.get(1).unwrap_or_else(|| capture.get(0).unwrap());
                    let from = head.0 - prefix[matched.start()..].encode_utf16().count();
                    let to = head.0 - prefix[matched.end()..].encode_utf16().count();
                    return Ok(Some(vec![Edit::ReplaceText {
                        range: DocPos(from)..DocPos(to),
                        text: replacement.clone(),
                    }]));
                }
            }
            Ok(None)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpdoc_plugin_paragraph::paragraph;
    #[test]
    fn punctuation_matches_at_utf16_caret_and_preserves_context() {
        let config = gpdoc_plugin::core::EditorConfig::builder()
            .plugins([paragraph(), typography(TypographyOptions::default())])
            .build()
            .unwrap();
        let mut editor =
            gpdoc_plugin::core::Editor::from_document(&config, vec![Node::text("paragraph", "")])
                .unwrap();
        for ch in "😀-- \"1/2\" 1/4 4 x 3 (c) ... ".chars() {
            editor.insert_text(&ch.to_string()).unwrap();
        }
        assert_eq!(
            editor.document().blocks(),
            [Node::text("paragraph", "😀— “1/2” ¼ 4 × 3 © … ")]
        );
    }
}
