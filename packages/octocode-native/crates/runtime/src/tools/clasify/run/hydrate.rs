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
    // `moreLines` lists line numbers; `moreLinesUnlisted` counts the rest.
    let more = file
        .pointer("/pagination/moreLines")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64);
    let mut lines = shown.chain(more).collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Contiguous windows (`2 × HYDRATED_LINE_RADIUS + 1` lines) covering every
/// hit cluster, densest first: each centers on the densest remaining run of
/// hits (an incidental first hit must not pull a window off its cluster), and
/// the hits it covers leave the pool. No hits yields the opening window.
pub(super) fn hit_cluster_windows(lines: Vec<u64>) -> Vec<(u64, u64)> {
    let mut windows = hit_cluster_windows_at(HYDRATED_LINE_RADIUS, lines);
    if windows.is_empty() {
        windows.push((1, HYDRATED_LINE_RADIUS * 2 + 1));
    }
    windows
}

/// [`hit_cluster_windows`] at any `radius`; no hits yields no window.
fn hit_cluster_windows_at(radius: u64, mut lines: Vec<u64>) -> Vec<(u64, u64)> {
    let mut windows = Vec::new();
    while let Some((first, last)) = densest_run(&lines, radius * 2) {
        let center = first + (last - first) / 2;
        let start = center.saturating_sub(radius).max(1);
        let end = center.saturating_add(radius);
        windows.push((start, end));
        lines.retain(|line| *line < start || *line > end);
    }
    windows
}

