//! The rows of a source larger than one judged whole. A read of such a
//! source streams it and cuts every window from one judgment of the stream
//! ([`octocode_tools_shared::large_source::served_lines`]); a search row of it shows
//! the same decision: its lines as that read serves them, and a matchOnly
//! span cut from them by position. A row whose lines the read cannot serve
//! is withheld, never shown as the search matched it.

use super::guard::{HashingReader, Proof, ValueCheck, Verification, drain_digest, proof, unproven};
use super::lines::{VerificationLines, VerifyReadError, read_verification_lines};
use super::settle::{self, RowLines};
use super::*;
use crate::local_search::sources::{Digest, Expected};
use octocode_engine::security::Edits;

/// The raw lines of `rows`' spans, and the digest of every byte read.
fn raw_lines(
    opened: &std::fs::File,
    spans: &[(usize, usize)],
    cancel: &impl CancellationCheck,
) -> Result<(VerificationLines, Option<Digest>), VerifyReadError> {
    let mut reader = std::io::BufReader::new(HashingReader {
        inner: opened,
        hasher: Some(octocode_engine::digest::HashWriter::default()),
    });
    // Read to its end, as a source judged whole is: a line too long to
    // classify anywhere in it leaves its key-block state unknown.
    let read = read_verification_lines(&mut reader, spans, usize::MAX, false, cancel)?;
    let digest = drain_digest(reader, cancel)?;
    Ok((read, digest))
}

/// Spans of whole lines covering every span of `rows` (sorted, each inside
/// one): rows on the same or adjoining lines share a window while its raw
/// lines fit `room` bytes, so a page of spans on one line reads that line
/// once.
fn windows_of(
    rows: &[(usize, usize)],
    read: &VerificationLines,
    room: usize,
) -> Vec<(usize, usize)> {
    let bytes = |first: usize, last: usize| -> usize {
        (first..=last)
            .map(|line| read.retained.get(&line).map_or(0, |text| text.len() + 1))
            .sum()
    };
    let mut sorted = rows.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for (first, last) in sorted {
        match windows.last_mut() {
            Some(window)
                if first <= window.1 + 1 && bytes(window.0, last.max(window.1)) <= room =>
            {
                window.1 = window.1.max(last);
            }
            _ => windows.push((first, last)),
        }
    }
    windows
}

