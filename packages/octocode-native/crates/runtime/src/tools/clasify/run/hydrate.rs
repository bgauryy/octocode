//! Search candidates and their hydration: splitting a search page into file
//! candidates, planning hit-cluster windows, and reading each window as one
//! bounded page.
use super::evidence::{candidate_state, fallback_context, provider_state};
use super::*;
use crate::tools::clasify::resource::{ResourceSource, tool_of};

pub(super) fn is_candidate_search(source: &Value) -> bool {
    tool_of(source).is_some_and(clasify::is_candidate_search_tool)
}

pub(super) fn file_chunks(source: &Value) -> bool {
    clasify::candidate_evidence(source) == Some(clasify::CandidateEvidence::FileChunks)
}

pub(super) fn candidate_identity(source: &Value, file: &Value) -> Option<String> {
    let path = file.get("path")?.as_str()?;
    match tool_of(source)? {
        // Rows are workspace-relative (root is the workspace root) or
        // absolute outside it, so either form reads back through localFetch.
        ToolId::LocalSearch => Some(path.to_owned()),
        ToolId::GhSearchCode => Some(format!(
            "{}/{}/{}",
            file.get("owner")?.as_str()?.to_ascii_lowercase(),
            file.get("repo")?.as_str()?.to_ascii_lowercase(),
            path
        )),
        _ => None,
    }
}

/// Turn one lexical search page into independent, path-deduplicated files.
#[cfg(test)]
pub(super) fn search_candidate_states(source: &Value, state: &Value) -> Option<Vec<Value>> {
    positioned_search_candidates(source, state)
        .map(|candidates| candidates.into_iter().map(|(_, state)| state).collect())
}