/// Sub-windows of a hit window cut short by its byte budget, at the fitted
/// `radius`, densest first: the window's own hits re-clustered, so no
/// sub-window sits between hits with none in it. A window without hits (the
/// opening window) narrows on its center.
pub(super) fn recut_window(hits: &[u64], (start, end): (u64, u64), radius: u64) -> Vec<(u64, u64)> {
    let inside = hits
        .iter()
        .copied()
        .filter(|hit| (start..=end).contains(hit))
        .collect::<Vec<_>>();
    if inside.is_empty() {
        let center = end.saturating_sub(HYDRATED_LINE_RADIUS).max(start);
        return vec![(center.saturating_sub(radius).max(1), center + radius)];
    }
    hit_cluster_windows_at(radius, inside)
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

/// The rows a local candidate shows: the lines a snippet page judges.
pub(super) fn shown_hit_lines(file: &Value) -> Vec<u64> {
    let mut lines = file
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|matched| matched.get("line").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// `ranges` values naming `windows`.
fn ranges_of(windows: &[(u64, u64)]) -> Value {
    windows
        .iter()
        .map(|(start, end)| json!(format!("{start}-{end}")))
        .collect()
}

/// The read of a local candidate's snippet page: it covers every row the
/// page judged. That is the densest hit window when it holds them all; else
/// each cluster of shown rows gets its window (merged when near), since a
/// denser cluster of rows the page only lists was not judged.
pub(super) fn local_candidate_read(candidate: &Value, max_bytes: usize) -> Option<Value> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
    let densest = *hit_cluster_windows(candidate_hit_lines(file)).first()?;
    let shown = shown_hit_lines(file);
    if shown
        .iter()
        .all(|line| (densest.0..=densest.1).contains(line))
    {
        return Some(local_window_read(&path, densest, max_bytes));
    }
    let mut windows = hit_cluster_windows(shown);
    windows.sort_unstable();
    let mut windows = merge_near_windows(windows);
    if windows.len() > MAX_READ_RANGES {
        // One read names at most this many ranges; one span still covers
        // every judged row.
        windows = vec![(windows[0].0, windows[windows.len() - 1].1)];
    }
    let mut read = local_window_read(&path, windows[0], max_bytes);
    read["query"]["ranges"] = ranges_of(&windows);
    Some(read)
}

/// Most `ranges` one localFetch row names.
const MAX_READ_RANGES: usize = crate::tools::local_fetch::MAX_READ_RANGES;

/// Rows one localFetch call accepts (the contract's `queries.maxItems`).
fn fetch_rows_per_call() -> usize {
    crate::contracts::tool_contract(ToolId::LocalFetch)
        .ok()
        .and_then(|contract| contract.pointer("/inputSchema/properties/queries/maxItems"))
        .and_then(Value::as_u64)
        .and_then(|rows| usize::try_from(rows).ok())
        .unwrap_or(1)
        .max(1)
}

/// Reads of `spans` of one file, as few as the localFetch limits allow:
/// each names every span once, up to [`MAX_READ_RANGES`] per row and the
/// contract's rows per call.
pub(super) fn batched_window_reads(path: &str, spans: &[(u64, u64)]) -> Vec<Value> {
    spans
        .chunks(MAX_READ_RANGES * fetch_rows_per_call())
        .map(|chunk| {
            let mut rows = chunk
                .chunks(MAX_READ_RANGES)
                .map(|ranges| json!({"path":path, "ranges":ranges_of(ranges)}))
                .collect::<Vec<_>>();
            let query = if rows.len() == 1 {
                rows.remove(0)
            } else {
                json!({"queries":rows})
            };
            json!({"tool":ToolId::LocalFetch.as_str(), "query":query})
        })
        .collect()
}

/// Source-ordered read windows covering every judged row line of one list
/// candidate: a window per hit cluster, merged when near, as one span past
/// the localFetch range limit. No lines yields no window.
pub(in crate::tools::clasify) fn judged_line_windows(mut lines: Vec<u64>) -> Vec<(u64, u64)> {
    lines.sort_unstable();
    let mut windows = hit_cluster_windows_at(HYDRATED_LINE_RADIUS, lines);
    windows.sort_unstable();
    let windows = merge_near_windows(windows);
    match (windows.first(), windows.last()) {
        (Some(first), Some(last)) if windows.len() > MAX_READ_RANGES => vec![(first.0, last.1)],
        _ => windows,
    }
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

/// The read a window resolved with, its result, and the reads of the hits a
/// narrowed window left out.
pub(super) type ResolvedWindow = (
    Value,
    Result<(Value, Option<Value>), crate::tools::clasify::context::ContextFailure>,
    Vec<Value>,
);

/// Resolve one hit window. A byte budget cuts a window from its first line,
/// which can drop the hits it holds; such a window is re-read narrower on its
/// densest hits until it fits (the last successful read is kept), and the
/// reads of its other hits come back as `rest` so none drops out.
pub(super) fn resolve_window(
    mut read: Value,
    hits: &[u64],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> ResolvedWindow {
    let mut resolved = crate::tools::clasify::context::resolve(&read, dispatcher, execution);
    let mut rest = Vec::new();
    let window = output::read_range(&read["query"]);
    let (Some((_, mut radius)), Some(window)) = (window_center(&read), window) else {
        return (read, resolved, rest);
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
        let windows = recut_window(hits, window, radius);
        let with_range = |(start, end): (u64, u64)| {
            let mut narrower = read.clone();
            output::set_read_range(&mut narrower["query"], start, end);
            narrower
        };
        let narrower = with_range(windows[0]);
        match crate::tools::clasify::context::resolve(&narrower, dispatcher, execution) {
            Ok(fitted) => {
                rest = windows[1..].iter().copied().map(with_range).collect();
                read = narrower;
                resolved = Ok(fitted);
            }
            Err(_) => break,
        }
    }
    (read, resolved, rest)
}

/// A hit window the byte budget left unjudged: reported with its read.
fn recut_rest_page(source: &Value, candidate: &Value, read: Value) -> CapturedPage {
    let mut context = crate::tools::clasify::context::candidate_receipt(source, candidate);
    crate::tools::clasify::context::attach_read(&mut context, read);
    CapturedPage::Failed {
        error: ClassificationError::new(
            "classificationBudgetSpent",
            "This hit window was not judged: the byte budget narrowed its window to denser hits.",
            "Run its hints.read to classify it.",
        ),
        context,
    }
}

/// One candidate read judged as one page. With `whole`, a read the byte
/// budget cut short yields `None` so the caller judges its parts instead.
pub(super) fn hydrate_candidate(
    source: &Value,
    candidate: Value,
    mut read: Value,
    hits: &[u64],
    anchored: bool,
    whole: bool,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> Option<Vec<CapturedPage>> {
    let mut rest = Vec::new();
    let resolved = if whole {
        crate::tools::clasify::context::resolve(&read, dispatcher, execution)
    } else {
        let (recentered, resolved, unjudged) = resolve_window(read, hits, dispatcher, execution);
        read = recentered;
        rest = unjudged;
        resolved
    };
    let page = match resolved {
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
    };
    let mut pages = vec![page];
    pages.extend(
        rest.into_iter()
            .map(|read| recut_rest_page(source, &candidate, read)),
    );
    Some(pages)
}

/// One hydration: a read judged as one page, or a merged span of several
/// cluster windows (`parts`) judged whole when it fits one bounded page and
/// window by window otherwise.
pub(super) struct HydrationJob {
    read: Value,
    anchored: bool,
    parts: Vec<Value>,
    /// Hit lines of the candidate, so a cut window narrows onto its hits.
    hits: Vec<u64>,
}

impl HydrationJob {
    fn single((read, anchored): (Value, bool)) -> Self {
        Self {
            read,
            anchored,
            parts: Vec::new(),
            hits: Vec::new(),
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
                &self.hits,
                self.anchored,
                whole,
                dispatcher,
                execution,
            )
        };
        if let Some(pages) = hydrate(self.read.clone(), !self.parts.is_empty()) {
            return pages;
        }
        self.parts
            .iter()
            .filter_map(|part| hydrate(part.clone(), false))
            .flatten()
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
        .zip(candidates)
        .map(|(entry, candidate)| {
            let (path, mut windows, taken) = entry?;
            let hits = candidate
                .pointer("/results/0/data/files/0")
                .map(candidate_hit_lines)
                .unwrap_or_default();
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
                            hits: hits.clone(),
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

/// The hit windows the page budget leaves unjudged, per candidate in source
/// order: they join the candidate's batched `classificationBudgetSpent` read
/// ([`consolidate_candidate_pages`]), so no hit cluster drops out.
pub(super) fn unjudged_windows(
    source: &Value,
    candidates: &[Value],
    page_budget: usize,
) -> Vec<Vec<(u64, u64)>> {
    if tool_of(source) != Some(ToolId::LocalSearch) {
        return vec![Vec::new(); candidates.len()];
    }
    allotted_windows(candidates, page_budget)
        .into_iter()
        .map(|entry| {
            entry
                .and_then(|(_, windows, taken)| windows.get(taken..).map(<[_]>::to_vec))
                .unwrap_or_default()
        })
        .collect()
}

/// Hit lines a local candidate's search page counted but did not list.
pub(super) fn unlisted_hit_count(file: &Value) -> u64 {
    file.pointer("/pagination/moreLinesUnlisted")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// The search that lists every hit line of one local candidate: the page's
/// matcher over that file alone, all of its rows on one match page. File
/// selection (globs, depth, order, paging) and row shaping do not apply to
/// one named file; the snapshot names the directory scan, so it is dropped.
pub(super) fn hit_lines_search(source: &Value, file: &Value) -> Option<Value> {
    let path = file.get("path")?.as_str()?;
    let rows = file
        .pointer("/pagination/totalItems")
        .and_then(Value::as_u64)?;
    let mut search = source.clone();
    let query = search.get_mut("query")?.as_object_mut()?;
    for field in [
        "page",
        "pageSize",
        "matchPage",
        "snapshot",
        "include",
        "exclude",
        "maxDepth",
        "sort",
        "reverse",
        "resultView",
        "unique",
        "contextLines",
        "matchContentLength",
    ] {
        query.remove(field);
    }
    query.insert("path".into(), json!(path));
    query.insert("matchPageSize".into(), json!(rows.max(1)));
    Some(search)
}

/// D1: a candidate whose search page only counted some hit lines
/// (`moreLinesUnlisted`) gets them listed by [`hit_lines_search`], so every
/// hit line reaches a window. A search that cannot list them all leaves the
/// candidate as it was and returns a `classificationBudgetSpent` page that
/// names the shortfall and carries that search as its read.
fn list_unlisted_hits(
    source: &Value,
    candidate: &mut Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Option<CapturedPage>, ExecutionError> {
    let Some(file) = candidate.pointer("/results/0/data/files/0") else {
        return Ok(None);
    };
    let unlisted = unlisted_hit_count(file);
    if unlisted == 0 {
        return Ok(None);
    }
    let Some(search) = hit_lines_search(source, file) else {
        return Ok(None);
    };
    let listed = match resolve_limited(&search, dispatcher, execution, reads)? {
        Ok((state, _)) => state
            .pointer("/results/0/data/files/0")
            .filter(|fetched| unlisted_hit_count(fetched) == 0)
            .map(candidate_hit_lines),
        Err(_) => None,
    };
    if let Some(lines) = listed
        && let Some(pagination) = candidate
            .pointer_mut("/results/0/data/files/0/pagination")
            .and_then(Value::as_object_mut)
    {
        let mut more = pagination
            .get("moreLines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .chain(lines)
            .collect::<Vec<_>>();
        more.sort_unstable();
        more.dedup();
        pagination.insert("moreLines".into(), json!(more));
        pagination.remove("moreLinesUnlisted");
        return Ok(None);
    }
    let mut context = crate::tools::clasify::context::candidate_receipt(source, candidate);
    crate::tools::clasify::context::attach_read(&mut context, search);
    Ok(Some(CapturedPage::Failed {
        error: ClassificationError::new(
            "classificationBudgetSpent",
            format!(
                "The search page counted {unlisted} hit lines of this file without listing them; they were not judged."
            ),
            "Run its hints.read to list them, then classify those lines.",
        ),
        context,
    }))
}

/// The one window a hit-window page reads, when it reads one.
fn page_window(page: &CapturedPage) -> Option<(u64, u64)> {
    output::read_range(&super::capture::page_context(page)["read"]["query"])
}

/// A hit window the page or byte budget left unjudged (its page carries the
/// window's read).
fn unjudged_window(page: &CapturedPage) -> Option<(u64, u64)> {
    match page {
        CapturedPage::Failed { error, .. } if error.code == "classificationBudgetSpent" => {
            page_window(page)
        }
        _ => None,
    }
}

/// D3/D4: one local candidate's hydrated pages with each window once. A
/// window two cut windows re-cut alike is judged on its first page only; an
/// unjudged window (a narrowed window's left-out hits, or `unjudged` windows
/// past the page budget) that a judged page reads, or that repeats, drops
/// out. The rest are batched into reads of the file that name each unjudged
/// line once (overlapping or touching windows merge), each on one
/// `classificationBudgetSpent` page.
pub(super) fn consolidate_candidate_pages(
    source: &Value,
    candidate: &Value,
    pages: Vec<CapturedPage>,
    mut unjudged: Vec<(u64, u64)>,
) -> Vec<CapturedPage> {
    let Some(path) = candidate
        .pointer("/results/0/data/files/0")
        .and_then(|file| candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file))
    else {
        return pages;
    };
    let mut seen = HashSet::new();
    let mut kept = Vec::with_capacity(pages.len());
    for page in pages {
        if let Some(window) = unjudged_window(&page) {
            unjudged.push(window);
            continue;
        }
        let code = match &page {
            CapturedPage::Ready { .. } => None,
            CapturedPage::Failed { error, .. } => Some(error.code.clone()),
        };
        if page_window(&page).is_some_and(|window| !seen.insert((code, window))) {
            continue;
        }
        kept.push(page);
    }
    unjudged.retain(|window| !seen.contains(&(None, *window)));
    let spans = crate::tools::line_spans::merge_spans(unjudged);
    kept.extend(batched_window_reads(&path, &spans).into_iter().map(|read| {
        let mut context = crate::tools::clasify::context::candidate_receipt(source, candidate);
        crate::tools::clasify::context::attach_read(&mut context, read);
        CapturedPage::Failed {
            error: ClassificationError::new(
                "classificationBudgetSpent",
                "These hit windows were not judged: the call's page or byte budget went to denser hits.",
                "Run hints.read (each unjudged window of this file, once), or narrow the search to classify them.",
            ),
            context,
        }
    }));
    kept
}

/// Hydrate candidates with `budget` characters shared by every read: each of
/// the planned reads is bounded to an equal share.
pub(super) fn hydrate_candidates(
    source: &Value,
    mut candidates: Vec<Value>,
    budget: usize,
    page_budget: usize,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Vec<CapturedPage>, ExecutionError> {
    let local = tool_of(source) == Some(ToolId::LocalSearch);
    let mut shortfalls = Vec::with_capacity(candidates.len());
    for candidate in &mut candidates {
        shortfalls.push(if local {
            list_unlisted_hits(source, candidate, dispatcher, execution, reads)?
        } else {
            None
        });
    }
    let declarations = if local {
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
    let unjudged = unjudged_windows(source, &candidates, page_budget);
    let originals = if local {
        candidates.clone()
    } else {
        Vec::new()
    };
    let completed = std::thread::scope(|scope| {
        let mut completed = Vec::with_capacity(candidates.len());
        let mut tasks = Vec::with_capacity(candidates.len());
        for (index, (candidate, job)) in candidates.into_iter().zip(jobs).enumerate() {
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
        Ok::<_, ExecutionError>(completed)
    })?;
    let mut per_candidate = shortfalls.iter().map(|_| Vec::new()).collect::<Vec<_>>();
    for ((index, _), pages) in completed {
        per_candidate[index].extend(pages);
    }
    Ok(per_candidate
        .into_iter()
        .zip(unjudged)
        .zip(shortfalls)
        .enumerate()
        .flat_map(|(index, ((pages, unjudged), shortfall))| {
            let mut pages = match originals.get(index) {
                Some(candidate) => consolidate_candidate_pages(source, candidate, pages, unjudged),
                None => pages,
            };
            pages.extend(shortfall);
            pages
        })
        .collect())
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
    fn more_lines_join_the_shown_hit_lines() {
        let file = json!({"matches":[{"line":3}],
            "pagination":{"moreLines":[1,5,6,7],"moreLinesUnlisted":40}});
        assert_eq!(candidate_hit_lines(&file), [1, 3, 5, 6, 7]);
    }

    #[test]
    fn a_clipped_candidate_is_judged_at_every_hit_cluster() {
        // Shown rows cluster near the top; the rows a clipped file lists in
        // moreLines include a far cluster that holds the deciding line.
        let candidate = json!({"root":"/repo","results":[{"data":{"files":[{
            "path":"scrape.go","matches":[{"line":700},{"line":711},{"line":718}],
            "pagination":{"totalItems":6,"moreLines":[1969,2159,2163]}
        }]}}]});
        let file = &candidate["results"][0]["data"]["files"][0];
        assert_eq!(
            hit_cluster_windows(candidate_hit_lines(file)),
            [(649, 769), (2101, 2221), (1909, 2029)],
            "densest cluster first, then the rest"
        );
    }

    /// D2 (CL5 repro: mod.rs, matchPage 3): a snippet page judged its shown
    /// rows 107-108, while the rows it only lists cluster densely at 489-609.
    /// Its read covers the judged rows, not the denser listed cluster.
    #[test]
    fn a_snippet_read_covers_the_rows_the_page_judged() {
        let ranges = |read: &Value| -> Vec<(u64, u64)> {
            read["query"]["ranges"]
                .as_array()
                .expect("ranges")
                .iter()
                .filter_map(|range| crate::tools::line_spans::parse_span(range.as_str()?))
                .collect()
        };
        let covers = |read: &Value, line: u64| {
            ranges(read)
                .iter()
                .any(|(start, end)| (*start..=*end).contains(&line))
        };
        let listed = (0..14).map(|step| 489 + step * 9).collect::<Vec<u64>>();
        let candidate = json!({"results":[{"data":{"files":[{
            "path":"tools/clasify/mod.rs",
            "matches":[{"line":107,"value":"No classification provider key is configured."},
                {"line":108,"value":"Set OCTOCODE_CLASSIFICATION_API."}],
            "pagination":{"totalItems":37,"hasMore":true,"moreLines":listed}
        }]}}]});
        let read = local_candidate_read(&candidate, 12_000).expect("snippet read");
        for line in [107, 108] {
            assert!(covers(&read, line), "{line}: {read}");
        }
        // Shown rows in two far clusters: one window each, in line order.
        let split = json!({"results":[{"data":{"files":[{
            "path":"a.rs","matches":[{"line":107},{"line":600}],
            "pagination":{"moreLines":listed}
        }]}}]});
        let read = local_candidate_read(&split, 12_000).expect("split read");
        assert_eq!(ranges(&read), [(47, 167), (540, 660)], "{read}");
        // When the densest window already holds every shown row, the read
        // is that window, as before.
        let held = json!({"results":[{"data":{"files":[{
            "path":"a.rs","matches":[{"line":500}],"pagination":{"moreLines":listed}
        }]}}]});
        let read = local_candidate_read(&held, 12_000).expect("held read");
        assert_eq!(ranges(&read).len(), 1, "{read}");
        assert!(covers(&read, 500), "{read}");
    }

    /// D1: the hit lines a search page only counted are listed by the same
    /// matcher over that one file, every row on one match page; file
    /// selection, paging, row shaping, and the directory snapshot drop.
    #[test]
    fn unlisted_hit_lines_are_listed_by_a_one_file_search() {
        let source = json!({"tool":"localSearch","query":{
            "mainGoal":"g","path":"tools/clasify","matchString":"classification",
            "caseMode":"sensitive","wholeWord":true,"include":["*.rs"],"exclude":["tests"],
            "page":2,"pageSize":5,"matchPage":1,"matchPageSize":10,"snapshot":"lexical-live-v4:abc",
            "resultView":"detailed","contextLines":3,"sort":"path"
        }});
        let file = json!({"path":"tools/clasify/transport.rs","matches":[{"line":6}],
            "pagination":{"totalItems":96,"hasMore":true,"moreLines":[1,5],"moreLinesUnlisted":62}});
        assert_eq!(unlisted_hit_count(&file), 62);
        assert_eq!(unlisted_hit_count(&json!({"path":"a.rs"})), 0);
        let search = hit_lines_search(&source, &file).expect("search");
        assert_eq!(
            search,
            json!({"tool":"localSearch","query":{
                "mainGoal":"g","path":"tools/clasify/transport.rs","matchString":"classification",
                "caseMode":"sensitive","wholeWord":true,"matchPageSize":96
            }})
        );
        let input = json!({"queries":[search["query"].clone()]});
        let valid = crate::contracts::validate("localSearch", input);
        assert!(valid.is_ok(), "{valid:?}");
    }

    /// A hydrated page that reads `window` of `path`: judged, or a hit
    /// window a budget left unjudged.
    fn window_page(path: &str, window: (u64, u64), judged: bool) -> CapturedPage {
        let context = json!({"read":local_window_read(path, window, 400)});
        if judged {
            CapturedPage::Ready {
                state: json!({"content":"x"}),
                context,
            }
        } else {
            CapturedPage::Failed {
                error: ClassificationError::new(
                    "classificationBudgetSpent",
                    "This hit window was not judged: the byte budget narrowed its window to denser hits.",
                    "Run its hints.read to classify it.",
                ),
                context,
            }
        }
    }

    /// Every read range of a page's read (one row or a `queries` batch).
    fn read_spans(read: &Value) -> Vec<(u64, u64)> {
        let query = &read["query"];
        query["queries"]
            .as_array()
            .map_or_else(|| vec![query], |rows| rows.iter().collect())
            .into_iter()
            .flat_map(|row| row["ranges"].as_array().into_iter().flatten())
            .filter_map(|range| crate::tools::line_spans::parse_span(range.as_str()?))
            .collect()
    }

    /// D3/D4 (CL5 fc1.json, mod.rs): two cut windows of one merged span
    /// re-cut alike, so 102-108 was judged twice and 108-114 listed twice,
    /// and every unjudged window carried its own error and read. Each
    /// window is now judged once, an unjudged window a judged page reads
    /// drops out, and the rest share one read that names each line once.
    #[test]
    fn hydrated_pages_judge_each_window_once_and_batch_the_unjudged() {
        let path = "tools/clasify/mod.rs";
        let source =
            json!({"tool":"localSearch","query":{"path":"tools","matchString":"classification"}});
        let candidate =
            json!({"results":[{"data":{"files":[{"path":path,"matches":[{"line":103}]}]}}]});
        let pages = vec![
            window_page(path, (102, 108), true),
            window_page(path, (16, 22), false),
            window_page(path, (108, 114), false),
            window_page(path, (102, 108), true),
            window_page(path, (108, 114), false),
            window_page(path, (102, 108), false),
            window_page(path, (509, 515), true),
            window_page(path, (527, 533), false),
            window_page(path, (530, 536), false),
        ];
        let pages = consolidate_candidate_pages(&source, &candidate, pages, vec![(158, 164)]);
        let judged = pages
            .iter()
            .filter(|page| matches!(page, CapturedPage::Ready { .. }))
            .filter_map(page_window)
            .collect::<Vec<_>>();
        assert_eq!(judged, [(102, 108), (509, 515)], "each window judged once");
        let unjudged = pages
            .iter()
            .filter_map(|page| match page {
                CapturedPage::Failed { error, context } => Some((error, context)),
                CapturedPage::Ready { .. } => None,
            })
            .collect::<Vec<_>>();
        let [(error, context)] = unjudged[..] else {
            panic!("one batched page: {}", unjudged.len());
        };
        assert_eq!(error.code, "classificationBudgetSpent");
        assert_eq!(context["read"]["query"]["path"], path);
        assert_eq!(
            read_spans(&context["read"]),
            [(16, 22), (108, 114), (158, 164), (527, 536)],
            "every unjudged line once; a judged window is not re-listed"
        );
        // More unjudged windows than one row names: one read, several rows.
        let many = (0..12)
            .map(|step| window_page(path, (1000 + step * 100, 1006 + step * 100), false))
            .collect();
        let pages = consolidate_candidate_pages(&source, &candidate, many, Vec::new());
        let [CapturedPage::Failed { context, .. }] = &pages[..] else {
            panic!("one batched page");
        };
        let rows = context["read"]["query"]["queries"]
            .as_array()
            .expect("batched rows");
        assert_eq!(rows.len(), 2, "{context}");
        assert_eq!(read_spans(&context["read"]).len(), 12);
        let fetched = crate::contracts::validate("localFetch", context["read"]["query"].clone());
        assert!(fetched.is_ok(), "{fetched:?}");
        // Past one call's rows, further reads; together every window once.
        let windows = (0..61)
            .map(|step| (1000 + step * 100, 1006 + step * 100))
            .collect::<Vec<_>>();
        let reads = batched_window_reads(path, &windows);
        assert_eq!(reads.len(), 2);
        let named = reads.iter().flat_map(read_spans).collect::<Vec<_>>();
        assert_eq!(named, windows);
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
    /// reachable through the candidate's batched read: judged plus unjudged
    /// windows cover every hit of every candidate.
    #[test]
    fn hit_windows_past_the_page_budget_stay_reachable() {
        let source =
            json!({"tool":"localSearch","query":{"path":"src","matchString":"task budget"}});
        let candidate = json!({"results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":10},{"line":1000},{"line":2000}]
        }]}}]});
        let candidates = std::slice::from_ref(&candidate);
        let judged = candidate_jobs(&source, candidates, 4_000, 1, &HashMap::new());
        let mut unjudged = unjudged_windows(&source, candidates, 1);
        assert_eq!(unjudged.len(), 1);
        assert_eq!(unjudged[0].len(), 2, "{unjudged:?}");
        let pages =
            consolidate_candidate_pages(&source, &candidate, Vec::new(), unjudged.remove(0));
        let [CapturedPage::Failed { context, .. }] = &pages[..] else {
            panic!("one batched page");
        };
        assert_eq!(context["read"]["query"]["path"], "src/a.rs");
        let windows = judged[0]
            .as_ref()
            .expect("jobs")
            .iter()
            .map(|job| span(&job.read))
            .chain(read_spans(&context["read"]))
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
        assert!(unjudged_windows(&source, candidates, 10)[0].is_empty());
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

    /// N1: a cut window holding two far hits re-clusters its own hits at the
    /// fitted radius: every sub-window holds a hit (never the empty midpoint
    /// between them), the densest is read first, and together they cover
    /// every hit, so the rest stay reachable as reads.
    #[test]
    fn cut_window_recenters_on_hits_and_keeps_the_rest() {
        let [window] = hit_cluster_windows(vec![429, 483])[..] else {
            panic!("one cluster");
        };
        assert_eq!(window, (396, 516));
        let cut = json!({"results":[{"data":{"pagination":{"length":600,"totalBytes":12000}}}]});
        let radius = fitted_radius(60, &cut);
        assert_eq!(radius, 3);
        let windows = recut_window(&[429, 483], window, radius);
        assert_eq!(windows.len(), 2, "{windows:?}");
        for (start, end) in &windows {
            assert!(
                [429, 483].iter().any(|hit| (start..=end).contains(&hit)),
                "{windows:?}"
            );
        }
        for hit in [429, 483] {
            assert!(
                windows
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(&hit))
            );
        }
        // A window without hits (the opening window) narrows on its center.
        assert_eq!(recut_window(&[], (1, 121), 3), [(58, 64)]);
        // Hits outside the window are not this window's.
        assert_eq!(recut_window(&[10, 450], (396, 516), 3), [(447, 453)]);
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
        // cluster that holds the declaration and its uses; the page judged
        // the incidental rows too, so each gets its own window.
        let clustered = json!({"root":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":21},{"line":283},{"line":288},
                {"line":293},{"line":294},{"line":507}]
        }]}}]});
        let read = local_candidate_read(&clustered, 12_000).expect("clustered read");
        assert_eq!(
            read["query"]["ranges"],
            json!(["1-81", "228-348", "447-567"])
        );

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
            "mainGoal":"g","reasoning":"r","ecosystem":"npm","keywords":["x"],"page":2,"pageSize":20
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
