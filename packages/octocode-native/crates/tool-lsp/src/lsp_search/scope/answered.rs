//! What the server's answer covers: the files, the lines it lists in them,
//! and the code sites it leaves out there.
use super::*;

impl Scope {
    /// The files the server's answer covers (canonical paths).
    pub(in super::super) fn answered_files(&self) -> std::collections::HashSet<String> {
        self.answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Whether the server's answer covers `file` (canonical path).
    pub(in super::super) fn is_answered(&self, file: &str) -> bool {
        self.answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(file)
    }

    /// Whether a file the server's answer covers under the scope root is
    /// gone: a file removed after the server read it leaves no stamp for a
    /// later walk to compare. Files outside the root (and non-file URIs)
    /// are not this scope's to prove.
    pub(in super::super) async fn answered_removed(
        &self,
        cancel: &dyn CancellationCheck,
    ) -> Result<bool, LspFailure> {
        let root = PathBuf::from(&self.root);
        let answered = self
            .answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|file| Path::new(file).starts_with(&root))
            .cloned()
            .collect::<Vec<_>>();
        if answered.is_empty() {
            return Ok(false);
        }
        let removed = blocking_cancellable(cancel, move |stopped| {
            answered.iter().take_while(|_| !stopped()).any(|file| {
                std::fs::symlink_metadata(file)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            })
        })
        .await?;
        // A check the request stopped proves nothing.
        Ok(removed.unwrap_or(true))
    }

    /// Record files (paths or `file://` uris) the server's answer covers.
    pub(in super::super) fn answer(&self, files: impl IntoIterator<Item = impl AsRef<str>>) {
        self.answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(files.into_iter().map(|file| canonical_uri(file.as_ref())));
    }

    /// Record the zero-based lines the answer lists, as `(file, line)` with
    /// the file a path or `file://` uri; `calls` says they are call sites
    /// (a `callers` answer), else uses of any kind.
    pub(in super::super) fn answer_lines(
        &self,
        calls: bool,
        sites: impl IntoIterator<Item = (impl AsRef<str>, u32)>,
    ) {
        let mut listed = self
            .listed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        listed.calls = calls;
        for (file, line) in sites {
            listed
                .files
                .entry(canonical_uri(file.as_ref()))
                .or_default()
                .insert(line);
        }
    }

    /// The code sites of `symbol` outside the answer inside the files it
    /// lists lines of: a server can miss a site in a file it answers, which
    /// no file count shows. Only a line the anchor's declaration reaches
    /// (`reach`) can be a missed site: a same-spelled word outside a local
    /// binding's scope names another symbol. A spelling is only a candidate
    /// ([`Gap::unverified`]): another symbol may be spelled the same, so a
    /// site counts as left out once the server says it names the anchor's
    /// declaration ([`Self::settle_gaps`]). Found once per request and
    /// symbol, on a cancellable worker; empty when the request stopped first.
    pub(in super::super) async fn check_answer_gaps(
        &self,
        symbol: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<Arc<Vec<Gap>>, LspFailure> {
        if let Some(known) = self.known_gaps(symbol) {
            return Ok(known);
        }
        let listed = self
            .listed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let name = symbol.to_owned();
        let (site, slot) = (self.site(), Arc::clone(&self.reach));
        let found = blocking_cancellable(cancel, move |stopped| {
            let reach = text::reach_in(&slot, site.as_ref(), &name, stopped);
            listed.gaps(&name, reach.as_deref(), stopped)
        })
        .await?
        .flatten();
        let Some(found) = found else {
            cancel.check().map_err(LspFailure::cancelled)?;
            return Ok(Arc::default());
        };
        Ok(self.keep_gaps(symbol, found))
    }

    /// [`Self::check_answer_gaps`] read here when it has not run.
    pub(in super::super) fn answer_gaps(&self, symbol: &str) -> Arc<Vec<Gap>> {
        if let Some(known) = self.known_gaps(symbol) {
            return known;
        }
        let listed = self
            .listed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let reach = text::reach_in(&self.reach, self.site().as_ref(), symbol, &|| false);
        let found = listed
            .gaps(symbol, reach.as_deref(), &|| false)
            .unwrap_or_default();
        self.keep_gaps(symbol, found)
    }

    /// Drop the gaps looked for so far: the answer has recorded more since.
    pub(in super::super) fn forget_gaps(&self) {
        *self
            .gaps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    fn known_gaps(&self, symbol: &str) -> Option<Arc<Vec<Gap>>> {
        self.gaps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|(known, _)| known == symbol)
            .map(|(_, gaps)| Arc::clone(gaps))
    }

    /// Replace the candidates of `symbol` with what the server's identity
    /// check settled of them: per file, the lines it proved name the anchor
    /// and those it could not place. A file with neither is no gap.
    pub(in super::super) fn settle_gaps(&self, symbol: &str, settled: Vec<Gap>) {
        let settled = settled
            .into_iter()
            .filter(|gap| !(gap.left_out.is_empty() && gap.unverified.is_empty()))
            .collect();
        self.keep_gaps(symbol, settled);
    }

    fn keep_gaps(&self, symbol: &str, found: Vec<Gap>) -> Arc<Vec<Gap>> {
        let found = Arc::new(found);
        *self
            .gaps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((symbol.to_owned(), Arc::clone(&found)));
        found
    }
}

/// What [`Listed::missed`] parsed of a file, by everything it depends on,
/// against the one process budget.
static MISSED: octocode_tools_shared::page_memo::PageMemo<Option<Vec<u32>>> =
    octocode_tools_shared::page_memo::PageMemo::new();

/// The lines an answer lists, by canonical file.
#[derive(Clone, Default)]
pub(in super::super) struct Listed {
    files: BTreeMap<String, BTreeSet<u32>>,
    /// The lines are call sites: only a call the answer leaves out is a gap.
    calls: bool,
}

/// Code sites of the symbol in one answered file that the answer does not
/// list: zero-based lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct Gap {
    pub(in super::super) file: String,
    /// Lines the server resolved to the anchor's declaration: uses its
    /// answer leaves out.
    pub(in super::super) left_out: Vec<u32>,
    /// Lines that spell the name in code and that no identity check placed:
    /// they may name another symbol, so they prove no missing use.
    pub(in super::super) unverified: Vec<u32>,
}

