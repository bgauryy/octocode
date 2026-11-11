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
    "B4 echo | ghStructure | resolvedRef <- request ref",
    "B4 echo | graphBuild | root <- request path",
    "B4 echo | graphQuery | from <- request target",
    "B4 echo | graphQuery | graph.root <- request path",
    "B4 echo | graphQuery | rows[].file <- request target",
    "B4 echo | graphQuery | rows[].kind <- request kind",
    "B4 echo | graphQuery | target.id <- request target",
    "B4 echo | securityCheck | advisories[].id <- request id",
    "B5 stated once | graphQuery | mostCalledSymbols[].kind",
    "B5 stated once | graphQuery | mostCalledSymbols[].symbolKind",
    "B5 stated once | graphQuery | mostImportedFiles[].kind",
    "B5 stated once | graphQuery | mostImportedFiles[].language",
    "B5 stated once | graphQuery | rows[].kind",
    "B5 stated once | graphQuery | rows[].language",
    "B5 stated once | graphQuery | rows[].symbolKind",
];

/// Wave 3 (location and symbol): graph rows carry `path`, `line` and
/// `symbolName`, so the per-row `verify` lead goes.
const WAVE_3: &[&str] = &[
    "B3 lead | graphQuery | outside related: rows[].verify[] -> lspSearch",
    "B3 lead | graphQuery | outside related: verify[] -> lspSearch",
];

/// Wave 4 (listing, paging, continuation, status): one pagination type, every
/// continuation under `next`, `outOfRange` for a read past the end, one
/// timestamp and number format, one empty-row rule.
const WAVE_4: &[&str] = &[
    "B1 state | localFetch | - outOfRange: error",
    "B2 continuation | clasify | outside next: resources.long.read",
    "B2 pagination | astSearch | pagination without unit",
    "B2 pagination | astSearch | pagination.page",
    "B2 pagination | astSearch | pagination.totalItems",
    "B2 pagination | astSearch | pagination.totalPages",
    "B2 pagination | ghSearchHistory | pagination without unit",
    "B2 pagination | ghSearchHistory | pagination.page",
    "B2 pagination | ghSearchHistory | pagination.totalItems",
    "B2 pagination | ghSearchHistory | pagination.totalPages",
    "B2 pagination | graphQuery | pagination without unit",
    "B2 pagination | graphQuery | pagination.page",
    "B2 pagination | graphQuery | pagination.totalItems",
    "B2 pagination | localSearch | pagination without unit",
    "B2 pagination | localSearch | pagination.page",
    "B2 pagination | localSearch | pagination.totalItems",
    "B2 pagination | localSearch | pagination.totalPages",
    "B2 pagination | lspSearch | pagination without unit",
    "B2 pagination | lspSearch | pagination.page",
    "B2 pagination | lspSearch | pagination.totalItems",
    "B2 pagination | lspSearch | pagination.totalPages",
    "B2 pagination | structureSearch | pagination without unit",
    "B2 pagination | structureSearch | pagination.page",
    "B2 pagination | structureSearch | pagination.totalItems",
    "B2 pagination | structureSearch | pagination.totalPages",
    "B6 format | artifactSearch | timestamp artifacts[].publishedAt: date only",
    "B6 format | ghGetHistoryItem | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | ghSearchHistory | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | ghSearchRepo | timestamp repositories[].createdAt: date only",
    "B6 format | ghSearchRepo | timestamp repositories[].pushedAt: date only",
    "B6 format | ghSearchRepo | timestamp repositories[].updatedAt: date only",
    "B6 format | localFetch | timestamp modified: fraction of a second",
    "B6 format | localFetch | timestamp responsePagination.expiresAt: fraction of a second",
    "B6 format | securityCheck | number advisories[].epss.percentage: more than 3 fraction digits",
    "B6 format | securityCheck | number advisories[].epss.percentile: more than 3 fraction digits",
    "B6 format | securityCheck | timestamp advisories[].modified: fraction of a second",
    "B6 format | securityCheck | timestamp responsePagination.expiresAt: fraction of a second",
    "B7 empty row | localSearch | no collection key",
    "B7 empty row | localSearch | zero counter matchCount",
    "B7 empty row | localSearch | zero counter matchedFiles",
    "B7 empty row | localSearch | zero counter matchedLineCount",
];

/// Rule H candidates the RFC's hoisting table does not rank: uniform on these
/// fixture pages (every fixture symbol is an exported function; every recorded
/// pull request is open). The wave that extends `STATED_ONCE_GROUPS` decides
/// each.
const UNRANKED: &[&str] = &[
    "B5 stated once | astSearch | files[].symbols[].exported",
    "B5 stated once | astSearch | files[].symbols[].kind",
    "B5 stated once | ghSearchHistory | pullRequests[].state",
    "B5 stated once | graphQuery | mostCalledSymbols[].exported",
    "B5 stated once | graphQuery | mostCalledSymbols[].fanIn",
    "B5 stated once | graphQuery | mostImportedFiles[].fanIn",
    "B5 stated once | graphQuery | rows[].exported",
    "B5 stated once | graphQuery | rows[].fanIn",
    "B5 stated once | graphQuery | rows[].match",
];

/// Equal to a request value by design or by the fixture, and kept: a listing
/// group's `dir` is the one split path form; the recorded repository's owner
/// wrote its commits; the language-server double answers in the file asked
/// about. An entry leaves only when its fixture or the listing shape changes.
const KEPT: &[&str] = &[
    "B4 echo | structureSearch | files[].dir <- request path",
    "B4 echo | ghGetHistoryItem | author.login <- request owner",
    "B4 echo | ghSearchHistory | commits[].author <- request owner",
    "B4 echo | lspSearch | files[].path <- request path",
];

pub fn all() -> impl Iterator<Item = &'static str> {
    [WAVE_2, WAVE_3, WAVE_4, UNRANKED, KEPT]
        .into_iter()
        .flatten()
        .copied()
}
