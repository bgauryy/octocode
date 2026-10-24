//! Text scans over the scope: files naming a symbol, ignored and lifted
//! files, and whether code uses or imports it.
use super::*;

impl Scope {
    /// Files under the scope that spell `symbol` as a word (canonical
    /// paths), scanned once per request and symbol. `Ok(None)` when the
    /// scan failed or hit a bound.
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
                let files = scan_text(
                    &self.root,
                    &self.include,
                    symbol,
                    policy,
                    cancel,
                    TEXT_SCAN_BUDGET,
                )
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

    /// Files under the scope that spell `symbol` as a word but that ignore
    /// rules keep out of every lexical scan (default-excluded directories
    /// such as `node_modules/` stay out), with the deciding rules of the
    /// first few; files in `skip` (already in the answer) are left out.
    /// The files come from the scope's ignored-file index (see
    /// `ignored_index`), built once per workspace snapshot; an index not
    /// ready within [`IGNORED_INDEX_WAIT`], incomplete, or read past
    /// [`IGNORED_READ_BUDGET`] says so in [`IgnoredMentions::unfinished`],
    /// with the files it reached.
    pub(in super::super) async fn ignored_text_files(
        &self,
        symbol: &str,
        skip: &std::collections::HashSet<String>,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<IgnoredMentions, LspFailure> {
        self.ignored_text_files_within(symbol, skip, policy, cancel, IGNORED_INDEX_WAIT)
            .await
    }

    pub(super) async fn ignored_text_files_within(
        &self,
        symbol: &str,
        skip: &std::collections::HashSet<String>,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
        wait: Duration,
    ) -> Result<IgnoredMentions, LspFailure> {
        let Some(visible) = self.text_files(symbol, policy, cancel).await? else {
            return Ok(IgnoredMentions::unfinished());
        };
        let Some(index) = self.ignored_index(policy, cancel, wait).await? else {
            return Ok(IgnoredMentions::unfinished());
        };
        let (word, read) = (symbol.to_owned(), Arc::clone(&index));
        let Some((mentions, read_all)) = blocking_cancellable(cancel, move |stopped| {
            read.mentions(&word, IGNORED_READ_BUDGET, stopped)
        })
        .await?
        else {
            return Ok(IgnoredMentions::unfinished());
        };
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
    /// not cover and that use the name in code: a file that spells it only
    /// in comments or strings holds no use the server could have missed (a
    /// file the grammar cannot parse counts its word matches). Past
    /// [`CODE_CHECK_MAX_FILES`] files, the rest count by their word matches.
    /// `None` when either side is unknown.
    pub(in super::super) fn text_only_files(&self, symbol: &str) -> Option<Vec<String>> {
        let unanswered = {
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
            files
                .iter()
                .filter(|file| !answered.contains(*file))
                .cloned()
                .collect::<Vec<_>>()
        };
        Some(
            unanswered
                .into_iter()
                .enumerate()
                .filter(|(index, file)| {
                    *index >= CODE_CHECK_MAX_FILES || self.uses_in_code(symbol, file)
                })
                .map(|(_, file)| file)
                .collect(),
        )
    }

    /// Whether `file` (canonical) uses `symbol` in code; memoized per
    /// request. An unreadable or unparsable file
    /// counts as a use.
    pub(super) fn uses_in_code(&self, symbol: &str, file: &str) -> bool {
        let key = format!("{symbol}\u{0}{file}");
        if let Some(known) = self
            .code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return *known;
        }
        let uses = crate::tools::source::read_bounded(Path::new(file), CODE_CHECK_MAX_BYTES)
            .ok()
            .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
            .is_none_or(|text| {
                octocode_engine::lsp::resolver::code_occurrence_lines(file, &text, symbol)
                    .is_none_or(|lines| !lines.is_empty())
            });
        self.code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, uses);
        uses
    }
}

/// Whether `file` (canonical, from the text scan) imports `symbol` by
/// name; an unreadable or unparsable file does not.
pub(in super::super) fn imports_in_code(symbol: &str, file: &str) -> bool {
    crate::tools::source::read_bounded(Path::new(file), CODE_CHECK_MAX_BYTES)
        .ok()
        .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        .and_then(|text| super::super::importers::imports_name(&text, file, symbol))
        .unwrap_or(false)
}
