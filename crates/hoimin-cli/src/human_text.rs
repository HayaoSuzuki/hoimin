//! Inert, single-line projections of external text for errors, help and warnings.

use std::fmt::{self, Write};

/// Escapes controls and Unicode line separators while preserving ordinary Unicode.
/// Apply once to external fields in errors/help/warnings, never to logical JSON data
/// or to a complete diagnostic whose layout belongs to its renderer.
pub(crate) struct HumanText<'a>(pub(crate) &'a str);

impl fmt::Display for HumanText<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for character in self.0.chars() {
            if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}') {
                write!(formatter, "{}", character.escape_debug())?;
            } else {
                formatter.write_char(character)?;
            }
        }
        Ok(())
    }
}

/// Project Clap's interpolated context, keeping its generated diagnostic layout.
/// Styled tips have mixed provenance: each inline body is rendered as plain text
/// and escaped once. Clap still owns the surrounding tip labels, usage and lines.
pub(crate) fn escape_clap_context(error: &mut clap::Error) {
    use clap::error::{ContextKind, ContextValue};

    let replacements = error
        .context()
        .filter_map(|(kind, value)| {
            let value = match value {
                ContextValue::String(value) => ContextValue::String(HumanText(value).to_string()),
                ContextValue::Strings(values) => ContextValue::Strings(
                    values
                        .iter()
                        .map(|value| HumanText(value).to_string())
                        .collect(),
                ),
                ContextValue::StyledStrs(values) if kind == ContextKind::Suggested => {
                    ContextValue::StyledStrs(
                        values
                            .iter()
                            .map(|value| HumanText(&value.to_string()).to_string().into())
                            .collect(),
                    )
                }
                _ => return None,
            };
            Some((kind, value))
        })
        .collect::<Vec<_>>();
    for (kind, value) in replacements {
        error.insert(kind, value);
    }
}

#[cfg(test)]
mod tests {
    use super::HumanText;
    use proptest::prelude::*;

    #[test]
    fn clap_contexts_escape_overlapping_values_without_changing_usage_layout() {
        use clap::error::{ContextKind, ContextValue, ErrorKind};
        let mut error = clap::Error::new(ErrorKind::ArgumentConflict);
        error.insert(ContextKind::InvalidArg, ContextValue::String("\n".into()));
        error.insert(
            ContextKind::PriorArg,
            ContextValue::Strings(vec!["\t\n".into(), "\n".into()]),
        );
        error.insert(
            ContextKind::Usage,
            ContextValue::StyledStr("Usage: hoimin run\n  [OPTIONS]".into()),
        );
        error.insert(
            ContextKind::Suggested,
            ContextValue::StyledStrs(vec!["inline\n\ttip".into()]),
        );

        super::escape_clap_context(&mut error);
        let rendered = error.to_string();

        assert!(
            rendered.contains("'\\n' cannot be used with"),
            "{rendered:?}"
        );
        assert!(rendered.contains("\\t\\n"), "{rendered:?}");
        assert!(rendered.contains("tip: inline\\n\\ttip"), "{rendered:?}");
        assert!(
            rendered.contains("\n\nUsage: hoimin run\n  [OPTIONS]"),
            "{rendered:?}"
        );
        assert!(
            !rendered.chars().any(|c| c.is_control() && c != '\n'),
            "{rendered:?}"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
        #[test]
        fn projections_never_emit_controls_or_line_separators(raw in any::<String>()) {
            let text = HumanText(&raw).to_string();

            prop_assert!(!text.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')), "{:?}", text);
        }
    }
}
