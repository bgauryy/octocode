//! Text scans over the scope: files naming a symbol, and whether code uses
//! or imports it.
use super::*;

impl Scope {
    /// Files under the scope that spell `symbol` as a word (canonical
    /// paths), scanned once per request and symbol. `Ok(None)` when the
    /// scan failed.
    pub(in super::super) async fn text_files(
        &self,
        symbol: &str,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Arc<BTreeSet<String>>>, LspFailure> {
        if let Some(scan) = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|scan| scan.symbol == symbol)
            .map(|scan| scan.files.clone())
        {
            return Ok(scan);
        }
        // One walk reads the family and the markup beside it.
        let include = [self.include.as_slice(), self.markup.as_slice()].concat();
        let scanned = scan_text(&self.root, &include, symbol, policy, cancel).await?;
        let (markup, files, listed) = match scanned {
            Some((found, listed)) => {
                let (markup, family) = found
                    .into_iter()
                    .partition(|file| !self.markup.is_empty() && paths::is_markup(file));
                (markup, Some(Arc::new(family)), Some(listed))
            }
            None => (BTreeSet::new(), None, None),
        };
        *self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(TextScan {
            symbol: symbol.to_owned(),
            files: files.clone(),
            markup: Arc::new(markup),
            listed,
        });
        Ok(files)
    }

    /// Files under the scope that spell `name` as a word (canonical paths),
    /// for another name than the request's symbol (a relay's alias of it):
    /// the same scan over the same listing as [`Self::text_files`], kept
    /// nowhere, so the one kept scan stays the symbol's. `Ok(None)` when the
    /// scan failed.
    pub(in super::super) async fn files_spelling(
        &self,
        name: &str,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<BTreeSet<String>>, LspFailure> {
        let include = [self.include.as_slice(), self.markup.as_slice()].concat();
        let scanned = scan_text(&self.root, &include, name, policy, cancel).await?;
        Ok(scanned.map(|(found, _)| {
            found
                .into_iter()
                .filter(|file| self.markup.is_empty() || !paths::is_markup(file))
                .collect()
        }))
    }

    /// Every file this request's text scan of `symbol` listed under the
    /// scope (canonical paths), whether or not it spells the name: what a
    /// module specifier can resolve to. `None` before that scan, or when it
    /// failed.
    pub(in super::super) fn listed_files(&self, symbol: &str) -> Option<Arc<[PathBuf]>> {
        self.text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|scan| scan.symbol == symbol)
            .and_then(|scan| scan.listed.clone())
    }

