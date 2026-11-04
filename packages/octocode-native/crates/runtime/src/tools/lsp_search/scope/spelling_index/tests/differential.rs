//! The entries' tokens against the search they stand in for: for every candidate
//! name, on real and generated text, an entry holds the name's hash exactly when
//! `file_spells` finds the name, and holds nothing else.
use super::*;

/// Maximal identifier runs of `text`, and each with one character off either
/// end: names that only start or end a longer word.
fn runs(text: &str, into: &mut BTreeSet<String>) {
    for run in text
        .split(|ch| !is_ident_char(ch))
        .filter(|r| !r.is_empty())
    {
        into.insert(run.to_owned());
        let mut chars = run.chars();
        chars.next();
        into.insert(chars.as_str().to_owned());
        let mut chars = run.chars();
        chars.next_back();
        into.insert(chars.as_str().to_owned());
    }
}

/// Every name a file's text could hold as a whole word, by each decoding the
/// search may apply, and some it does not.
fn candidates(raw: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    runs(&String::from_utf8_lossy(raw), &mut names);
    runs(&octocode_engine::text::decode(raw).text, &mut names);
    runs(&String::from_utf8_lossy(&searched_text(raw)), &mut names);
    names.remove("");
    names.extend(["foo", "f", "é", "名前", "$f", "_", "x1"].map(str::to_owned));
    names
}

/// Write `raw` to `file` and hold the entry to the search for every candidate.
fn entry_equals_search(file: &Path, raw: &[u8]) {
    std::fs::write(file, raw).expect("write");
    let tokens = token_hashes(&searched_text(raw));
    assert!(tokens.is_sorted(), "{raw:?}");
    let mut found = BTreeSet::new();
    for name in candidates(raw) {
        let spelled = file_spells(file, name.as_bytes(), None).expect("readable");
        let held = tokens.binary_search(&word_hash(name.as_bytes())).is_ok();
        assert_eq!(held, spelled, "{name:?} in {raw:?}");
        if spelled {
            found.insert(word_hash(name.as_bytes()));
        }
    }
    let held = tokens.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(held, found, "tokens no search finds, in {raw:?}");
}

/// Each case a search treats apart, with what the entry must hold.
#[test]
fn entries_hold_the_whole_words_of_each_text_kind() {
    let hashes = |names: &[&str]| {
        let mut hashes = names
            .iter()
            .map(|name| word_hash(name.as_bytes()))
            .collect::<Vec<_>>();
        hashes.sort_unstable();
        hashes
    };
    let utf16 = |bom: [u8; 2], text: &str, order: fn(u16) -> [u8; 2]| {
        let mut bytes = bom.to_vec();
        bytes.extend(text.encode_utf16().flat_map(order));
        bytes
    };
    let cases: Vec<(Vec<u8>, &[&str])> = vec![
        (b"".to_vec(), &[]),
        (
            b"foo bar_1 $baz qux$".to_vec(),
            &["foo", "bar_1", "$baz", "qux$"],
        ),
        ("é名前 — 𝐚b·c".as_bytes().to_vec(), &["é名前", "𝐚b·c"]),
        // A UTF-8 byte-order mark is no identifier character.
        (b"\xef\xbb\xbffoo".to_vec(), &["foo"]),
        // Undecodable bytes may be part of a word, on either side.
        (b"\xe9foo bar\xe9 baz".to_vec(), &["baz"]),
        (b"foo\xe2\x80".to_vec(), &[]),
        // A word ends at a NUL, and nothing past it is text.
        (b"foo\0bar".to_vec(), &["foo"]),
        (b"\0foo".to_vec(), &[]),
        // A binary prefix holds no text at all.
        (b"\x01\x02\x03foo\0".to_vec(), &[]),
        // Without a NUL no text is judged binary.
        (b"\x01\x02\x03foo".to_vec(), &["foo"]),
        (
            utf16([0xff, 0xfe], "foo bar", u16::to_le_bytes),
            &["foo", "bar"],
        ),
        (
            utf16([0xfe, 0xff], "foo é", u16::to_be_bytes),
            &["foo", "é"],
        ),
        // A NUL is only a separator in UTF-16 text.
        (
            utf16([0xff, 0xfe], "foo\0bar", u16::to_le_bytes),
            &["foo", "bar"],
        ),
        // An odd length is not UTF-16: read as the search reads it, 8-bit, `ÿþ` joins.
        (b"\xff\xfefoo bar".to_vec(), &["bar", "ÿþfoo"]),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            &*token_hashes(&searched_text(&raw)),
            hashes(expected).as_slice(),
            "{raw:?}"
        );
    }
}

