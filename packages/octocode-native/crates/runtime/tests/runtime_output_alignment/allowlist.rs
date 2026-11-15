//! Today's deviations from the protocol, as `check | tool | detail`. Each list
//! only shrinks: the wave named above a list removes it, and an entry that no
//! longer deviates fails the test until it is deleted. A new entry is never
//! added to make a change pass.

/// Wave 2 (identity and revision): Rule E class (a), a response field that
/// restates the request, and the graph row facts every row of a page shares
/// (hoisting table rows 3, 11 and 14).
const WAVE_2: &[&str] = &[
    "B4 echo | artifactSearch | artifacts[].packageName <- request packageName",
    "B4 echo | artifactSearch | artifacts[].version <- request version",
    "B4 echo | ghGetHistoryItem | commitSha <- request ref",
    "B4 echo | ghGetHistoryItem | pullRequests[].number <- request number",
    "B4 echo | ghSearchRepo | repositories[].owner <- request owner",
    "B4 echo | ghSearchRepo | repositories[].repo <- request repo",
    "B4 echo | graphBuild | root <- request path",
    "B4 echo | graphQuery | graph.root <- request path",
    "B4 echo | graphQuery | rows[].kind <- request kind",
    "B4 echo | securityCheck | advisories[].id <- request id",
];

/// Wave 4 (listing, paging, continuation, status): one pagination type, every
/// continuation under `next`, `outOfRange` for a read past the end, one
/// timestamp and number format, one empty-row rule.
const WAVE_4: &[&str] = &[
    "B1 state | localFetch | - outOfRange: error",
    "B2 continuation | clasify | outside next: resources.long.read",
    "B6 format | artifactSearch | timestamp artifacts[].publishedAt: date only",
    "B6 format | ghGetHistoryItem | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | ghSearchHistory | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | ghSearchRepo | timestamp repositories[].pushedAt: date only",
    "B6 format | localFetch | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | securityCheck | number advisories[].epss.percentage: more than 3 fraction digits",
    "B6 format | securityCheck | number advisories[].epss.percentile: more than 3 fraction digits",
    "B6 format | securityCheck | timestamp advisories[].modifiedAt: fraction of a second",
    "B6 format | securityCheck | timestamp responsePagination.expiresAt: fraction of a second",
];

/// Rule H candidates the RFC's hoisting table does not rank: uniform on these
/// fixture pages (every fixture symbol is an exported function; every recorded
/// pull request is open; every fixture edge has one source occurrence). The wave that extends `STATED_ONCE_GROUPS` decides
/// each.
const UNRANKED: &[&str] = &[
    "B5 stated once | astSearch | files[].symbols[].exported",
    "B5 stated once | astSearch | files[].symbols[].kind",
    "B5 stated once | ghSearchHistory | pullRequests[].state",
    "B5 stated once | graphQuery | rows[].sites",
];

/// Equal to a request value by design or by the fixture, and kept: a listing
/// group's `dir` is the one split path form; the recorded repository's owner
/// wrote its commits; the language-server double answers in the file asked
/// about, and a references answer lists the declaration at the anchor line
/// (`displayRange.startLine` before, which this check did not read). An entry
/// leaves only when its fixture or the listing shape changes.
const KEPT: &[&str] = &[
    "B4 echo | structureSearch | files[].dir <- request path",
    "B4 echo | ghGetHistoryItem | author.login <- request owner",
    "B4 echo | lspSearch | files[].path <- request path",
    "B4 echo | lspSearch | files[].matches[].line <- request line",
];

pub fn all() -> impl Iterator<Item = &'static str> {
    [WAVE_2, WAVE_4, UNRANKED, KEPT]
        .into_iter()
        .flatten()
        .copied()
}