    /// The script-bearing markup files this request's text scan of `symbol`
    /// found ([`Self::text_files`]), which its answer does not cover.
    pub(in super::super) fn unanswered_markup(&self, symbol: &str) -> Vec<String> {
        let markup = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|scan| scan.symbol == symbol)
            .map(|scan| Arc::clone(&scan.markup));
        markup
            .iter()
            .flat_map(|files| files.iter())
            .filter(|file| !self.is_answered(file))
            .cloned()
            .collect()
    }

    /// The files this request's text scan of `symbol` found ([`Self::text_files`]);
    /// `None` before that scan, or when it failed.
    pub(in super::super) fn scanned_files(&self, symbol: &str) -> Option<Arc<BTreeSet<String>>> {
        self.text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|scan| scan.symbol == symbol)?
            .files
            .clone()
    }

    /// Files the text scan of `symbol` found that the server's answer does
    /// not cover, that could reference the anchor's declaration (`reach`)
    /// and that use the name in code: a file that spells it only in
    /// comments or strings holds no use the server could have missed (a
    /// file the grammar cannot parse counts its word matches). Every file is
    /// checked; [`Self::check_code_uses`] checks them on a cancellable worker
    /// first, so this reads its memo. `None` when either side is unknown.
    pub(in super::super) fn text_only_files(&self, symbol: &str) -> Option<Vec<String>> {
        let unanswered = self.unanswered(symbol)?;
        Some(
            unanswered
                .into_iter()
                .filter(|file| self.code_use(symbol, file) == Use::Code)
                .collect(),
        )
    }

    /// The scanned files the answer does not cover that lie outside the
    /// anchor declaration's reach, with that reach as a reader checks it;
    /// `None` when there are none or no bound is known.
    pub(in super::super) fn out_of_reach(&self, symbol: &str) -> Option<OutOfReach> {
        let files = self
            .unanswered(symbol)?
            .iter()
            .filter(|file| self.code_use(symbol, file) == Use::OutOfReach)
            .count();
        let reach = self.reach_of(symbol, &|| false)?;
        (files > 0).then(|| OutOfReach {
            files,
            bound: reach.describe(&self.root),
        })
    }

    /// The one path (canonical) the anchor's declaration of `symbol` is
    /// reached under, when its reach is bounded to one: where a text check
    /// of the name can stop. `None` when no bound is known.
    pub(in super::super) fn reach_directory(&self, symbol: &str) -> Option<String> {
        let directory = self.reach_of(symbol, &|| false)?.directory()?;
        Some(directory.to_string_lossy().into_owned())
    }

    /// Whether the anchor's declaration of `symbol` is a local binding (a
    /// parameter, a name bound inside a function or a block): no other file
    /// can name it, so no importer or other file holds a use of it.
    pub(in super::super) fn confined(&self, symbol: &str) -> bool {
        self.reach_of(symbol, &|| false)
            .is_some_and(|reach| matches!(*reach, reach::Reach::Local { .. }))
    }

    /// Whether `file` (canonical) lies in the reach of the anchor's
    /// declaration of `symbol`; every file does when no bound is known.
    pub(in super::super) fn may_reach(&self, symbol: &str, file: &str) -> bool {
        self.reach_of(symbol, &|| false)
            .is_none_or(|reach| reach.reaches(file, &|| false))
    }

    /// Whether scanned `file` (canonical) could reference the anchor's
    /// declaration of `symbol`; a file not checked yet is.
    pub(in super::super) fn in_reach(&self, symbol: &str, file: &str) -> bool {
        self.code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&code_use_key(symbol, file))
            != Some(&Use::OutOfReach)
    }

    /// Check every scanned file the answer does not cover for its reach and
    /// a code use of `symbol` on a cancellable worker, memoizing each for
    /// [`Self::text_only_files`]. The answer only grows, so a file it
    /// leaves out later was checked here.
    pub(in super::super) async fn check_code_uses(
        &self,
        symbol: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), LspFailure> {
        let Some(unanswered) = self.unanswered(symbol) else {
            return Ok(());
        };
        self.classify_files(symbol, unanswered, cancel).await
    }

    /// [`Self::check_code_uses`] over `files`, whatever walk found them.
    async fn classify_files(
        &self,
        symbol: &str,
        unanswered: Vec<String>,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), LspFailure> {
        let (memo, symbol) = (Arc::clone(&self.code_uses), symbol.to_owned());
        let (site, slot) = (self.site(), Arc::clone(&self.reach));
        let question = self.question.get().copied().unwrap_or_default();
        blocking_cancellable(cancel, move |stopped| {
            let reach = reach_in(&slot, site.as_ref(), &symbol, stopped);
            for file in unanswered {
                if stopped() {
                    return;
                }
                let key = code_use_key(&symbol, &file);
                if memo
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains_key(&key)
                {
                    continue;
                }
                let found = classify(&symbol, &file, reach.as_deref(), question, stopped);
                memo.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(key, found);
            }
        })
        .await?;
        Ok(())
    }

    pub(super) fn site(&self) -> Option<reach::Site> {
        self.site
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The reach of the anchor's declaration of `symbol`, read once.
    fn reach_of(&self, symbol: &str, stopped: &dyn Fn() -> bool) -> Option<Arc<reach::Reach>> {
        reach_in(&self.reach, self.site().as_ref(), symbol, stopped)
    }

    /// The scanned files of `symbol` the answer does not cover; `None`
    /// before the scan or the answer.
    fn unanswered(&self, symbol: &str) -> Option<Vec<String>> {
        let text = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let files = text
            .as_ref()
            .filter(|scan| scan.symbol == symbol)?
            .files
            .clone()?;
        let answered = self
            .answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if answered.is_empty() {
            return None;
        }
        Some(
            files
                .iter()
                .filter(|file| !answered.contains(*file))
                .cloned()
                .collect(),
        )
    }

    /// How `file` (canonical) uses `symbol`; memoized per request.
    fn code_use(&self, symbol: &str, file: &str) -> Use {
        let key = code_use_key(symbol, file);
        if let Some(known) = self
            .code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return *known;
        }
        let found = classify(
            symbol,
            file,
            self.reach_of(symbol, &|| false).as_deref(),
            self.question.get().copied().unwrap_or_default(),
            &|| false,
        );
        self.code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, found);
        found
    }
}