/// Search candidates with the position of their file row on the page.
pub(super) fn positioned_search_candidates(
    source: &Value,
    state: &Value,
) -> Option<Vec<(usize, Value)>> {
    if !is_candidate_search(source) {
        return None;
    }
    let files = state.pointer("/results/0/data/files")?.as_array()?;
    let code_search = tool_of(source) == Some(ToolId::GhSearchCode);
    // A repo-scoped ghSearchCode page names owner/repo once, on `data`, or
    // (minimized as a request echo) only in the search query itself.
    let read = code_search.then(|| ResourceSource::of(source)).flatten();
    let (query_owner, query_repo) = read
        .as_ref()
        .map_or((None, None), ResourceSource::code_search_repository);
    let field = |name: &str, queried: Option<&str>| {
        state
            .pointer(&format!("/results/0/data/{name}"))
            .cloned()
            .or_else(|| queried.map(|value| json!(value)))
    };
    let page_repo = Some((field("owner", query_owner), field("repo", query_repo)));
    let mut seen = HashSet::new();
    let candidates = files
        .iter()
        .enumerate()
        .filter_map(|(position, file)| {
            let file = if let Some(row) = file.as_str() {
                if code_search {
                    let (repo, path) = row.split_once(':')?;
                    let (owner, repo) = repo.split_once('/')?;
                    json!({"owner":owner, "repo":repo, "path":path})
                } else {
                    json!({"path":row})
                }
            } else {
                let mut file = file.clone();
                if code_search
                    && let (Some(object), Some((Some(owner), Some(repo)))) =
                        (file.as_object_mut(), page_repo.clone())
                {
                    object.entry("owner").or_insert(owner);
                    object.entry("repo").or_insert(repo);
                }
                file
            };
            candidate_identity(source, &file)
                .is_some_and(|id| seen.insert(id))
                .then_some((position, file))
        })
        .map(|(position, file)| {
            let mut candidate = state.clone();
            candidate["results"][0]["data"]["files"] = json!([file]);
            (position, candidate)
        })
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

/// Every hit line of one local candidate: its shown rows plus the rows a
/// clipped file names in `pagination.moreLines`.
pub(super) fn candidate_hit_lines(file: &Value) -> Vec<u64> {
    let shown = file
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|matched| matched.get("line").and_then(Value::as_u64));
    // `moreLines` names runs (`711-717,802`) and may end with a count of
    // omitted lines (`+40 more`), which names no line.
    let more = file
        .pointer("/pagination/moreLines")
        .and_then(Value::as_str)
        .into_iter()
        .flat_map(crate::tools::line_spans::more_lines_named);
    let mut lines = shown.chain(more).collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Contiguous windows (`2 × HYDRATED_LINE_RADIUS + 1` lines) covering every
/// hit cluster, densest first: each centers on the densest remaining run of
/// hits (an incidental first hit must not pull a window off its cluster), and
/// the hits it covers leave the pool. No hits yields the opening window.
pub(super) fn hit_cluster_windows(mut lines: Vec<u64>) -> Vec<(u64, u64)> {
    let mut windows = Vec::new();
    while let Some((first, last)) = densest_run(&lines, HYDRATED_LINE_RADIUS * 2) {
        let center = first + (last - first) / 2;
        let start = center.saturating_sub(HYDRATED_LINE_RADIUS).max(1);
        let end = center.saturating_add(HYDRATED_LINE_RADIUS);
        windows.push((start, end));
        lines.retain(|line| *line < start || *line > end);
    }
    if windows.is_empty() {
        windows.push((1, HYDRATED_LINE_RADIUS * 2 + 1));
    }
    windows
}

/// Windows judged per candidate within `budget` pages: every candidate gets
/// its densest cluster first, then further clusters go round-robin, so a
/// deciding line far from the densest cluster is still judged.
pub(super) fn allocate_windows(clusters: &[usize], budget: usize) -> Vec<usize> {
    let mut taken = clusters
        .iter()
        .map(|count| (*count).min(1))
        .collect::<Vec<_>>();
    let mut left = budget.saturating_sub(taken.iter().sum());
    let mut round = 1;
    while left > 0 && clusters.iter().any(|count| *count > round) {
        for (index, count) in clusters.iter().enumerate() {
            if left > 0 && *count > round {
                taken[index] += 1;
                left -= 1;
            }
        }
        round += 1;
    }
    taken
}

/// Longest span of merged windows: three windows' lines.
pub(super) const MAX_MERGED_WINDOW_LINES: u64 = 3 * (HYDRATED_LINE_RADIUS * 2 + 1);

/// Sorted windows of one file, joined when they overlap or the gap between
/// them is at most one window radius and the joined span stays within
/// [`MAX_MERGED_WINDOW_LINES`]: one contiguous page judges both clusters in
/// one provider call instead of two (and never judges overlap lines twice).
pub(super) fn merge_near_windows(windows: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(windows.len());
    for (start, end) in windows {
        match merged.last_mut() {
            Some(last)
                if start <= last.1 + 1 + HYDRATED_LINE_RADIUS
                    && end.max(last.1) - last.0 < MAX_MERGED_WINDOW_LINES =>
            {
                last.1 = last.1.max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Lines a hydrated span may grow at each edge to take in a declaration it
/// cuts.
pub(super) const DECLARATION_SNAP_LINES: u64 = HYDRATED_LINE_RADIUS;

/// `span` grown so neither edge cuts a declaration: an edge inside a
/// declaration that otherwise lies within the span moves out to that
/// declaration's boundary, when the boundary is at most
/// [`DECLARATION_SNAP_LINES`] away. A declaration that also crosses the other
/// edge (an enclosing impl, class, or module) is not completed by moving one
/// edge, so it leaves the span alone. `spans` are 1-based inclusive.
pub(super) fn snap_to_declarations(
    (start, end): (u64, u64),
    spans: &[(usize, usize)],
) -> (u64, u64) {
    let spans = spans
        .iter()
        .map(|&(first, last)| (first as u64, last as u64));
    let snapped_start = spans
        .clone()
        .filter(|&(first, last)| {
            first < start && start <= last && last <= end && start - first <= DECLARATION_SNAP_LINES
        })
        .map(|(first, _)| first)
        .min()
        .unwrap_or(start);
    let snapped_end = spans
        .filter(|&(first, last)| {
            first <= end
                && end < last
                && first >= snapped_start
                && last - end <= DECLARATION_SNAP_LINES
        })
        .map(|(_, last)| last)
        .max()
        .unwrap_or(end);
    (snapped_start, snapped_end)
}

/// Declaration spans of each local candidate file, for snapping its hydrated
/// windows. Read through the path policy; a file it refuses, a file too
/// large to outline, or a language without an outline has none.
pub(super) fn local_declaration_spans(
    paths: &PathPolicy,
    security: &crate::security::ContentSecurity,
    candidates: &[Value],
) -> HashMap<String, Vec<(usize, usize)>> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let file = candidate.pointer("/results/0/data/files/0")?;
            let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
            let validated = paths.validate_read(&path).ok()?;
            let text = crate::tools::source::read_text(
                &validated.canonical,
                octocode_engine::signatures::MAX_PARSE_SIZE,
                security,
            )?;
            let spans = crate::tools::local_fetch::declaration_spans(&text, &path)?;
            Some((path, spans))
        })
        .collect()
}

pub(super) fn local_window_read(path: &str, (start, end): (u64, u64), max_bytes: usize) -> Value {
    json!({
        "tool":ToolId::LocalFetch.as_str(),
        "query":{
            "reasoning":"Read a bounded search candidate for classification.",
            "path":path,
            "ranges":[format!("{start}-{end}")],
            "unit":"bytes",
            "length":max_bytes,
            "minify":"none"
        }
    })
}

/// One bounded read per hit cluster of a local candidate, densest first.
pub(super) fn local_candidate_reads(candidate: &Value, max_bytes: usize) -> Option<Vec<Value>> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
    Some(
        hit_cluster_windows(candidate_hit_lines(file))
            .into_iter()
            .map(|window| local_window_read(&path, window, max_bytes))
            .collect(),
    )
}

pub(super) fn local_candidate_read(candidate: &Value, max_bytes: usize) -> Option<Value> {
    local_candidate_reads(candidate, max_bytes)?
        .into_iter()
        .next()
}

/// Center of the densest run of match lines that fits one hydrated window.
pub(in crate::tools::clasify) fn densest_match_line(mut lines: Vec<u64>) -> Option<u64> {
    lines.sort_unstable();
    let (first, last) = densest_run(&lines, HYDRATED_LINE_RADIUS * 2)?;
    Some(first + (last - first) / 2)
}

/// First and last line of the longest run of sorted `lines` spanning at most
/// `width` lines.
pub(super) fn densest_run(lines: &[u64], width: u64) -> Option<(u64, u64)> {
    let mut best = (0, 0);
    let mut first = 0;
    for last in 0..lines.len() {
        while lines[last] - lines[first] > width {
            first += 1;
        }
        if last - first > best.1 - best.0 {
            best = (first, last);
        }
    }
    Some((*lines.get(best.0)?, *lines.get(best.1)?))
}

/// Longest matched line used verbatim as a read anchor; longer (minified)
/// lines fall back to the matched term.
pub(super) const MAX_ANCHOR_LINE_CHARS: usize = 160;

/// Literal anchor for one GitHub snippet: the line holding the most matched
/// terms, trimmed. A bare keyword recurs across the file, so anchoring on it
/// reads every occurrence instead of the hit.
pub(super) fn github_match_anchor(matched: &Value) -> Option<String> {
    let value = matched.get("value")?.as_str()?;
    let ranges = matched.get("matchIndices")?.as_array()?;
    let line_of = |range: &Value| -> Option<usize> {
        if let Some(offset) = range.get("lineOffset").and_then(Value::as_u64) {
            return usize::try_from(offset).ok();
        }
        let start = usize::try_from(range.get("start")?.as_u64()?).ok()?;
        Some(
            crate::content::utf16_slice(value, 0, start)?
                .matches('\n')
                .count(),
        )
    };
    let mut counts = Vec::<(usize, usize)>::new();
    for line in ranges.iter().filter_map(line_of) {
        match counts.iter_mut().find(|(seen, _)| *seen == line) {
            Some((_, count)) => *count += 1,
            None => counts.push((line, 1)),
        }
    }
    let densest = counts
        .iter()
        .fold(None::<(usize, usize)>, |best, &(line, count)| match best {
            Some((_, top)) if top >= count => best,
            _ => Some((line, count)),
        })
        .map(|(line, _)| line);
    let line = densest
        .and_then(|line| value.split('\n').nth(line))
        .map(str::trim)
        .filter(|line| !line.is_empty() && line.chars().count() <= MAX_ANCHOR_LINE_CHARS);
    if let Some(line) = line {
        return Some(line.to_owned());
    }
    let range = ranges.first()?;
    let start = usize::try_from(range.get("start")?.as_u64()?).ok()?;
    let end = usize::try_from(range.get("end")?.as_u64()?).ok()?;
    crate::content::utf16_slice(value, start, end).filter(|value| !value.trim().is_empty())
}

pub(super) fn github_candidate_read(candidate: &Value, max_bytes: usize) -> Option<(Value, bool)> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let mut query = json!({
        "reasoning":"Read a bounded GitHub search candidate for classification.",
        "owner":file.get("owner")?,
        "repo":file.get("repo")?,
        "path":file.get("path")?,
        "unit":"bytes",
        "length":max_bytes,
        "minify":"none"
    });
    // A line-resolved row (`"N\ttext"`) anchors on its first hit line.
    let line_anchor = file
        .get("lines")
        .and_then(Value::as_array)
        .and_then(|lines| lines.first())
        .and_then(Value::as_str)
        .and_then(|line| line.split_once(crate::tools::numbered::SEPARATOR))
        .map(|(_, text)| text.trim())
        .filter(|text| !text.is_empty() && text.chars().count() <= MAX_ANCHOR_LINE_CHARS)
        .map(str::to_owned);
    let anchor = line_anchor.or_else(|| {
        file.get("matches")
            .and_then(Value::as_array)
            .and_then(|matches| matches.first())
            .and_then(github_match_anchor)
    });
    let anchored = anchor.is_some();
    if let Some(anchor) = anchor {
        query["matchString"] = json!(anchor);
        query["contextLines"] = json!(20);
    } else {
        output::set_read_range(&mut query, 1, HYDRATED_LINE_RADIUS * 2 + 1);
    }
    Some((
        json!({"tool":ToolId::GhGetFileContent.as_str(),"query":query}),
        anchored,
    ))
}