impl Listed {
    /// The lines listed of `file` (canonical).
    #[cfg(test)]
    pub(in super::super) fn lines_of(&self, file: &str) -> Option<&BTreeSet<u32>> {
        self.files.get(file)
    }

    /// Each listed file's gaps, by path, on lines `reach` admits (every
    /// line without one); `None` when `stopped` ended the look. A file is
    /// parsed only when it spells `symbol` on a line the answer leaves out;
    /// one that cannot be read or parsed claims none.
    fn gaps(
        &self,
        symbol: &str,
        reach: Option<&reach::Reach>,
        stopped: &dyn Fn() -> bool,
    ) -> Option<Vec<Gap>> {
        let mut found = Vec::new();
        for (file, covered) in &self.files {
            if stopped() {
                return None;
            }
            let Some(lines) = self.missed(symbol, file, covered) else {
                continue;
            };
            let lines = lines
                .into_iter()
                .filter(|line| reach.is_none_or(|reach| reach.reaches_line(file, *line)))
                .collect::<Vec<_>>();
            if !lines.is_empty() {
                found.push(Gap {
                    file: file.clone(),
                    left_out: Vec::new(),
                    unverified: lines,
                });
            }
        }
        Some(found)
    }

    fn missed(&self, symbol: &str, file: &str, covered: &BTreeSet<u32>) -> Option<Vec<u32>> {
        let bytes = octocode_tools_shared::source::read_bounded(
            Path::new(file),
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        )
        .ok()?;
        let text = super::super::source::admitted_text(&bytes).ok()?;
        let uncovered =
            |line: usize| u32::try_from(line).is_ok_and(|line| !covered.contains(&line));
        // Most answered files list every line that spells the name: they
        // are never parsed.
        let spelled = text.lines().enumerate().any(|(line, words)| {
            uncovered(line)
                && super::super::words::spells_word(words.as_bytes(), symbol.as_bytes(), true, true)
        });
        if !spelled {
            return None;
        }
        // What the parse found depends only on the text, the path, the name,
        // the lines listed and what they are sites of: a repeat or a later
        // page reads it.
        let listed = covered
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let key = format!(
            "{}\u{0}{symbol}\u{0}{}\u{0}{file}\u{0}{}",
            self.calls,
            octocode_engine::digest::sha256(text.as_bytes()),
            octocode_engine::digest::sha256(listed.as_bytes())
        );
        if let Some(kept) = MISSED.get(&key, "", Clone::clone) {
            return kept;
        }
        let missed = self.parse_missed(symbol, file, &text, &uncovered);
        let bytes = missed
            .as_ref()
            .map_or(0, |lines| lines.len() * std::mem::size_of::<u32>());
        MISSED.put(key, String::new(), bytes, missed.clone());
        missed
    }

    /// [`Self::missed`] from the file's parse.
    fn parse_missed(
        &self,
        symbol: &str,
        file: &str,
        text: &str,
        uncovered: &dyn Fn(usize) -> bool,
    ) -> Option<Vec<u32>> {
        let occurrences = octocode_engine::lsp::resolver::code_occurrences(file, text, symbol)?;
        let (declared, called) = question::declared_and_called(symbol, file, text, &occurrences)?;
        let lines = occurrences
            .lines
            .iter()
            .copied()
            .filter(|line| uncovered(*line))
            .filter(|line| {
                if self.calls {
                    called.contains(line)
                } else {
                    !declared.contains(line)
                }
            })
            .filter_map(|line| u32::try_from(line).ok())
            .collect::<Vec<_>>();
        (!lines.is_empty()).then_some(lines)
    }
}