/// The source this repository is made of.
#[test]
fn entries_equal_the_search_on_real_source() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (_dir, root) = fixture();
    let copy = root.join("copy.rs");
    let mut files = 0;
    for dir in [
        "engine/src/text",
        "runtime/src/tools/lsp_search/scope",
        "../../octocode-chrome-devtools/src/engine",
    ] {
        for entry in ignore::Walk::new(crates.join(dir)).flatten() {
            if entry.file_type().is_some_and(|kind| kind.is_file()) {
                entry_equals_search(&copy, &std::fs::read(entry.path()).expect("source"));
                files += 1;
            }
        }
    }
    assert!(files > 10, "{files}");
}

/// Text assembled from what the rule treats apart, with every encoding.
#[test]
fn entries_equal_the_search_on_generated_text() {
    const WORDS: [&str; 18] = [
        "foo", "f", "_", "$", "1", "é", "名前", "𝐚", "٣", "·", "℘", "\u{200d}", "\u{301}", " ",
        ".", "—", "\u{feff}", "\u{a0}",
    ];
    const RAW: [&[u8]; 10] = [
        b"\0",
        b"\xe9",
        b"\xc3",
        b"\xe2\x80",
        b"\xf0\x9d",
        b"\x80",
        b"\x01\x02\x03",
        b"\n",
        b"\r\n",
        b"\xff",
    ];
    let (_dir, root) = fixture();
    let file = root.join("generated.ts");
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % bound as u64).expect("small")
    };
    for round in 0..3000 {
        let mut text = String::new();
        let mut raw = Vec::new();
        for _ in 0..next(14) {
            let word = WORDS[next(WORDS.len())];
            text.push_str(word);
            raw.extend_from_slice(word.as_bytes());
            if next(3) == 0 {
                raw.extend_from_slice(RAW[next(RAW.len())]);
            }
        }
        let units = text.encode_utf16();
        let bytes = match round % 8 {
            0 => {
                let mut marked = b"\xef\xbb\xbf".to_vec();
                marked.extend(&raw);
                marked
            }
            1 => [0xff, 0xfe]
                .into_iter()
                .chain(units.flat_map(u16::to_le_bytes))
                .collect(),
            2 => [0xfe, 0xff]
                .into_iter()
                .chain(units.flat_map(u16::to_be_bytes))
                .collect(),
            3 => {
                // Declared UTF-16 that does not decode: a lone surrogate or an odd length.
                let mut broken = vec![0xff, 0xfe];
                broken.extend(units.flat_map(u16::to_le_bytes));
                broken.extend_from_slice(if next(2) == 0 { b"\x00\xd8" } else { b"x" });
                broken
            }
            _ => raw,
        };
        entry_equals_search(&file, &bytes);
    }
}

/// The rule's own class is the tokenizer's, on every character.
#[test]
fn every_character_splits_or_joins_as_the_rule_says() {
    let (_dir, root) = fixture();
    let file = root.join("chars.ts");
    let sample = (0..=0x2_ffff_u32)
        .step_by(97)
        .chain(0..0x180)
        .filter_map(char::from_u32);
    for ch in sample {
        let text = format!("foo{ch}bar");
        let whole = !is_ident_char(ch);
        let tokens = token_hashes(text.as_bytes());
        assert_eq!(
            tokens.binary_search(&word_hash(b"foo")).is_ok(),
            whole,
            "{ch:?}"
        );
        assert_eq!(
            octocode_engine::text::is_whole_word(text.as_bytes(), 0, 3),
            whole,
            "{ch:?}"
        );
    }
    std::fs::write(&file, "\u{1d41a}foo").expect("write");
    assert_eq!(file_spells(&file, b"foo", None), Some(false));
}
