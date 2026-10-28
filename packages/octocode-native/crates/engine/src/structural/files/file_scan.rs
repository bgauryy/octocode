//! The per-file scan: gate, read and match one candidate file.

use std::path::Path;

use super::anchors::{AnchorFilter, Anchored};
use super::{CompiledMatchers, unsupported_language};
use crate::error::{Error, Result};
use crate::search::walk::Stop;
use crate::signatures::languages;
use crate::structural::octo::Found;
use crate::structural::types::{
    FileMatches, StructuralDemand, StructuralDiagnostic, StructuralSearchDetailedFileResult,
};
use crate::text::Refusal;
use crate::text::analysis::{AdmittedSource, MapBudget, MapLimits};

/// How one candidate file counts toward the scan totals.
pub(super) enum Tally {
    None,
    PreFilter,
    Unsupported,
    Unreadable,
    Large,
    CompileFailure,
    Parsed(u32),
}

pub(super) type FileOutcome = Option<(Tally, StructuralSearchDetailedFileResult)>;

/// Shared, read-only state of the parallel per-file scan.
pub(super) struct FileScan<'a> {
    pub(super) admit: &'a (dyn Fn(&Path) -> bool + Sync),
    pub(super) stop: &'a Stop<'a>,
    pub(super) select_extension: &'a (dyn Fn(&Path) -> String + Sync),
    pub(super) anchors: Option<AnchorFilter>,
    pub(super) matchers: CompiledMatchers,
    pub(super) max_file_bytes: u64,
    pub(super) demand: StructuralDemand<'a>,
}

impl FileScan<'_> {
    /// Gate, read, and match one candidate. `None` when path policy denies it.
    pub(super) fn file(&self, file_path: &Path) -> Result<FileOutcome> {
        if (self.stop)() {
            return Err(Error::cancelled());
        }
        if !(self.admit)(file_path) {
            return Ok(None);
        }
        let path_string = file_path.to_string_lossy().to_string();
        // One read serves both the anchor check and the parse.
        let prefiltered = self
            .anchors
            .as_ref()
            .map(|anchors| anchors.check(file_path, self.max_file_bytes));
        if let Some(Anchored::Miss(source_sha256)) = &prefiltered {
            let mut file = skipped_file(
                path_string,
                "skippedByPreFilter",
                "preFilter",
                StructuralDiagnostic::new(
                    "structural.prefilter.skipped",
                    "info",
                    "scan",
                    "File excluded by the text pre-filter (it does not contain the pattern's anchor literal), so AST parsing was skipped.",
                )
                .with_recovery("Remove the literal prefilter by using a rule with no safe anchor if every file must be parsed."),
            );
            file.source_sha256 = source_sha256.clone();
            return Ok(Some((Tally::PreFilter, file)));
        }

        let ext = (self.select_extension)(file_path);
        let Some(entry) = languages::find_entry(&ext) else {
            return Ok(Some((
                Tally::Unsupported,
                skipped_file(
                    path_string.clone(),
                    "unsupported",
                    "unsupportedExtension",
                    unsupported_language(&path_string),
                ),
            )));
        };

        let bytes = match prefiltered {
            None | Some(Anchored::Miss(_)) => read_source(file_path, self.max_file_bytes),
            Some(Anchored::Hit(bytes)) => Ok(bytes),
            Some(Anchored::Large(len)) => Err(SourceSkip::Large(len)),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(skip) => {
                return Ok(Some(source_skipped(
                    path_string,
                    &skip,
                    self.max_file_bytes,
                )));
            }
        };
        let mut budget = MapBudget::new(
            MapLimits::default(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            self.stop,
        );
        let source = match AdmittedSource::from_bytes(&bytes, &path_string, &mut budget) {
            Ok(source) => source,
            Err(crate::text::analysis::SourceError::Refused(refusal)) => {
                return Ok(Some(source_skipped(
                    path_string,
                    &SourceSkip::Refused(refusal),
                    self.max_file_bytes,
                )));
            }
            Err(error) => return Err(error.into()),
        };
        let mut source_error = None;
        let (tally, mut file) = self.match_source(
            path_string,
            &ext,
            entry.language_id,
            &source,
            &mut source_error,
            budget,
        );
        if let Some(error) = source_error {
            return Err(error.into());
        }
        file.source_sha256 = Some(crate::digest::sha256(&bytes));
        Ok(Some((tally, file)))
    }

    fn match_source(
        &self,
        path_string: String,
        ext: &str,
        language_id: Option<&str>,
        admitted: &AdmittedSource<'_>,
        source_error: &mut Option<crate::text::analysis::SourceError>,
        budget: MapBudget<'_>,
    ) -> (Tally, StructuralSearchDetailedFileResult) {
        let content = admitted.text();
        let Some(compiled) = self.matchers.get(ext) else {
            return (
                Tally::CompileFailure,
                skipped_file(
                    path_string.clone(),
                    "parserFailed",
                    "queryCompile",
                    StructuralDiagnostic::new(
                        "structural.matcher.missing",
                        "error",
                        "match",
                        "Structural matcher was unavailable after compilation.",
                    )
                    .with_path(path_string)
                    .with_recovery(
                        "Retry the search; this indicates an internal matcher lifecycle issue.",
                    ),
                ),
            );
        };
        let run = match compiled {
            Ok(run) => run,
            Err(error) => {
                if let Some(interruption) = error.interruption() {
                    let diagnostic = interruption.diagnostic(&path_string);
                    return (
                        Tally::None,
                        skipped_file(path_string, "truncated", "queryCompile", diagnostic),
                    );
                }
                return (
                    Tally::CompileFailure,
                    skipped_file(
                        path_string.clone(),
                        "parserFailed",
                        "queryCompile",
                        error.diagnostic().with_path(path_string),
                    ),
                );
            }
        };

        let mut found = Found::new(self.demand);
        found.bind_source(crate::structural::SourceBinder::with_budget(
            admitted, budget,
        ));
        if let Err(error) = run(content, &mut found) {
            let diagnostic = error.diagnostic(&path_string);
            return (
                Tally::None,
                skipped_file(path_string, "truncated", "executionLimit", diagnostic),
            );
        }
        if let Some(error) = found.source_error() {
            *source_error = Some(error.clone());
            return (
                Tally::None,
                skipped_file(
                    path_string,
                    "truncated",
                    "sourceLimit",
                    StructuralDiagnostic::new(
                        "structural.source.failed",
                        "error",
                        "source",
                        error.to_string(),
                    ),
                ),
            );
        }
        let recovered = found.recovered();
        let matches = found.into_file_matches();
        (
            Tally::Parsed(matches.len() as u32),
            StructuralSearchDetailedFileResult {
                path: path_string,
                source_sha256: None,
                status: "ok".to_owned(),
                language_id: language_id.map(str::to_owned),
                skipped_reason: None,
                matches,
                recovered,
                diagnostics: Vec::new(),
            },
        )
    }
}