pub(super) fn candidate_read(
    source: &Value,
    candidate: &Value,
    max_bytes: usize,
) -> Option<(Value, bool)> {
    let (mut read, anchored) = match tool_of(source) {
        Some(ToolId::LocalSearch) => (local_candidate_read(candidate, max_bytes)?, true),
        Some(ToolId::GhSearchCode) => github_candidate_read(candidate, max_bytes)?,
        _ => return None,
    };
    inherit_search_goal(&mut read, source);
    Some((read, anchored))
}

/// The hydrated read is a new tool call. It keeps the search brief so the
/// required goal is the decision the search was opened for.
pub(super) fn inherit_search_goal(read: &mut Value, source: &Value) {
    if read
        .pointer("/query/mainGoal")
        .and_then(Value::as_str)
        .is_none_or(|text| text.trim().is_empty())
        && let Some(goal) = source
            .pointer("/query/mainGoal")
            .filter(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
    {
        read["query"]["mainGoal"] = goal.clone();
    }
}

pub(super) fn pin_github_read(read: &mut Value, state: &Value) {
    if tool_of(read) != Some(ToolId::GhGetFileContent) {
        return;
    }
    if let Some(commit) = state
        .pointer("/results/0/data/commitSha")
        .and_then(Value::as_str)
    {
        read["query"]["ref"] = json!(commit);
    }
}

pub(super) fn default_search_page_size(source: &Value) -> u64 {
    if tool_of(source) == Some(ToolId::GhSearchCode) {
        return 30;
    }
    match source.pointer("/query/resultView").and_then(Value::as_str) {
        Some("files" | "filesWithout" | "discovery" | "countLines" | "countMatches") => 100,
        _ => 20,
    }
}

/// Bound candidate fan-out before executing search. The rewritten page size
/// divides the original starting offset, so the page starts at the same file.
pub(super) fn bounded_search_source(
    source: &Value,
    candidate_limit: usize,
) -> Result<Value, ClassificationError> {
    let list = ResourceSource::of(source).is_some_and(|read| read.is_paged_list());
    if !(is_candidate_search(source) || list) || candidate_limit == 0 {
        return Ok(source.clone());
    }
    let mut bounded = source.clone();
    let query = bounded
        .get_mut("query")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ClassificationError::new(
                "invalidClassificationContext",
                "Search candidate context is missing its query.",
                "Pass one ordinary localSearch or ghSearchCode query.",
            )
        })?;
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    let local = tool_of(source) == Some(ToolId::LocalSearch);
    let original_size = match query.get("pageSize").and_then(Value::as_u64) {
        Some(size) => size,
        // A list tool's first page starts at offset 0 under any page size.
        None if list && page <= 1 => u64::MAX,
        // A later page with an unknown default cannot be re-paged safely;
        // the expanded-cell check still bounds provider work. A localSearch
        // page after the first without `pageSize` is cut by the response
        // budget, so it has no file offset either.
        None if list || (local && page > 1) => return Ok(bounded),
        None => default_search_page_size(source),
    };
    let mut limit = u64::try_from(candidate_limit).unwrap_or(u64::MAX);
    if original_size <= limit {
        // Name the file-page size, so this page's continuations and resumes
        // keep file offsets instead of budget-cut pages.
        if local {
            query.entry("pageSize").or_insert(json!(original_size));
        }
        return Ok(bounded);
    }
    let offset = page.saturating_sub(1).saturating_mul(original_size);
    if offset % limit != 0 {
        // A smaller divisor starts at the same file while fitting the cell
        // budget. Rounding the page number would skip or repeat candidates.
        while offset % limit != 0 {
            limit -= 1;
        }
    }
    query.insert("pageSize".into(), json!(limit));
    query.insert("page".into(), json!(offset / limit + 1));
    Ok(bounded)
}

/// The runtime already continued this page, so its continuation and
/// "continue explicitly" limitation no longer describe caller work.
pub(super) fn mark_followed(context: &mut Value) {
    let Some(receipt) = context.as_object_mut() else {
        return;
    };
    receipt.remove("next");
    if let Some(limitations) = receipt.get_mut("limitations").and_then(Value::as_array_mut) {
        limitations.retain(|limitation| {
            limitation.as_str() != Some(crate::tools::clasify::context::PAGE_ONLY_LIMITATION)
        });
        if limitations.is_empty() {
            receipt.remove("limitations");
        }
    }
}