/// How a scanned file uses a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Use {
    /// In code: a site the server may have missed.
    Code,
    /// Only in comments or strings, or in code that cannot answer the
    /// question ([`Question`]).
    Mention,
    /// The file cannot reference the anchor's declaration at all.
    OutOfReach,
}

/// Scanned files outside the anchor declaration's reach: no missed site,
/// no lead, and so no partial reason.
#[derive(Debug)]
pub(in super::super) struct OutOfReach {
    pub(in super::super) files: usize,
    /// The reach as a reader checks it (`pub(super) reaches only module …`).
    pub(in super::super) bound: String,
}

fn classify(
    symbol: &str,
    file: &str,
    reach: Option<&reach::Reach>,
    question: Question,
    stopped: &dyn Fn() -> bool,
) -> Use {
    if reach.is_some_and(|reach| !reach.reaches(file, stopped)) {
        Use::OutOfReach
    } else if uses_in_code(symbol, file, question) {
        Use::Code
    } else {
        Use::Mention
    }
}

/// The reach `slot` holds for `symbol`, read from `site` on first use.
pub(super) fn reach_in(
    slot: &ReachSlot,
    site: Option<&reach::Site>,
    symbol: &str,
    stopped: &dyn Fn() -> bool,
) -> Option<Arc<reach::Reach>> {
    let mut held = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((known, reach)) = held.as_ref()
        && known == symbol
    {
        return reach.clone();
    }
    let site = site?;
    if stopped() {
        return None;
    }
    let reach = reach::Reach::of(site, symbol).map(Arc::new);
    *held = Some((symbol.to_owned(), reach.clone()));
    reach
}

fn code_use_key(symbol: &str, file: &str) -> String {
    format!("{symbol}\u{0}{file}")
}

/// Whether `file` (canonical) uses `symbol` in code, not only in comments
/// or strings, in a way that may answer `question`. An unreadable or
/// unparsable file counts as a use.
fn uses_in_code(symbol: &str, file: &str, question: Question) -> bool {
    code_use_memo::kept_or_read(symbol, file, question, || {
        let bytes = octocode_tools_shared::source::read_bounded(
            Path::new(file),
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        )
        .ok()?;
        let Ok(text) = super::super::source::admitted_text(&bytes) else {
            return Some(true);
        };
        Some(
            octocode_engine::lsp::resolver::code_occurrences(file, &text, symbol).is_none_or(
                |occurrences| {
                    !occurrences.lines.is_empty()
                        && question.may_answer(symbol, file, &text, &occurrences)
                },
            ),
        )
    })
    .unwrap_or(true)
}

/// Whether `file` (canonical, from the text scan) imports `symbol` by
/// name (the declarations parse holds its imports); an
/// unreadable or unparsable file does not.
pub(in super::super) fn imports_in_code(symbol: &str, file: &str) -> bool {
    octocode_tools_shared::source::read_bounded(
        Path::new(file),
        octocode_runtime_base::security::SourceLimits::REAL.whole_source,
    )
    .ok()
    .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
    .and_then(|text| {
        octocode_tools_shared::declarations::file_document(&text, file)
            .map(|facts| super::super::importers::imports_name(&facts, &text, symbol))
    })
    .unwrap_or(false)
}