/// The skipped-file row and tally for a source that failed its read gates.
fn source_skipped(
    path_string: String,
    skip: &SourceSkip,
    max_file_bytes: u64,
) -> (Tally, StructuralSearchDetailedFileResult) {
    let (tally, status, reason, diagnostic) = match skip {
        SourceSkip::Metadata(err) => (
            Tally::Unreadable,
            "unreadable",
            "metadata",
            StructuralDiagnostic::new(
                "structural.file.unreadable",
                "warning",
                "scan",
                format!("Could not read file metadata: {err}."),
            )
            .with_recovery("Retry if the file still exists and permissions allow reading it."),
        ),
        SourceSkip::Large(len) => (
            Tally::Large,
            "truncated",
            "maxFileBytes",
            StructuralDiagnostic::new(
                "structural.file.tooLarge",
                "warning",
                "scan",
                format!(
                    "File is {len} bytes, above the structural search limit of {max_file_bytes} bytes."
                ),
            ),
        ),
        SourceSkip::Unreadable(err) => (
            Tally::Unreadable,
            "unreadable",
            "read",
            StructuralDiagnostic::new(
                "structural.file.unreadable",
                "warning",
                "scan",
                format!("Could not read file content: {err}."),
            ),
        ),
        SourceSkip::Refused(refusal) => (
            Tally::Unreadable,
            "unreadable",
            "read",
            StructuralDiagnostic::new(
                "structural.file.unreadable",
                "warning",
                "scan",
                format!("Could not read file content as text: {refusal}."),
            )
            .with_recovery("Use binary inspection or text search for non-UTF-8 content."),
        ),
    };
    let diagnostic = diagnostic.with_path(path_string.clone());
    (tally, skipped_file(path_string, status, reason, diagnostic))
}

/// Why a candidate's source was not read.
pub(super) enum SourceSkip {
    Metadata(std::io::Error),
    Large(u64),
    Unreadable(std::io::Error),
    /// The bytes are not admitted as text ([`admit`]).
    Refused(Refusal),
}

/// A candidate's bytes through the one bounded reader: an oversized file is
/// refused from its handle's size, never read whole. [`admit`] turns them
/// into the text a parse sees.
pub(super) fn read_source(
    path: &Path,
    max_file_bytes: u64,
) -> std::result::Result<Vec<u8>, SourceSkip> {
    match crate::text::read_source(path, max_file_bytes) {
        Ok(source) => Ok(source.bytes),
        Err(crate::text::ReadError::TooLarge(len)) => Err(SourceSkip::Large(len)),
        Err(crate::text::ReadError::Io(err)) => Err(SourceSkip::Unreadable(err)),
        Err(crate::text::ReadError::NotRegular) => Err(SourceSkip::Metadata(
            std::io::Error::other("not a regular file"),
        )),
    }
}

fn skipped_file(
    path: String,
    status: &str,
    skipped_reason: &str,
    diagnostic: StructuralDiagnostic,
) -> StructuralSearchDetailedFileResult {
    StructuralSearchDetailedFileResult {
        path,
        source_sha256: None,
        status: status.to_owned(),
        language_id: None,
        skipped_reason: Some(skipped_reason.to_owned()),
        matches: FileMatches::Held(Vec::new()),
        recovered: false,
        diagnostics: vec![diagnostic],
    }
}
