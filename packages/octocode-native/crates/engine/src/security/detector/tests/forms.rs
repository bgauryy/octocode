//! Which compiled form of the patterns a text is searched in, and the handles
//! a thread searches them through.

use super::super::regex_forms::{
    Classes, ascii_classes, ascii_pattern_regex, pattern_regex, with_handles,
};
use super::*;

/// Characters outside ASCII that no `\w`, `\d` or `\s` holds: punctuation,
/// symbols, arrows, emoji, a CJK comma.
const SYMBOLS: [char; 10] = [
    '…',
    '—',
    '→',
    '✓',
    '©',
    '€',
    '“',
    '”',
    '\u{1F389}',
    '\u{3001}',
];

/// A text whose characters outside ASCII are all symbols is searched in the
/// ASCII forms; one letter, digit, mark, connector or space outside ASCII,
/// a case variant of an ASCII letter among them, sends it to the full forms.
#[test]
fn a_text_whose_other_characters_are_symbols_is_searched_in_the_ascii_forms() {
    for text in [
        "plain ascii",
        "let note = \"done … next → ✓\"; // © 2026 “quoted”",
        "\u{1F389} party\u{3001}then € 5",
    ] {
        assert_eq!(Classes::of(text), Classes::Ascii, "{text:?}");
    }
    for member in [
        'é', 'ſ', '\u{212A}', '中', 'λ', '\u{0663}', '\u{00A0}', '\u{2003}', '\u{200D}',
        '\u{203F}', '\u{0301}',
    ] {
        let text = format!("token … {member} →");
        assert_eq!(Classes::of(&text), Classes::Unicode, "{member:?}");
    }
}

/// Each class the ASCII form rewrites (`\w`, `\d`, `\s`, inside a bracket or
/// out, negated, case-insensitive) holds exactly what the full form's holds on
/// every character a text searched in the ASCII forms may hold: every ASCII
/// character, and every other one outside the full classes.
#[test]
fn both_forms_of_a_class_agree_on_every_character_an_ascii_searched_text_holds() {
    let atoms = [
        r"\w",
        r"\d",
        r"\s",
        r"(?i)\w",
        r"(?i)[\w-]",
        r"[\w.+/=-]",
        r#"[^\s"'`]"#,
        r"(?i)[^\s]",
        r"[^\d\s]",
    ];
    let pairs: Vec<(regex::Regex, regex::Regex)> = atoms
        .iter()
        .map(|atom| {
            let full = format!("^(?:{atom})$");
            (
                regex::Regex::new(&full).expect(atom),
                regex::Regex::new(&ascii_classes(&full)).expect(atom),
            )
        })
        .collect();
    let mut buffer = [0u8; 4];
    for ch in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
        let text: &str = ch.encode_utf8(&mut buffer);
        if Classes::of(text) == Classes::Unicode {
            continue;
        }
        for ((full, ascii), atom) in pairs.iter().zip(atoms) {
            assert_eq!(
                full.is_match(text),
                ascii.is_match(text),
                "{atom} on {ch:?} (U+{:04X})",
                u32::from(ch)
            );
        }
    }
}

/// On a sample of each pattern with symbols outside ASCII written around and
/// inside it, the ASCII form finds every match, and every group of it, that
/// the full form finds. A sample that holds a Unicode class member itself (a
/// case variant `ſ` of a `(?i)` letter) is searched in the full forms, and
/// the symbols leave it there.
#[test]
fn each_pattern_finds_in_ascii_form_what_its_full_form_finds_beside_symbols() {
    let mut rng = Lcg(0x5ab1);
    let mut compared = 0;
    for (idx, pattern) in PATTERNS.iter().enumerate() {
        for sample in matching_samples(idx, &mut rng) {
            let classes = Classes::of(&sample);
            let mut text = String::new();
            for (at, ch) in sample.chars().enumerate() {
                text.push(ch);
                if at % 7 == 3 {
                    text.push(SYMBOLS[rng.below(SYMBOLS.len())]);
                }
            }
            for text in [format!("{}{sample}{}", SYMBOLS[0], SYMBOLS[8]), text] {
                assert_eq!(Classes::of(&text), classes, "{text:?}");
                if classes == Classes::Unicode {
                    continue;
                }
                compared += 1;
                let groups = |regex: &regex::Regex| {
                    regex
                        .captures_iter(&text)
                        .map(|captures| {
                            captures
                                .iter()
                                .map(|group| group.map(|group| group.range()))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    groups(ascii_pattern_regex(idx)),
                    groups(pattern_regex(idx)),
                    "{}: {text:?}",
                    pattern.name
                );
            }
        }
    }
    assert!(compared > PATTERNS.len(), "{compared} texts compared");
}

/// A thread searches through handles of its own: the same handle on each of
/// its searches, never the regex every thread shares, and never another
/// thread's.
#[test]
fn a_thread_searches_each_pattern_through_its_own_handle() {
    let handle = |idx: usize| {
        with_handles(|handles| std::ptr::from_ref(handles.regex(Classes::Ascii, idx)) as usize)
    };
    for idx in [0, PATTERNS.len() - 1] {
        let first = handle(idx);
        assert_eq!(handle(idx), first);
        assert_ne!(
            first,
            std::ptr::from_ref(Classes::Ascii.regex(idx)) as usize
        );
        let other = std::thread::spawn(move || handle(idx))
            .join()
            .expect("searching thread");
        assert_ne!(other, first);
    }
}
