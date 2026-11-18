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
mod tests;
