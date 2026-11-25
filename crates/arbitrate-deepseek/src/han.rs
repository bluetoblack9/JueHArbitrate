//! Billing character count for DeepSeek text-to-speech.

include!("han_ranges.rs");

pub const TTS_MAX_BILLABLE_CHARS: usize = 10_000;

/// Each Unicode scalar counts once. Scalars with Script=Han count twice.
pub fn tts_billable_chars(text: &str) -> usize {
    text.chars()
        .map(|ch| {
            let cp = u32::from(ch);
            let index = HAN_RANGES.partition_point(|&(_, end)| end < cp);
            1 + usize::from(HAN_RANGES.get(index).is_some_and(|&(start, _)| cp >= start))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{HAN_RANGES, TTS_MAX_BILLABLE_CHARS, tts_billable_chars};

    const SCRIPTS: &str = include_str!("../../../tests/fixtures/unicode/Scripts-17.0.0.txt");

    fn han_ranges() -> Vec<(u32, u32)> {
        assert!(SCRIPTS.starts_with("# Scripts-17.0.0.txt"));
        let mut ranges = Vec::new();
        for line in SCRIPTS.lines() {
            let fields = line.split('#').next().unwrap().trim();
            if fields.is_empty() {
                continue;
            }
            let (range, script) = fields.split_once(';').unwrap();
            if script.trim() != "Han" {
                continue;
            }
            let range = range.trim();
            let (start, end) = range.split_once("..").unwrap_or((range, range));
            let start = u32::from_str_radix(start, 16).unwrap();
            let end = u32::from_str_radix(end, 16).unwrap();
            assert!(start <= end);
            ranges.push((start, end));
        }
        ranges.sort_unstable();
        assert!(!ranges.is_empty());
        assert!(ranges.windows(2).all(|pair| pair[0].1 < pair[1].0));
        ranges
    }

    #[test]
    fn compiled_ranges_match_the_pinned_scripts_file() {
        assert_eq!(HAN_RANGES, han_ranges());
    }

    #[test]
    #[ignore = "regenerates the Rust table from the pinned Unicode data"]
    fn regenerate_han_ranges() {
        use std::fmt::Write;
        use std::path::Path;

        let mut source = String::from(concat!(
            "// Generated from Unicode 17.0.0 Scripts.txt. See tests/fixtures/unicode/LICENSE.txt.\n",
            "#[rustfmt::skip]\n",
            "const HAN_RANGES: &[(u32, u32)] = &[\n",
        ));
        for (start, end) in han_ranges() {
            writeln!(&mut source, "    (0x{start:04X}, 0x{end:04X}),").unwrap();
        }
        source.push_str("];\n");
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/han_ranges.rs");
        std::fs::write(path, source).unwrap();
    }

    #[test]
    fn counts_script_han_rather_than_unicode_blocks() {
        assert_eq!(tts_billable_chars(""), 0);
        for ch in ['a', '、', '〆', '\u{2E9A}', '🦀'] {
            assert_eq!(tts_billable_chars(ch.encode_utf8(&mut [0; 4])), 1, "{ch}");
        }
        for ch in [
            '々',
            '\u{2E80}',
            '\u{2F00}',
            '\u{3021}',
            '\u{3038}',
            '\u{20000}',
            '\u{2EBF0}',
            '\u{30000}',
            '\u{31350}',
        ] {
            assert_eq!(tts_billable_chars(ch.encode_utf8(&mut [0; 4])), 2, "{ch}");
        }
    }

    #[test]
    fn counts_mixed_text_around_the_billing_limit() {
        assert_eq!(TTS_MAX_BILLABLE_CHARS, 10_000);
        let han = "汉".repeat(4_999);
        for (suffix, expected) in [("a", 9_999), ("ab", 10_000), ("abc", 10_001)] {
            assert_eq!(tts_billable_chars(&format!("{han}{suffix}")), expected);
        }
    }
}
