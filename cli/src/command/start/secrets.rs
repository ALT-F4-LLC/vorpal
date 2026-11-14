//! The secret name rule and the secret reference matcher, shared by the agent and the worker.
//!
//! A secret name becomes a file name, an environment key and an expansion target, so one
//! rule decides which names are allowed everywhere.

use std::{collections::HashSet, fmt, ops::Range};

/// Names with this prefix would shadow the variables the worker defines for every step.
const RESERVED_PREFIX: &str = "VORPAL_";

/// Why a list of secret names was rejected. Each variant holds the offending name, which
/// `Display` prints escaped so that control characters cannot forge log lines.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SecretNameError {
    Invalid(String),
    Reserved(String),
    Duplicate(String),
}

impl fmt::Display for SecretNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(name) => {
                write!(f, "secret name {name:?} must match [A-Za-z_][A-Za-z0-9_]*")
            }
            Self::Reserved(name) => {
                write!(
                    f,
                    "secret name {name:?} must not start with {RESERVED_PREFIX}"
                )
            }
            Self::Duplicate(name) => write!(f, "secret name {name:?} is repeated"),
        }
    }
}

impl std::error::Error for SecretNameError {}

/// Accepts the names only when each matches `[A-Za-z_][A-Za-z0-9_]*`, none starts with
/// `VORPAL_`, and none repeats. The first offending name is returned.
pub(super) fn validate_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), SecretNameError> {
    let mut seen = HashSet::new();

    for name in names {
        let mut bytes = name.bytes();
        let valid = bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');

        if !valid {
            return Err(SecretNameError::Invalid(name.to_string()));
        }

        if name.starts_with(RESERVED_PREFIX) {
            return Err(SecretNameError::Reserved(name.to_string()));
        }

        if !seen.insert(name) {
            return Err(SecretNameError::Duplicate(name.to_string()));
        }
    }

    Ok(())
}

/// Returns the byte ranges of `text` that reference `name`, in order and non-overlapping.
///
/// `${NAME}` is a reference anywhere. `$NAME` is a reference only at the end of `text` or
/// before a character that cannot continue a name and is not `{`, so `$NAMEX` and `$NAME{`
/// do not reference `NAME`.
pub(super) fn references(text: &str, name: &str) -> Vec<Range<usize>> {
    let braced = format!("${{{name}}}");
    let unbraced = format!("${name}");
    let mut spans = Vec::new();
    let mut from = 0;

    while let Some(offset) = text[from..].find('$') {
        let start = from + offset;
        let rest = &text[start..];

        let length = if rest.starts_with(&braced) {
            Some(braced.len())
        } else if rest.starts_with(&unbraced) && ends_reference(&rest[unbraced.len()..]) {
            Some(unbraced.len())
        } else {
            None
        };

        match length {
            Some(length) => {
                spans.push(start..start + length);
                from = start + length;
            }
            None => from = start + 1,
        }
    }

    spans
}

fn ends_reference(after: &str) -> bool {
    after
        .chars()
        .next()
        .is_none_or(|c| c != '{' && c != '_' && !c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{references, validate_names};

    #[test]
    fn accepts_names_matching_the_rule() {
        assert_eq!(
            validate_names(["TOKEN", "_a1", "api_key", "VORPALX"]),
            Ok(())
        );
        assert_eq!(validate_names(["MY_VORPAL_", "x"]), Ok(()));
        assert_eq!(validate_names([]), Ok(()));
    }

    #[test]
    fn finds_braced_and_delimited_references() {
        assert_eq!(references("$NAME", "NAME"), vec![0..5]);
        assert_eq!(references("${NAME}", "NAME"), vec![0..7]);
        assert_eq!(references("x$NAME.y", "NAME"), vec![1..6]);
        assert_eq!(references("${NAME}x", "NAME"), vec![0..7]);
        assert_eq!(references("$NAME-a ${NAME}", "NAME"), vec![0..5, 8..15]);
    }

    #[test]
    fn longer_names_and_brace_suffixes_are_not_references() {
        assert_eq!(
            references("$NAMEX", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
        assert_eq!(
            references("$NAME_1", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
        assert_eq!(
            references("$NAME{", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
        assert_eq!(
            references("$NAMEé", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
        assert_eq!(
            references("${NAMEX}", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
        assert_eq!(
            references("NAME", "NAME"),
            [] as [std::ops::Range<usize>; 0]
        );
    }
}

#[cfg(test)]
mod abuse {
    use super::{validate_names, SecretNameError};

    #[test]
    fn abuse_path_and_control_characters_are_rejected() {
        for name in [
            "a/b", "/etc", "..", "../x", "a\0b", "a b", "a\nb", "", "A=B", "é",
        ] {
            assert_eq!(
                validate_names([name]),
                Err(SecretNameError::Invalid(name.to_string())),
                "{name:?}"
            );
        }
        assert_eq!(
            validate_names(["1A"]),
            Err(SecretNameError::Invalid("1A".to_string()))
        );
    }

    #[test]
    fn abuse_vorpal_prefix_is_rejected() {
        for name in ["VORPAL_", "VORPAL_OUTPUT", "VORPAL_SECRET_DIR"] {
            assert_eq!(
                validate_names([name]),
                Err(SecretNameError::Reserved(name.to_string())),
                "{name:?}"
            );
        }
    }

    #[test]
    fn abuse_repeated_name_is_rejected_naming_it() {
        let result = validate_names(["A", "B", "A"]);
        assert_eq!(result, Err(SecretNameError::Duplicate("A".to_string())));

        let message = result.err().map_or_default(|err| err.to_string());
        assert!(message.contains("\"A\""), "{message}");
    }

    #[test]
    fn abuse_rejected_name_is_escaped_in_the_error() {
        for name in [
            "a\nINFO forged",
            "a\r\nb",
            "a\x1b[2Jb",
            "a\0b",
            "a\u{202e}b",
        ] {
            let message = validate_names([name])
                .err()
                .map_or_default(|err| err.to_string());
            assert!(!message.is_empty(), "{name:?} was accepted");
            assert!(
                !message.chars().any(char::is_control),
                "{name:?} produced {message:?}"
            );
            assert!(
                message.contains(&name.escape_debug().to_string()),
                "{message}"
            );
        }
    }
}
