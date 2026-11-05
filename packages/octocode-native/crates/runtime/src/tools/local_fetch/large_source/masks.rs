//! What a read shows of a source that names a Kubernetes Secret or a JSON Web Key,
//! when no window of it can show what makes a value secret.
//!
//! A pass over every byte says whether the source names one ([`SourceNames`]), and
//! a walk of it says which bytes are secret. Those spans come after the pass that
//! holds the windows (`data` comes before `kind`), so they are written onto the
//! windows it held. A mask keeps a value's length: a window's offsets count the
//! source's bytes, and every page of a byte walk seeks by them. The placeholder
//! replaces it where a page is scanned ([`super::unjudged::Masked`]), so no text
//! shows how long a value was. A byte page's offsets still do, as they do for any
//! redaction in a line that pages by bytes.
use super::stream::seek;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::security::{
    Named, SecretForm, SecretSpan, SecretSpans, SourceWalk, every_value_spans,
};
use std::io::Read;

/// Source bytes the walk takes at once.
const READ_BYTES: usize = 64 * 1024;

/// What a source's windows must not show.
pub(super) enum Judged {
    /// These bytes, in order: all that is secret.
    Exact(Vec<SecretSpan>),
    /// A walk could not keep the source's structure, so no value in it can be told
    /// from another: every value of what it names is hidden.
    EveryValue(Named),
}

impl Judged {
    /// About how many bytes this holds.
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Exact(spans) => spans.capacity() * std::mem::size_of::<SecretSpan>(),
            Self::EveryValue(_) => 0,
        }
    }
}

/// Walk the source behind `file`, whose bytes a pass proved to be `digest`. Bytes
/// that no longer are fail the read: its windows came from the pass.
pub(super) fn judge(
    file: &std::fs::File,
    named: Named,
    digest: &str,
    budget: usize,
    cancel: &impl CancellationCheck,
) -> Result<Judged, String> {
    if !named.any() {
        return Ok(Judged::Exact(Vec::new()));
    }
    seek(file, 0)?;
    let mut walk = SourceWalk::new(named, budget);
    let mut hasher = octocode_engine::digest::HashWriter::default();
    let mut buffer = vec![0u8; READ_BYTES];
    loop {
        cancel.check()?;
        let read = match (&mut &*file).read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.to_string()),
        };
        super::count_read(read);
        hasher.update(&buffer[..read]);
        walk.feed(&buffer[..read]);
    }
    if hex::encode(hasher.finalize()) != digest {
        return Err("the source changed while it was read; read it again".to_owned());
    }
    Ok(match walk.finish() {
        SecretSpans::Exact(spans) => Judged::Exact(spans),
        SecretSpans::TooMany => Judged::EveryValue(named),
    })
}

/// The mask byte of `form`. No text holds it by chance where it counts: only a
/// window a mask was written on has its masks turned to placeholders, and such a
/// source's stray control byte is at worst hidden too.
const fn mask_byte(form: SecretForm) -> u8 {
    match form {
        SecretForm::SecretInJson => 1,
        SecretForm::JsonWebKey => 2,
        SecretForm::SecretInYaml => 3,
    }
}

/// `text` with each run of a mask byte turned into its form's placeholder.
pub(super) fn with_placeholders(text: &str) -> std::borrow::Cow<'_, str> {
    let form_of = |byte: u8| {
        [
            SecretForm::SecretInJson,
            SecretForm::JsonWebKey,
            SecretForm::SecretInYaml,
        ]
        .into_iter()
        .find(|form| mask_byte(*form) == byte)
    };
    let bytes = text.as_bytes();
    let Some(mut masked) = bytes.iter().position(|byte| form_of(*byte).is_some()) else {
        return text.into();
    };
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    loop {
        out.push_str(&text[from..masked]);
        let byte = bytes[masked];
        from = masked + bytes[masked..].iter().take_while(|at| **at == byte).count();
        if let Some(form) = form_of(byte) {
            out.push_str(&octocode_engine::security::redaction_placeholder(
                form.pattern(),
            ));
        }
        match bytes[from..]
            .iter()
            .position(|byte| form_of(*byte).is_some())
        {
            Some(next) => masked = from + next,
            None => break,
        }
    }
    out.push_str(&text[from..]);
    out.into()
}

/// Mask `window[from..to]`, each line from its first byte that is not a blank: a
/// span's indentation stays, and a span of blanks hides nothing. Whether any mask
/// was written.
pub(super) fn cover(window: &mut [u8], from: usize, to: usize, form: SecretForm) -> bool {
    let mut covered = false;
    let mut in_blanks = true;
    for byte in &mut window[from..to] {
        match *byte {
            b'\n' => in_blanks = true,
            b' ' | b'\t' | b'\r' if in_blanks => {}
            _ => {
                in_blanks = false;
                *byte = mask_byte(form);
                covered = true;
            }
        }
    }
    covered
}

/// Mask every value a window of what `named` says can hold: the fallback when a
/// walk could not keep the source's structure. Whether any was masked.
pub(super) fn cover_every_value(window: &mut [u8], cut: bool, named: Named) -> bool {
    let mut covered = false;
    for span in every_value_spans(window, cut, named) {
        covered |= cover(window, span.range.start, span.range.end, span.form);
    }
    covered
}