/// The read a host runs for a kept search candidate: the same anchored
/// window hydration would judge, without the provider byte budget.
pub(super) fn host_read(source: &Value, candidate: &Value) -> Option<Value> {
    let (mut read, _) = candidate_read(source, candidate, MAX_HYDRATED_CHARS)?;
    let query = read.get_mut("query")?.as_object_mut()?;
    query.remove("unit");
    query.remove("length");
    // The matrix brief is copied on by the response stage.
    query.remove("mainGoal");
    query.remove("reasoning");
    read["confidence"] = json!("high");
    Some(read)
}

/// One list candidate as its own page: narrowed evidence, its identity, and
/// the read that fetches it.
pub(super) fn item_page(source: &Value, item: items::Item) -> CapturedPage {
    let mut context = crate::tools::clasify::context::candidate_receipt(source, &item.state);
    if let Some(receipt_source) = context.get_mut("source").and_then(Value::as_object_mut) {
        if let Some(path) = item.path {
            receipt_source.insert("path".into(), json!(path));
        }
        if let Some(identity) = item.item {
            receipt_source.insert("item".into(), json!(identity));
        }
    }
    if let Some(read) = item.read {
        crate::tools::clasify::context::attach_read(&mut context, read);
    }
    CapturedPage::Ready {
        state: candidate_state(source, item.state),
        context,
    }
}

/// Narrower reads tried for one hit window cut short by its byte budget.
pub(super) const RECENTER_ATTEMPTS: usize = 4;

/// Center line and radius of a local hit-window read (one window, not a
/// merged span). Windows clamped at line 1 still end `radius` past their center.
pub(super) fn window_center(read: &Value) -> Option<(u64, u64)> {
    if tool_of(read) != Some(ToolId::LocalFetch)
        || read.pointer("/query/unit").and_then(Value::as_str) != Some("bytes")
    {
        return None;
    }
    let (start, end) = output::read_range(&read["query"])?;
    (end >= start && end - start <= HYDRATED_LINE_RADIUS * 2).then(|| {
        (
            end.saturating_sub(HYDRATED_LINE_RADIUS).max(start),
            HYDRATED_LINE_RADIUS,
        )
    })
}

/// A radius whose window should fit the byte page the cut read returned.
pub(super) fn fitted_radius(radius: u64, state: &Value) -> u64 {
    let pagination = state.pointer("/results/0/data/pagination");
    let ratio = pagination
        .and_then(|page| {
            let chunk = page.get("length")?.as_f64()?;
            let total = page.get("totalBytes")?.as_f64()?;
            (total > 0.0).then_some(chunk / total)
        })
        .unwrap_or(0.5);
    ((radius as f64 * ratio).floor() as u64).min(radius.saturating_sub(1))
}

/// Resolve one hit window. A byte budget cuts a window from its first line,
/// which can drop the hit it is centered on; such a window is re-read narrower
/// around its center until it fits (the last successful read is kept).
pub(super) fn resolve_window(
    mut read: Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> (
    Value,
    Result<(Value, Option<Value>), crate::tools::clasify::context::ContextFailure>,
) {
    let mut resolved = crate::tools::clasify::context::resolve(&read, dispatcher, execution);
    let Some((center, mut radius)) = window_center(&read) else {
        return (read, resolved);
    };
    for _ in 0..RECENTER_ATTEMPTS {
        let Ok((state, receipt)) = &resolved else {
            break;
        };
        if radius == 0
            || receipt
                .as_ref()
                .and_then(crate::tools::clasify::context::continuation)
                .is_none()
        {
            break;
        }
        radius = fitted_radius(radius, state);
        let mut narrower = read.clone();
        output::set_read_range(
            &mut narrower["query"],
            center.saturating_sub(radius).max(1),
            center + radius,
        );
        match crate::tools::clasify::context::resolve(&narrower, dispatcher, execution) {
            Ok(fitted) => {
                read = narrower;
                resolved = Ok(fitted);
            }
            Err(_) => break,
        }
    }
    (read, resolved)
}

/// One candidate read judged as one page. With `whole`, a read the byte
/// budget cut short yields `None` so the caller judges its parts instead.
pub(super) fn hydrate_candidate(
    source: &Value,
    candidate: Value,
    mut read: Value,
    anchored: bool,
    whole: bool,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> Option<CapturedPage> {
    let resolved = if whole {
        crate::tools::clasify::context::resolve(&read, dispatcher, execution)
    } else {
        let (recentered, resolved) = resolve_window(read, dispatcher, execution);
        read = recentered;
        resolved
    };
    Some(match resolved {
        Ok((hydrated_state, hydrated_receipt)) => {
            if whole
                && hydrated_receipt
                    .as_ref()
                    .and_then(crate::tools::clasify::context::continuation)
                    .is_some()
            {
                return None;
            }
            let evidence = provider_state(&read, hydrated_state.clone());
            let evidence_chars = evidence_chars(&evidence);
            let mut context = hydrated_receipt.unwrap_or_else(|| fallback_context(&read));
            crate::tools::clasify::context::append_limitation(
                &mut context,
                "Only a bounded candidate chunk was assessed; unread file content may change the verdict.",
            );
            if !anchored {
                crate::tools::clasify::context::append_limitation(
                    &mut context,
                    "No stable match anchor was available; only the file's opening chunk was assessed.",
                );
            }
            pin_github_read(&mut read, &hydrated_state);
            read["confidence"] = json!("exact");
            crate::tools::clasify::context::attach_read(&mut context, read);
            if evidence_chars == 0 {
                CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationContextEmpty",
                        "The hydrated candidate contained no evidence to judge.",
                        "Read the candidate directly or choose a different search anchor.",
                    ),
                    context,
                }
            } else if evidence_chars > MAX_HYDRATED_CHARS {
                CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationCandidateChunkTooLarge",
                        format!(
                            "The sanitized candidate chunk is {evidence_chars} characters; the limit is {MAX_HYDRATED_CHARS}."
                        ),
                        "Use hints.read to select a smaller exact region.",
                    ),
                    context,
                }
            } else {
                CapturedPage::Ready {
                    state: evidence,
                    context,
                }
            }
        }
        Err(failure) => {
            let mut context = failure.receipt.unwrap_or_else(|| {
                crate::tools::clasify::context::candidate_receipt(source, &candidate)
            });
            crate::tools::clasify::context::append_limitation(
                &mut context,
                "Candidate hydration failed; classification was not run for this file.",
            );
            CapturedPage::Failed {
                error: failure.error,
                context,
            }
        }
    })
}