/// [`super::guard_source_secrets`] for `source`, a file of `meta.len()` bytes
/// past [`octocode_runtime_base::security::SourceLimits::whole_source`], open as `opened`.
pub(super) fn guard_large(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    (opened, meta): (&std::fs::File, &std::fs::Metadata),
    expected: Expected,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    let ValueCheck {
        security,
        shape,
        cancel,
        hidden,
        ..
    } = *check;
    if expected == Expected::Stored(None) {
        return Ok(Verification::Changed);
    }
    let scanned = file.source.map(|searched| searched.digest);
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let placed = file.matches[start..end]
        .iter()
        .map(|row| settle::row_lines(shape, row))
        .collect::<Vec<_>>();
    let spans = placed
        .iter()
        .flatten()
        .map(|lines| (lines.first, lines.first + lines.span.max(1) - 1))
        .collect::<Vec<_>>();
    if placed.is_empty() {
        return Ok(Verification::Verified);
    }
    let withhold = |file: &mut octocode_engine::types::TextSearchFile| {
        Ok(unproven(
            &mut file.matches[start..end],
            source,
            (security, hidden),
        ))
    };
    let (read, digest) = match raw_lines(opened, &spans, cancel) {
        Ok((read, Some(digest))) => (read, digest),
        Ok((_, None)) | Err(VerifyReadError::Io) => return withhold(file),
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    match proof(expected, scanned, digest) {
        Proof::Same => {}
        Proof::Changed => return Ok(Verification::Changed),
        Proof::Other => return withhold(file),
    }
    if read.unclassified {
        return withhold(file);
    }
    let room = security.limits().window / 2;
    let windows = windows_of(&spans, &read, room);
    let known = hex::encode(digest);
    // A read that serves nothing of this source says why on its own row: no
    // row of it is shown.
    let Ok(served) = octocode_tools_shared::large_source::served_lines(
        source,
        opened,
        meta,
        &windows,
        Some(&known),
        security,
        cancel,
    ) else {
        cancel.check()?;
        return withhold(file);
    };
    // The lines served are of other bytes than the rows' raw lines: the file
    // changed between the two reads.
    if served.digest != known {
        return if matches!(expected, Expected::Stored(_)) {
            Ok(Verification::Changed)
        } else {
            withhold(file)
        };
    }
    let judged = placed
        .iter()
        .map(|row| {
            row.and_then(|lines| {
                judged_lines(lines, &windows, &served.spans, (&read, room), source)
            })
        })
        .collect::<Vec<_>>();
    // A row the read serves no whole line of sits on a line longer than a
    // window, which the read pages by bytes: the search judged that line as
    // it matched it, and its window is settled as one of any long line is.
    let mut verified = Verification::Verified;
    let unserved = placed
        .iter()
        .zip(&judged)
        .any(|(row, judged)| row.is_some() && judged.is_none());
    let matched_values = unserved.then(|| {
        file.matches[start..end]
            .iter()
            .map(|row| row.value.clone())
            .collect::<Vec<_>>()
    });
    if unserved {
        verified = super::guard::guard_read(
            file,
            source,
            || octocode_engine::text::open_regular(source).map(|(file, _)| file),
            expected,
            start..end,
            check,
        )?;
        if verified == Verification::Changed {
            return Ok(verified);
        }
    }
    // Rows all cut from the read's judgment are judged leaves for the output
    // gate; with a row settled in its own window, the gate judges each.
    if !unserved {
        hidden.judged_whole();
    }
    for (index, (row, judged)) in placed.iter().zip(judged).enumerate() {
        let matched = &mut file.matches[start + index];
        let Some(lines) = *row else {
            // Its lines disagree with the window the search reports: nothing
            // places them in the source, so none is shown.
            matched.value = octocode_runtime_base::security::row_shown_unchecked(
                security,
                &matched.value,
                Some(source),
            );
            verified = Verification::Unverified;
            continue;
        };
        let Some(judged) = judged else {
            continue;
        };
        if let Some(value) = matched_values.as_ref().and_then(|values| values.get(index)) {
            matched.value.clone_from(value);
        }
        if matched.line == 0 {
            continue;
        }
        if matched.line as usize > served.total_lines {
            matched.value = octocode_runtime_base::security::row_shown_unchecked(
                security,
                &matched.value,
                Some(source),
            );
            verified = Verification::Unverified;
            continue;
        }
        // The judged text is the row's own lines, from its first.
        let own = [Some(RowLines { first: 1, ..lines })];
        let one = std::slice::from_mut(matched);
        if settle::settle_judged(
            one,
            &own,
            shape,
            &judged,
            (usize::MAX, hidden),
            (source, security),
        ) != Verification::Verified
        {
            verified = Verification::Unverified;
        }
    }
    Ok(verified)
}

/// The judgment of the lines `row` holds: its raw lines from `read` against
/// the lines the read serves of them (`served`, by `windows`). `None` when
/// either is missing, a line outgrows a window (`room`), or they are not line
/// for line the same source;
/// `Some(None)` when the read serves them as they are.
fn judged_lines(
    row: RowLines,
    windows: &[(usize, usize)],
    served: &[Option<Vec<String>>],
    (read, room): (&VerificationLines, usize),
    source: &std::path::Path,
) -> Option<settle::Verdict> {
    let last = row.first + row.span.max(1) - 1;
    let at = windows
        .iter()
        .position(|&(first, end)| first <= row.first && last <= end)?;
    let window = served.get(at)?.as_ref()?;
    let from = row.first - windows[at].0;
    let clean = window.get(from..from + row.span.max(1))?.join("\n");
    let raw = (row.first..=last)
        .map(|line| read.retained.get(&line).map(String::as_str))
        .collect::<Option<Vec<_>>>()?
        .join("\n");
    if raw == clean {
        return Some(None);
    }
    // A line longer than a window is served by bytes, not whole.
    if raw.len() > room {
        return None;
    }
    let redactions = octocode_tools_shared::large_source::redactions(&raw, &clean);
    // The redactions must turn the raw lines into the served ones exactly.
    let mut rebuilt = String::with_capacity(clean.len());
    let mut from = 0;
    for (range, text) in &redactions {
        rebuilt.push_str(raw.get(from..range.start)?);
        rebuilt.push_str(text);
        from = range.end;
    }
    rebuilt.push_str(raw.get(from..)?);
    if rebuilt != clean {
        return None;
    }
    let edits = Edits::of_pass(
        redactions
            .into_iter()
            .map(|(range, text)| (range, text.len())),
    );
    Some(Some(std::sync::Arc::new(
        octocode_runtime_base::security::JudgedText::judged_as(
            &raw,
            source,
            Some((&clean, &edits)),
        ),
    )))
}
