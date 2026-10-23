//! Shell words for printed commands (next steps, hints, helper scripts): the
//! one POSIX single-quote rule every crate shares. A caller that also prints
//! for another shell (PowerShell) keeps only that branch itself.

/// `arg` as one POSIX shell word: unchanged when every character is safe
/// unquoted, else single-quoted with each `'` written as `'\''`. An empty
/// argument is `''`, so it survives as a word.
pub fn quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@,=+".contains(c))
    {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn safe_words_stay_bare_and_everything_else_is_one_quoted_word() {
        assert_eq!(quote("src/a-b_c.rs:1@2,3=4+5"), "src/a-b_c.rs:1@2,3=4+5");
        assert_eq!(quote("two words"), "'two words'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$HOME;`x`"), "'$HOME;`x`'");
        assert_eq!(quote("a\nb"), "'a\nb'");
    }

    #[test]
    fn an_empty_argument_is_kept_as_an_empty_word() {
        assert_eq!(quote(""), "''");
    }
}