/// One hydration: a read judged as one page, or a merged span of several
/// cluster windows (`parts`) judged whole when it fits one bounded page and
/// window by window otherwise.
pub(super) struct HydrationJob {
    read: Value,
    anchored: bool,
    parts: Vec<Value>,
}

impl HydrationJob {
    fn single((read, anchored): (Value, bool)) -> Self {
        Self {
            read,
            anchored,
            parts: Vec::new(),
        }
    }

    fn run(
        self,
        source: &Value,
        candidate: &Value,
        dispatcher: &DomainDispatcher,
        execution: &ExecutionContext,
    ) -> Vec<CapturedPage> {
        let hydrate = |read, whole| {
            hydrate_candidate(
                source,
                candidate.clone(),
                read,
                self.anchored,
                whole,
                dispatcher,
                execution,
            )
        };
        if let Some(page) = hydrate(self.read.clone(), !self.parts.is_empty()) {
            return vec![page];
        }
        self.parts
            .iter()
            .filter_map(|part| hydrate(part.clone(), false))
            .collect()
    }
}

/// Reads for each candidate: local candidates get one window per hit
/// cluster within `page_budget` pages in all; others get their one read.
pub(super) fn candidate_jobs(
    source: &Value,
    candidates: &[Value],
    max_bytes: usize,
    page_budget: usize,
    declarations: &HashMap<String, Vec<(usize, usize)>>,
) -> Vec<Option<Vec<HydrationJob>>> {
    if tool_of(source) != Some(ToolId::LocalSearch) {
        return candidates
            .iter()
            .map(|candidate| {
                candidate_read(source, candidate, max_bytes)
                    .map(|read| vec![HydrationJob::single(read)])
            })
            .collect();
    }
    allotted_windows(candidates, page_budget)
        .into_iter()
        .map(|entry| {
            let (path, mut windows, taken) = entry?;
            windows.truncate(taken);
            // Judge a file's windows in source order.
            windows.sort_unstable();
            let read = |window| {
                let mut read = local_window_read(&path, window, max_bytes);
                inherit_search_goal(&mut read, source);
                read
            };
            let spans = declarations.get(&path).map_or(&[][..], Vec::as_slice);
            Some(
                merge_near_windows(windows.clone())
                    .into_iter()
                    .map(|span| {
                        let parts = windows
                            .iter()
                            .filter(|window| span.0 <= window.0 && window.1 <= span.1)
                            .map(|window| read(*window))
                            .collect::<Vec<_>>();
                        // A snapped span is judged whole only when it fits
                        // one page; otherwise its unsnapped windows are.
                        let snapped = snap_to_declarations(span, spans);
                        HydrationJob {
                            read: read(snapped),
                            anchored: true,
                            parts: if parts.len() > 1 || snapped != span {
                                parts
                            } else {
                                Vec::new()
                            },
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

/// One local candidate's file, its hit-cluster windows (densest first), and
/// how many of them the page budget judges (at least one).
pub(super) type AllottedWindows = (String, Vec<(u64, u64)>, usize);

/// [`AllottedWindows`] per candidate; `None` without a usable file identity.
pub(super) fn allotted_windows(
    candidates: &[Value],
    page_budget: usize,
) -> Vec<Option<AllottedWindows>> {
    let windows = candidates
        .iter()
        .map(|candidate| {
            let file = candidate.pointer("/results/0/data/files/0")?;
            let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
            Some((path, hit_cluster_windows(candidate_hit_lines(file))))
        })
        .collect::<Vec<_>>();
    let clusters = windows
        .iter()
        .map(|entry| entry.as_ref().map_or(0, |(_, windows)| windows.len()))
        .collect::<Vec<_>>();
    let taken = allocate_windows(&clusters, page_budget);
    windows
        .into_iter()
        .zip(taken)
        .map(|(entry, taken)| entry.map(|(path, windows)| (path, windows, taken.max(1))))
        .collect()
}

/// Reads of the hit windows the page budget leaves unjudged, per candidate in
/// source order: each becomes a `classificationBudgetSpent` page carrying its
/// read, so no hit cluster drops out of the result.
pub(super) fn unjudged_window_reads(
    source: &Value,
    candidates: &[Value],
    max_bytes: usize,
    page_budget: usize,
) -> Vec<Vec<Value>> {
    if tool_of(source) != Some(ToolId::LocalSearch) {
        return vec![Vec::new(); candidates.len()];
    }
    allotted_windows(candidates, page_budget)
        .into_iter()
        .map(|entry| {
            let Some((path, windows, taken)) = entry else {
                return Vec::new();
            };
            let mut rest = windows.get(taken..).unwrap_or_default().to_vec();
            rest.sort_unstable();
            rest.into_iter()
                .map(|window| {
                    let mut read = local_window_read(&path, window, max_bytes);
                    inherit_search_goal(&mut read, source);
                    read
                })
                .collect()
        })
        .collect()
}

/// Hydrate candidates with `budget` characters shared by every read: each of
/// the planned reads is bounded to an equal share.
pub(super) fn hydrate_candidates(
    source: &Value,
    candidates: Vec<Value>,
    budget: usize,
    page_budget: usize,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Vec<CapturedPage>, ExecutionError> {
    let declarations = if tool_of(source) == Some(ToolId::LocalSearch) {
        local_declaration_spans(&dispatcher.paths, &dispatcher.security, &candidates)
    } else {
        HashMap::new()
    };
    let planned = candidate_jobs(source, &candidates, 1, page_budget, &declarations)
        .iter()
        .flatten()
        .map(Vec::len)
        .sum::<usize>();
    let max_bytes = budget
        .checked_div(planned.max(1))
        .unwrap_or(budget)
        .clamp(1, MAX_HYDRATED_CHARS);
    let jobs = candidate_jobs(source, &candidates, max_bytes, page_budget, &declarations);
    let unjudged = unjudged_window_reads(source, &candidates, max_bytes, page_budget);
    std::thread::scope(|scope| {
        let mut completed = Vec::with_capacity(candidates.len());
        let mut tasks = Vec::with_capacity(candidates.len());
        for (index, ((candidate, job), unjudged)) in
            candidates.into_iter().zip(jobs).zip(unjudged).enumerate()
        {
            execution.check()?;
            let Some(job) = job else {
                completed.push((
                    (index, 0),
                    vec![CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationCandidateUnhydratable",
                            "A search result did not contain a usable file identity.",
                            "Run the search directly and inspect the malformed candidate.",
                        ),
                        context: crate::tools::clasify::context::candidate_receipt(
                            source, &candidate,
                        ),
                    }],
                ));
                continue;
            };
            let judged = job.len();
            for (offset, read) in unjudged.into_iter().enumerate() {
                let mut context =
                    crate::tools::clasify::context::candidate_receipt(source, &candidate);
                crate::tools::clasify::context::attach_read(&mut context, read);
                completed.push((
                    (index, judged + offset),
                    vec![CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationBudgetSpent",
                            "This hit window was not judged: the call's page budget was spent on denser clusters.",
                            "Run its hints.read, or narrow the search to classify it.",
                        ),
                        context,
                    }],
                ));
            }
            for (window, job) in job.into_iter().enumerate() {
                // Acquire before spawning so at most the permitted number of
                // blocking workers exists; later reads wait in this loop.
                let call_permit = reads.acquire(execution)?;
                let process_permit = PROCESS_READS.acquire(execution)?;
                let candidate = candidate.clone();
                tasks.push((
                    (index, window),
                    scope.spawn(move || {
                        let (_call_permit, _process_permit) = (call_permit, process_permit);
                        job.run(source, &candidate, dispatcher, execution)
                    }),
                ));
            }
        }
        for (key, task) in tasks {
            completed.push((key, task.join().map_err(|_| ExecutionError::WorkerFailed)?));
        }
        completed.sort_by_key(|(key, _)| *key);
        Ok(completed.into_iter().flat_map(|(_, pages)| pages).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single `ranges` window of a read.
    fn span(read: &Value) -> (u64, u64) {
        crate::tools::clasify::output::read_range(&read["query"])
            .unwrap_or_else(|| panic!("ranges: {read}"))
    }

    #[test]
    fn code_search_pages_split_into_stable_file_candidates() {
        let state = json!({"results":[{"data":{"files":[
            {"owner":"o","repo":"r","path":"a.rs","matches":[{"value":"alpha"}]},
            {"owner":"O","repo":"R","path":"a.rs","matches":[{"value":"duplicate"}]},
            {"owner":"o","repo":"r","path":"b.rs","matches":[{"value":"beta"}]}
        ],"next":{"nextPage":{"tool":"ghSearchCode","query":{"page":2}}}}}]});
        let code = json!({"tool":"ghSearchCode","query":{}});
        let candidates = search_candidate_states(&code, &state).expect("code candidates");
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0]["results"][0]["data"]["files"][0]["path"],
            "a.rs"
        );
        assert_eq!(
            candidates[1]["results"][0]["data"]["files"][0]["path"],
            "b.rs"
        );
        assert_eq!(
            candidates[0]["results"][0]["data"]["next"], state["results"][0]["data"]["next"],
            "the original search continuation stays available after fan-out"
        );
        let receipt = crate::tools::clasify::context::candidate_receipt(&code, &candidates[0]);
        assert_eq!(receipt["source"]["path"], "o/r/a.rs");
        let tree = json!({"tool":"ghStructure","query":{}});
        assert!(
            search_candidate_states(&tree, &state).is_none(),
            "tree entries are one discovery page, not code candidates"
        );
    }

    /// A repo-scoped page names owner/repo once (or only in the query, when
    /// the echo was minimized) and lists numbered `lines`: each candidate
    /// keeps its identity and reads from its first hit line.
    #[test]
    fn repo_scoped_code_pages_keep_candidate_identity_and_line_anchors() {
        let state = json!({"results":[{"data":{"files":[
            {"path":"a.rs","lines":["12\tfn alpha() {}","40\talpha();"]},
            {"path":"b.rs","matches":[{"value":"beta"}],"lineResolved":false}
        ]}}]});
        let code =
            json!({"tool":"ghSearchCode","query":{"owner":"o","repo":"r","keywords":["alpha"]}});
        let candidates = search_candidate_states(&code, &state).expect("code candidates");
        assert_eq!(candidates.len(), 2);
        let first = &candidates[0]["results"][0]["data"]["files"][0];
        assert_eq!(
            (first["owner"].as_str(), first["repo"].as_str()),
            (Some("o"), Some("r"))
        );
        let (read, anchored) = github_candidate_read(&candidates[0], 4000).expect("read");
        assert!(anchored);
        assert_eq!(read["query"]["matchString"], "fn alpha() {}", "{read}");
        assert_eq!(read["query"]["owner"], "o");
    }

    #[test]
    fn more_lines_runs_expand_and_their_omitted_count_names_no_line() {
        let file = json!({"matches":[{"line":3}],
            "pagination":{"moreLines":"1,5-7,+40 more"}});
        assert_eq!(candidate_hit_lines(&file), [1, 3, 5, 6, 7]);
    }

    #[test]
    fn a_clipped_candidate_is_judged_at_every_hit_cluster() {
        // Shown rows cluster near the top; the rows a clipped file lists in
        // moreLines include a far cluster that holds the deciding line.
        let candidate = json!({"root":"/repo","results":[{"data":{"files":[{
            "path":"scrape.go","matches":[{"line":700},{"line":711},{"line":718}],
            "pagination":{"totalItems":6,"moreLines":"1969,2159,2163"}
        }]}}]});
        let reads = local_candidate_reads(&candidate, 12_000).expect("reads");
        let windows = reads.iter().map(span).collect::<Vec<_>>();
        assert_eq!(
            windows,
            [(649, 769), (2101, 2221), (1909, 2029)],
            "densest cluster first, then the rest"
        );
    }

    /// Windows of one file that overlap or sit within a window radius of each
    /// other are judged as one contiguous page (one provider call), up to
    /// three windows' span; far windows stay separate.
    #[test]
    fn near_windows_of_one_file_merge_into_one_span() {
        assert_eq!(
            merge_near_windows(vec![(1, 74), (51, 171), (182, 302)]),
            [(1, 302)]
        );
        assert_eq!(
            merge_near_windows(vec![(2351, 2471), (2658, 2778)]),
            [(2351, 2471), (2658, 2778)]
        );
        assert_eq!(
            merge_near_windows(vec![(1, 121), (122, 242), (243, 363), (364, 484)]),
            [(1, 363), (364, 484)]
        );
        assert_eq!(merge_near_windows(vec![(5, 125)]), [(5, 125)]);
    }

    /// A hydrated window whose edge cuts a declaration grows to that
    /// declaration's boundary when it is near; a far boundary (an enclosing
    /// impl or module) leaves the edge where the hits put it.
    #[test]
    fn hydrated_windows_snap_to_nearby_declaration_boundaries() {
        // `poll_proceed` 343-364 inside a 60-574 module; `Coop` 400-460.
        let spans = [(60, 574), (290, 320), (343, 364), (400, 460)];
        assert_eq!(snap_to_declarations((72, 350), &spans), (72, 364));
        assert_eq!(snap_to_declarations((407, 527), &spans), (400, 527));
        // Edges outside every nearby declaration stay.
        assert_eq!(snap_to_declarations((321, 342), &spans), (321, 342));
        assert_eq!(snap_to_declarations((5, 50), &[]), (5, 50));
    }

    /// Snapped spans are judged whole only when they fit the page; the
    /// unsnapped windows stay as the fallback, so a declaration never costs
    /// the hit its window was centered on.
    #[test]
    fn snapped_jobs_keep_the_unsnapped_windows_as_fallback() {
        let source =
            json!({"tool":"localSearch","query":{"path":"src","matchString":"task budget"}});
        let candidate = json!({"results":[{"data":{"files":[{
            "path":"src/coop.rs","matches":[{"line":129},{"line":136},{"line":271},{"line":291},{"line":310}]
        }]}}]});
        let spans = HashMap::from([("src/coop.rs".to_owned(), vec![(343, 364)])]);
        let jobs = candidate_jobs(&source, std::slice::from_ref(&candidate), 4_000, 10, &spans);
        let [Some(jobs)] = &jobs[..] else {
            panic!("one candidate");
        };
        let [job] = &jobs[..] else {
            panic!("one merged span");
        };
        assert_eq!(span(&job.read), (72, 364));
        let parts: Vec<_> = job.parts.iter().map(span).collect();
        assert_eq!(parts, [(72, 192), (230, 350)]);
        // A single window that snaps falls back to itself.
        let single = json!({"results":[{"data":{"files":[{
            "path":"src/coop.rs","matches":[{"line":300}]
        }]}}]});
        let jobs = candidate_jobs(&source, &[single], 4_000, 10, &spans);
        let job = &jobs[0].as_ref().expect("job")[0];
        assert_eq!(span(&job.read), (240, 364));
        assert_eq!(job.parts.len(), 1);
        assert_eq!(span(&job.parts[0]).1, 360);
        // Without spans nothing changes.
        let plain = candidate_jobs(&source, &[candidate], 4_000, 10, &HashMap::new());
        let job = &plain[0].as_ref().expect("job")[0];
        assert_eq!(span(&job.read).1, 350);
    }

    /// Hit windows past the page budget are not judged, but each stays
    /// reachable as its own read: judged plus unjudged windows cover every
    /// hit of every candidate.
    #[test]
    fn hit_windows_past_the_page_budget_stay_reachable() {
        let source =
            json!({"tool":"localSearch","query":{"path":"src","matchString":"task budget"}});
        let candidate = json!({"results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":10},{"line":1000},{"line":2000}]
        }]}}]});
        let candidates = std::slice::from_ref(&candidate);
        let judged = candidate_jobs(&source, candidates, 4_000, 1, &HashMap::new());
        let unjudged = unjudged_window_reads(&source, candidates, 4_000, 1);
        assert_eq!(unjudged.len(), 1);
        assert_eq!(unjudged[0].len(), 2, "{unjudged:?}");
        let windows = judged[0]
            .as_ref()
            .expect("jobs")
            .iter()
            .map(|job| &job.read)
            .chain(&unjudged[0])
            .map(|read| {
                assert_eq!(read["query"]["path"], "src/a.rs");
                span(read)
            })
            .collect::<Vec<_>>();
        for hit in [10, 1000, 2000] {
            assert!(
                windows
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(&hit)),
                "{hit}: {windows:?}"
            );
        }
        // A budget that covers every cluster leaves nothing unjudged.
        assert!(unjudged_window_reads(&source, candidates, 4_000, 10)[0].is_empty());
    }

    #[test]
    fn cluster_windows_share_the_page_budget_round_robin() {
        assert_eq!(allocate_windows(&[3, 1, 4], 25), [3, 1, 4]);
        assert_eq!(allocate_windows(&[3, 1, 4], 5), [2, 1, 2]);
        // Every candidate keeps its densest cluster even past the budget.
        assert_eq!(allocate_windows(&[2, 2, 2], 2), [1, 1, 1]);
        assert_eq!(allocate_windows(&[0, 2], 5), [0, 2]);
    }

    #[test]
    fn cut_windows_recenter_on_their_hit_and_shrink_to_the_returned_page() {
        let read = |window| local_window_read("/w/a.rs", window, 3000);
        let [(start, end)] = hit_cluster_windows(vec![150])[..] else {
            panic!("one cluster");
        };
        assert_eq!(window_center(&read((start, end))), Some((150, 60)));
        // Clamped at line 1, the window still ends one radius past its hit.
        let [clamped] = hit_cluster_windows(vec![20])[..] else {
            panic!("one cluster");
        };
        assert_eq!(window_center(&read(clamped)), Some((20, 60)));
        // A merged span is judged through its parts, never re-centered.
        assert_eq!(window_center(&read((1, 300))), None);
        let cut = json!({"results":[{"data":{"pagination":{"length":3000,"totalBytes":12000}}}]});
        assert_eq!(fitted_radius(60, &cut), 15);
        assert_eq!(fitted_radius(1, &cut), 0);
    }

    #[test]
    fn hydrated_candidate_reads_are_bounded_and_source_specific() {
        let local = json!({"root":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":90,"value":"needle"}]
        }]}}]});
        let read = local_candidate_read(&local, 12_000).expect("local read");
        assert_eq!(read["tool"], "localFetch");
        // Rows are workspace-relative, which localFetch resolves as-is.
        assert_eq!(read["query"]["path"], "src/a.rs");
        assert_eq!(span(&read), (30, 150));
        assert_eq!(read["query"]["length"], 12_000);

        // An incidental first hit must not pull the window away from the
        // cluster that holds the declaration and its uses.
        let clustered = json!({"root":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":21},{"line":283},{"line":288},
                {"line":293},{"line":294},{"line":507}]
        }]}}]});
        let read = local_candidate_read(&clustered, 12_000).expect("clustered read");
        assert_eq!(span(&read), (228, 348));

        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"src/a.rs","matches":[{
                "value":"fn needle() {}", "matchIndices":[{"start":3,"end":9}]
            }]
        }]}}]});
        let (mut read, anchored) = github_candidate_read(&github, 8_000).expect("github read");
        assert!(anchored);
        assert_eq!(read["query"]["matchString"], "fn needle() {}");
        assert_eq!(read["query"]["length"], 8_000);
        let hydrated = json!({"results":[{"data":{"commitSha":"abc123"}}]});
        pin_github_read(&mut read, &hydrated);
        assert_eq!(read["query"]["ref"], "abc123");
    }

    #[test]
    fn github_candidate_read_anchors_on_the_densest_matched_line() {
        // A bare keyword ("semaphore") recurs across the file, so anchoring on
        // it reads every occurrence; the matched line is the hit itself.
        let value = "        }\n\n        let guard = WakeReceiverOnDrop { chan: &self.chan };\n        let result = self.chan.semaphore().semaphore.acquire(n).await;\n\n        match result {";
        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"src/bounded.rs","matches":[{
                "value":value,
                "matchIndices":[
                    {"start":103,"end":112,"lineOffset":3},
                    {"start":115,"end":124,"lineOffset":3},
                    {"start":125,"end":132,"lineOffset":3}
                ]
            }]
        }]}}]});
        let (read, anchored) = github_candidate_read(&github, 8_000).expect("github read");
        assert!(anchored);
        assert_eq!(
            read["query"]["matchString"],
            "let result = self.chan.semaphore().semaphore.acquire(n).await;"
        );
        // Without lineOffset the line is derived from the match start; an
        // overlong (minified) line falls back to the matched term.
        let long = format!("{} needle {}", "x".repeat(300), "y".repeat(300));
        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"a.min.js","matches":[
                {"value":format!("a\n{long}"),"matchIndices":[{"start":303,"end":309}]}
            ]
        }]}}]});
        let (read, _) = github_candidate_read(&github, 8_000).expect("minified read");
        assert_eq!(read["query"]["matchString"], "needle");
    }

    #[test]
    fn list_tools_bound_their_first_page_to_the_cell_budget() {
        let repos = json!({"tool":"ghSearchRepo","query":{"mainGoal":"g","reasoning":"r","keywords":["x"]}});
        let bounded = bounded_search_source(&repos, 6).expect("first page");
        assert_eq!(bounded["query"]["pageSize"], 6);
        assert_eq!(bounded["query"]["page"], 1);
        let packages = json!({"tool":"artifactSearch","query":{
            "mainGoal":"g","reasoning":"r","type":"npm","keywords":["x"],"page":2,"pageSize":20
        }});
        let bounded = bounded_search_source(&packages, 5).expect("later page");
        assert_eq!(bounded["query"]["pageSize"], 5);
        assert_eq!(bounded["query"]["page"], 5);
        let later = json!({"tool":"ghSearchHistory","query":{"operation":"issue","page":3}});
        assert_eq!(
            bounded_search_source(&later, 5).expect("unknown default"),
            later
        );
        let symbols = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/r"}});
        assert_eq!(
            bounded_search_source(&symbols, 5).expect("grouped"),
            symbols
        );
    }

    #[test]
    fn candidate_page_bound_preserves_the_original_search_offset() {
        let first = json!({"tool":"localSearch","query":{
            "mainGoal": "test", "reasoning":"find","path":"/repo","matchString":"x","pageSize":20
        }});
        let bounded = bounded_search_source(&first, 5).expect("first page");
        assert_eq!(bounded["query"]["page"], 1);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let aligned = json!({"tool":"ghSearchCode","query":{
            "mainGoal": "test", "reasoning":"find","owner":"o","keywords":["x"],
            "page":2,"pageSize":20
        }});
        let bounded = bounded_search_source(&aligned, 5).expect("aligned offset");
        assert_eq!(bounded["query"]["page"], 5);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let unaligned = json!({"tool":"localSearch","query":{
            "mainGoal": "test", "reasoning":"find","path":"/repo","matchString":"x","page":2,"pageSize":6
        }});
        let bounded = bounded_search_source(&unaligned, 5).expect("aligned smaller page");
        assert_eq!(bounded["query"]["page"], 3);
        assert_eq!(bounded["query"]["pageSize"], 3);
        assert_eq!(
            (bounded["query"]["page"].as_u64().expect("page") - 1)
                * bounded["query"]["pageSize"].as_u64().expect("page size"),
            6
        );
    }

    #[test]
    fn concise_search_candidates_keep_per_file_identity_and_executable_reads() {
        let source = json!({"tool":"ghSearchCode", "query":{"mainGoal":"find evidence"}});
        let state = json!({"results":[{"data":{"files":["o/r:src/a.rs", "o/r:src/b.rs", "o/r:src/a.rs"]}}]});
        let candidates = search_candidate_states(&source, &state).expect("concise candidates");
        assert_eq!(candidates.len(), 2);
        for (candidate, path) in candidates.iter().zip(["src/a.rs", "src/b.rs"]) {
            let file = &candidate["results"][0]["data"]["files"][0];
            assert_eq!(
                candidate_identity(&source, file),
                Some(format!("o/r/{path}"))
            );
            let read = host_read(&source, candidate).expect("candidate read");
            assert_eq!(read["tool"], "ghGetFileContent");
            assert_eq!(read["query"]["owner"], "o");
            assert_eq!(read["query"]["repo"], "r");
            assert_eq!(read["query"]["path"], path);
        }
    }

    #[test]
    fn followed_page_receipt_drops_its_continuation_but_keeps_other_limits() {
        let mut context = json!({"coverage":"partial","next":{"continue":{}},"limitations":[
            crate::tools::clasify::context::PAGE_ONLY_LIMITATION
        ]});
        mark_followed(&mut context);
        assert_eq!(context, json!({"coverage":"partial"}));
        let mut terminal = json!({"next":{"continue":{}},"limitations":["terminal limit"]});
        mark_followed(&mut terminal);
        assert_eq!(terminal, json!({"limitations":["terminal limit"]}));
    }
}
