use crate::error::MergeError;
use memchr::memchr2_iter;
use std::str::from_utf8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Eol {
    Lf,
    CrLf,
    Cr,
    None,
}

#[derive(Debug, Clone)]
pub struct Line<'a> {
    pub content: &'a str,
    pub eol: Eol,
}

/// Split input bytes into lines, tracking EOL style separately from content.
///
/// Manual byte scanning instead of `str::lines()` because the merge tool
/// needs to preserve and compare EOL styles across platforms.
pub fn tokenize(input: &[u8]) -> Result<Vec<Line<'_>>, MergeError> {
    let text = from_utf8(input).map_err(|e| MergeError::Utf8 {
        position: e.valid_up_to(),
        cause: e,
    })?;
    let bytes = text.as_bytes();

    let mut lines = Vec::new();
    let mut start = 0;

    for pos in memchr2_iter(b'\n', b'\r', bytes) {
        if pos < start {
            continue;
        }
        let eol = if bytes[pos] == b'\r' {
            if bytes.get(pos + 1) == Some(&b'\n') {
                Eol::CrLf
            } else {
                Eol::Cr
            }
        } else {
            Eol::Lf
        };
        lines.push(Line {
            content: &text[start..pos],
            eol,
        });
        start = pos + if eol == Eol::CrLf { 2 } else { 1 };
    }

    if start < bytes.len() {
        lines.push(Line {
            content: &text[start..],
            eol: Eol::None,
        });
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(input: &[u8]) -> Vec<(&str, Eol)> {
        tokenize(input)
            .unwrap()
            .iter()
            .map(|l| (l.content, l.eol))
            .collect()
    }

    #[test]
    fn lf_and_crlf() {
        // the only two that matter in practice; bare \r is OS9 legacy
        assert_eq!(
            contents(b"hello\nworld\r\n"),
            vec![("hello", Eol::Lf), ("world", Eol::CrLf)]
        );
    }

    #[test]
    fn empty_and_bare_newline() {
        assert_eq!(contents(b""), vec![]);
        assert_eq!(contents(b"\n"), vec![("", Eol::Lf)]);
    }

    #[test]
    fn missing_trailing_newline() {
        // git complains about this; make sure we keep it as Eol::None
        // so write-back preserves it byte-for-byte
        assert_eq!(
            contents(b"Aa\nBb"),
            vec![("Aa", Eol::Lf), ("Bb", Eol::None)]
        );
    }

    #[test]
    fn cr_only_kept_for_split() {
        assert_eq!(
            contents(b"hello\rworld\r"),
            vec![("hello", Eol::Cr), ("world", Eol::Cr)]
        );
    }

    #[test]
    fn non_utf8_returns_error() {
        let bad = vec![0xFF, 0xFE, b'\n'];
        assert!(tokenize(&bad).is_err());
    }

    #[test]
    fn utf16_bom_rejected() {
        // UTF-16 LE BOM (0xFF 0xFE) followed by ASCII encoded as UTF-16
        let utf16le = vec![0xFF, 0xFE, b'h', 0x00, b'i', 0x00];
        assert!(tokenize(&utf16le).is_err());
    }

    #[test]
    fn iso_8859_1_rejected() {
        // "café" in ISO-8859-1: 0xE9 is é in Latin-1 but invalid UTF-8
        let latin1 = vec![b'c', b'a', b'f', 0xE9, b'\n'];
        assert!(tokenize(&latin1).is_err());
    }

    #[test]
    fn utf8_with_accents_accepted() {
        // "café" properly encoded in UTF-8: é = 0xC3 0xA9
        let utf8 = "café\n".as_bytes();
        let lines = tokenize(utf8).unwrap();
        assert_eq!(lines[0].content, "café");
    }

    #[test]
    fn utf8_multibyte_accepted() {
        // CJK, emoji — valid UTF-8, should tokenize fine
        let utf8 = "hello 世界\n🦀 rust\n".as_bytes();
        let lines = tokenize(utf8).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].content, "hello 世界");
        assert_eq!(lines[1].content, "🦀 rust");
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn reconstruct(lines: &[Line]) -> String {
            let mut out = String::new();
            for line in lines {
                out.push_str(line.content);
                match line.eol {
                    Eol::Lf => out.push('\n'),
                    Eol::CrLf => out.push_str("\r\n"),
                    Eol::Cr => out.push('\r'),
                    Eol::None => {}
                }
            }
            out
        }

        proptest! {
            // Each test ended up catching something:
            // roundtrip caught the \r\n split off-by-one
            // no \r in content
            // tail Eol matches trailing newline
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn roundtrip(input in "[^\x00]{0,500}") {
                let lines = tokenize(input.as_bytes()).unwrap();
                prop_assert_eq!(reconstruct(&lines), input);
            }

            #[test]
            fn no_newlines_in_content(input in "[^\x00]{0,500}") {
                let lines = tokenize(input.as_bytes()).unwrap();
                for line in &lines {
                    prop_assert!(!line.content.contains('\n'));
                    prop_assert!(!line.content.contains('\r'));
                }
            }

            #[test]
            fn last_line_eol(input in "[^\x00]{1,500}") {
                let lines = tokenize(input.as_bytes()).unwrap();
                if let Some(last) = lines.last() {
                    let ends_with_newline = input.ends_with('\n') || input.ends_with('\r');
                    if !ends_with_newline {
                        prop_assert_eq!(last.eol, Eol::None);
                    } else {
                        prop_assert_ne!(last.eol, Eol::None);
                    }
                }
            }
        }
    }
}
