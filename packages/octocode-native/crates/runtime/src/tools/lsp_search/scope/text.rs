//! Text scans over the scope: files naming a symbol, ignored and lifted
//! files, and whether code uses or imports it.
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
        let stored = self.fingerprint().map(|fingerprint| {
            octocode_engine::digest::sha256(
                format!(
                    "{}\u{0}{}\u{0}{symbol}\u{0}{fingerprint}\u{0}{policy:?}",
                    self.root,
                    self.include.join("\u{0}")
                )
                .as_bytes(),
            )
        });
        let hit = stored
            .as_ref()
            .and_then(|key| TEXT_SCANS.get(key))
            .map(|hit| hit.value);
        let files = match hit {
            Some(files) => Some(files),
            None => {
                let files = scan_text(&self.root, &self.include, symbol, policy, cancel)
                    .await?
                    .map(Arc::new);
                if let (Some(key), Some(files)) = (stored, &files) {
                    TEXT_SCANS.insert(key, Arc::clone(files), 1, None);
                }
                files
            }
        };
        if let Ok(mut slot) = self.text.lock() {
            *slot = Some(TextScan {
                symbol: symbol.to_owned(),
                files: files.clone(),
            });
        }
        Ok(files)
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

    /// Files under the scope that spell `symbol` as a word but that ignore
    /// rules keep out of every lexical scan (default-excluded directories
    /// such as `node_modules/` stay out), with the deciding rules of the
    /// first few; files in `skip` (already in the answer) are left out.
    /// The files come from the scope's ignored-file index (see
    /// `ignored_index`), built once per workspace snapshot. The request
    /// waits for an index a walk is building and reads every listed file,
    /// however long that takes: only its own cancellation or deadline ends
    /// either early (an error, never a shorter answer). An index the walk
    /// left incomplete says so in [`IgnoredMentions::unfinished`], with the
    /// files it reached.
    pub(in super::super) async fn ignored_text_files(
        &self,
        symbol: &str,
        skip: &std::collections::HashSet<String>,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<IgnoredMentions, LspFailure> {
        let Some(visible) = self.text_files(symbol, policy, cancel).await? else {
            return Ok(IgnoredMentions::unfinished());
        };
        let Some(index) = self.ignored_index(policy, cancel).await? else {
            return Ok(IgnoredMentions::unfinished());
        };
        let (word, read) = (symbol.to_owned(), Arc::clone(&index));
        let Some((mentions, read_all)) =
            blocking_cancellable(cancel, move |stopped| read.mentions(&word, stopped)).await?
        else {
            return Ok(IgnoredMentions::unfinished());
        };
        // A read the request stopped is the request's end, not an answer.
        if !read_all {
            cancel.check().map_err(LspFailure::cancelled)?;
        }
        let files = mentions
            .into_iter()
            .filter(|file| !visible.contains(file) && !skip.contains(file))
            .collect::<Vec<_>>();
        let root = Path::new(&self.root);
        let mut rules = Vec::new();
        for file in files.iter().take(crate::tools::empty_walk::SAMPLE_FILES) {
            if let Some(rule) = crate::tools::empty_walk::ignore_rule(root, Path::new(file), false)
                && !rules.contains(&rule)
            {
                rules.push(rule);
            }
        }
        Ok(IgnoredMentions {
            files,
            rules,
            unfinished: !index.complete || !read_all,
            nested_repositories: index.nested.clone(),
            ..IgnoredMentions::default()
        })
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
        let (memo, symbol) = (Arc::clone(&self.code_uses), symbol.to_owned());
        let (site, slot) = (self.site(), Arc::clone(&self.reach));
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
                let found = classify(&symbol, &file, reach.as_deref(), stopped);
                memo.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(key, found);
            }
        })
        .await?;
        Ok(())
    }

    fn site(&self) -> Option<reach::Site> {
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
    /// Only in comments or strings.
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
    stopped: &dyn Fn() -> bool,
) -> Use {
    if reach.is_some_and(|reach| !reach.reaches(file, stopped)) {
        Use::OutOfReach
    } else if uses_in_code(symbol, file) {
        Use::Code
    } else {
        Use::Mention
    }
}

/// The reach `slot` holds for `symbol`, read from `site` on first use.
fn reach_in(
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
/// or strings. An unreadable or unparsable file counts as a use.
fn uses_in_code(symbol: &str, file: &str) -> bool {
    crate::tools::source::read_bounded(Path::new(file), octocode_engine::signatures::MAX_PARSE_SIZE)
        .ok()
        .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        .is_none_or(|text| {
            octocode_engine::lsp::resolver::code_occurrence_lines(file, &text, symbol)
                .is_none_or(|lines| !lines.is_empty())
        })
}

/// Whether `file` (canonical, from the text scan) imports `symbol` by
/// name (the digest-cached declarations parse holds its imports); an
/// unreadable or unparsable file does not.
pub(in super::super) fn imports_in_code(symbol: &str, file: &str) -> bool {
    crate::tools::source::read_bounded(Path::new(file), octocode_engine::signatures::MAX_PARSE_SIZE)
        .ok()
        .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        .and_then(|text| {
            crate::tools::declarations_cache::file_document(&text, file)
                .map(|facts| super::super::importers::imports_name(&facts, &text, symbol))
        })
        .unwrap_or(false)
}
