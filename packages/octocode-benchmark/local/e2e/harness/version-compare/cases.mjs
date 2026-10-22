// Cases for harness/version-compare.mjs: the SAME research step run against the
// local build and a published release. Every local contract tool has an entry.
//
// Each check runs one query per side: `loc` (local contract fields) and `pub`
// ({ tool, query } in the published release's own contract), or `{ na: reason }`
// when that side has no honest equivalent. Kinds:
//   happy    the primary operation; rows are counted and compared across sides
//   page     a small page; the named lead (`lead.loc` / `lead.pub` under data.next)
//            is replayed to the end and the union compared with `unpaged`
//   empty    a valid query with no evidence: must say so and guide the next step
//   invalid  a malformed query: must be rejected with a typed, actionable error
// `content: true` compares reconstructed text instead of row identities.
//
// Local tools read fixtures/tool-health (the process cwd). GitHub tools read
// live public data pinned to a tag where the tool takes a ref (expressjs/express
// v5.3.0); package tools read the live npm registry. Both sides call the same
// provider at the same time, so provider drift affects both equally.

const O = 'expressjs', R = 'express', REF = 'v5.3.0';

export const CASES = {
  localSearch: {
    pub: 'localSearch',
    checks: [
      { kind: 'happy', loc: { matchString: 'healthMarker', path: 'src' }, pub: { query: { operation: 'text', searchText: 'healthMarker', path: 'src' } } },
      { kind: 'page', loc: { matchString: 'healthMarker', path: 'src', pageSize: 1 }, pub: { query: { operation: 'text', searchText: 'healthMarker', path: 'src', pageSize: 1 } }, lead: { loc: 'nextPage', pub: 'nextPage' } },
      { kind: 'empty', loc: { matchString: 'zzzNoSuchToken', path: 'src' }, pub: { query: { operation: 'text', searchText: 'zzzNoSuchToken', path: 'src' } } },
      { kind: 'invalid', loc: { path: 'src' }, pub: { query: { operation: 'text', path: 'src' } } },
    ],
  },
  structureSearch: {
    pub: 'localSearch',
    checks: [
      { kind: 'happy', loc: { path: '.', operation: 'files' }, pub: { query: { operation: 'files', path: '.' } } },
      { kind: 'page', loc: { path: '.', operation: 'files', pageSize: 2 }, pub: { query: { operation: 'files', path: '.', pageSize: 2 } }, lead: { loc: 'nextPage', pub: 'nextPage' } },
      { kind: 'empty', loc: { path: '.', operation: 'files', include: ['zzzNoSuchName*'] }, pub: { query: { operation: 'files', path: '.', names: ['zzzNoSuchName*'] } } },
      { kind: 'invalid', loc: { operation: 'files' }, pub: { query: { operation: 'files' } } },
    ],
  },
  astSearch: {
    pub: 'localSearch',
    checks: [
      { kind: 'happy', loc: { path: 'src', operation: 'match', pattern: 'greet($A)', language: 'typescript' }, pub: { query: { operation: 'structural', path: 'src', pattern: 'greet($A)', langType: 'typescript' } } },
      { kind: 'page', loc: { path: 'src', operation: 'symbols', pageSize: 1 }, pub: { na: 'the published release has no declaration listing over a directory (symbols); lspGetSemantics documentSymbols is one file per call' }, lead: { loc: 'nextPage' } },
      { kind: 'empty', loc: { path: 'src', operation: 'match', pattern: 'zzzNoSuchCall($A)', language: 'typescript' }, pub: { query: { operation: 'structural', path: 'src', pattern: 'zzzNoSuchCall($A)', langType: 'typescript' } } },
      { kind: 'invalid', loc: { path: 'src', operation: 'match' }, pub: { query: { operation: 'structural', path: 'src' } } },
    ],
  },
  localFetch: {
    pub: 'localGetFileContent',
    checks: [
      { kind: 'happy', content: true, loc: { path: 'src/greeter.ts', fullContent: true }, pub: { query: { path: 'src/greeter.ts', fullContent: true, minify: 'none' } } },
      { kind: 'page', content: true, truth: { file: 'src/greeter.ts' }, loc: { path: 'src/greeter.ts', unit: 'lines', offset: 0, length: 5 }, pub: { query: { path: 'src/greeter.ts', fullContent: true, minify: 'none', charOffset: 0, charLength: 120 } }, lead: { loc: 'continueWindow', pub: 'nextChunk' } },
      { kind: 'match', loc: { path: 'src/greeter.ts', matchString: 'healthMarker' }, pub: { query: { path: 'src/greeter.ts', matchString: 'healthMarker' } } },
      { kind: 'empty', loc: { path: 'src/greeter.ts', matchString: 'zzzNoSuchText' }, pub: { query: { path: 'src/greeter.ts', matchString: 'zzzNoSuchText' } } },
      { kind: 'invalid', loc: { path: 'src/greeter.ts', ranges: 'not-an-array' }, pub: { query: { path: 'src/greeter.ts', startLine: 'x', endLine: 2 } } },
    ],
  },
  lspSearch: {
    pub: 'lspGetSemantics',
    checks: [
      { kind: 'happy', loc: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references' }, pub: { query: { uri: 'src/greeter.ts', symbolName: 'greet', lineHint: 6, type: 'references' } } },
      { kind: 'page', loc: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references', pageSize: 1 }, pub: { query: { uri: 'src/greeter.ts', symbolName: 'greet', lineHint: 6, type: 'references', pageSize: 1 } }, lead: { loc: 'nextPage', pub: 'nextPage' } },
      { kind: 'definition', loc: { path: 'src/app.ts', symbolName: 'greet', line: 1, operation: 'definition' }, pub: { query: { uri: 'src/app.ts', symbolName: 'greet', lineHint: 1, type: 'definition' } } },
      { kind: 'empty', loc: { operation: 'workspaceSymbol', workspaceRoot: '.', symbolName: 'zzzNoSuchSymbol' }, pub: { query: { type: 'workspaceSymbol', workspaceRoot: '.', symbolName: 'zzzNoSuchSymbol' } } },
      { kind: 'invalid', loc: { path: 'src/greeter.ts', symbolName: 'greet' }, pub: { query: { uri: 'src/greeter.ts', symbolName: 'greet' } } },
    ],
  },
  ghSearchCode: {
    pub: 'ghSearch',
    checks: [
      { kind: 'happy', loc: { owner: O, repo: R, keywords: ['redirect'], path: 'lib' }, pub: { query: { operation: 'code', owner: O, repo: R, keywords: ['redirect'], path: 'lib' } } },
      { kind: 'page', loc: { owner: O, repo: R, keywords: ['require'], path: 'lib', pageSize: 2 }, pub: { query: { operation: 'code', owner: O, repo: R, keywords: ['require'], path: 'lib', pageSize: 2 } }, lead: { loc: 'nextPage', pub: 'nextPage' } },
      { kind: 'empty', loc: { owner: O, repo: R, keywords: ['zzznosuchtokenqq'] }, pub: { query: { operation: 'code', owner: O, repo: R, keywords: ['zzznosuchtokenqq'] } } },
      { kind: 'invalid', loc: { repo: R, keywords: ['redirect'] }, pub: { query: { operation: 'code', keywords: ['redirect'], pageSize: 1000 } } },
    ],
  },
  ghSearchRepo: {
    pub: 'ghSearch',
    checks: [
      { kind: 'happy', loc: { keywords: ['express'], owner: O }, pub: { query: { operation: 'repositories', keywords: ['express'], owner: O } } },
      { kind: 'page', loc: { keywords: ['express'], owner: O, pageSize: 3 }, pub: { query: { operation: 'repositories', keywords: ['express'], owner: O, pageSize: 3 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, maxPages: 12 },
      { kind: 'empty', loc: { keywords: ['zzznosuchrepoqq'], owner: O }, pub: { query: { operation: 'repositories', keywords: ['zzznosuchrepoqq'], owner: O } } },
      { kind: 'invalid', loc: { keywords: ['express'], pageSize: 1000 }, pub: { query: { operation: 'repositories', keywords: ['express'], pageSize: 1000 } } },
    ],
  },
  ghStructure: {
    pub: 'ghSearch',
    checks: [
      { kind: 'happy', loc: { owner: O, repo: R, ref: REF, maxDepth: 2 }, pub: { query: { operation: 'tree', owner: O, repo: R, branch: REF, maxDepth: 2 } } },
      { kind: 'page', loc: { owner: O, repo: R, ref: REF, maxDepth: 3, pageSize: 20 }, pub: { query: { operation: 'tree', owner: O, repo: R, branch: REF, maxDepth: 3, pageSize: 20 } }, lead: { loc: 'nextPage', pub: 'nextPage' } },
      { kind: 'empty', loc: { owner: O, repo: R, ref: REF, include: ['**/zzzNoSuchName*'] }, pub: { query: { operation: 'tree', owner: O, repo: R, branch: REF, include: ['**/zzzNoSuchName*'] } } },
      { kind: 'invalid', loc: { owner: O }, pub: { query: { operation: 'tree', owner: O } } },
    ],
  },
  ghGetFileContent: {
    pub: 'ghGetFileContent',
    checks: [
      { kind: 'happy', content: true, loc: { owner: O, repo: R, ref: REF, path: 'lib/application.js', fullContent: true }, pub: { query: { owner: O, repo: R, branch: REF, path: 'lib/application.js', fullContent: true, minify: 'none' } } },
      { kind: 'page', content: true, truth: { owner: O, repo: R, ref: REF, path: 'History.md' }, loc: { owner: O, repo: R, ref: REF, path: 'History.md', fullContent: true }, pub: { query: { owner: O, repo: R, branch: REF, path: 'History.md', fullContent: true, minify: 'none' } }, lead: { loc: 'continueWindow', pub: 'nextChunk' }, maxPages: 40 },
      { kind: 'match', loc: { owner: O, repo: R, ref: REF, path: 'lib/response.js', matchString: 'res.redirect = function' }, pub: { query: { owner: O, repo: R, branch: REF, path: 'lib/response.js', matchString: 'res.redirect = function' } } },
      { kind: 'empty', loc: { owner: O, repo: R, ref: REF, path: 'lib/response.js', matchString: 'zzzNoSuchText' }, pub: { query: { owner: O, repo: R, branch: REF, path: 'lib/response.js', matchString: 'zzzNoSuchText' } } },
      { kind: 'invalid', loc: { owner: O, repo: R }, pub: { query: { owner: O, repo: R } } },
    ],
  },
  ghSearchHistory: {
    pub: 'ghSearchHistory',
    checks: [
      { kind: 'happy', loc: { operation: 'commit', owner: O, repo: R, path: 'lib/router', until: '2025-01-01' }, pub: { query: { operation: 'commits', owner: O, repo: R, path: 'lib/router', until: '2025-01-01' } } },
      { kind: 'page', loc: { operation: 'pullRequest', owner: O, repo: R, keywords: ['redirect'], state: 'merged', pageSize: 5 }, pub: { query: { operation: 'pullRequests', owner: O, repo: R, keywords: ['redirect'], state: 'closed', 'merged-at': '>2000-01-01', pageSize: 5 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, maxPages: 5 },
      { kind: 'empty', loc: { operation: 'pullRequest', owner: O, repo: R, keywords: ['zzznosuchqq'] }, pub: { query: { operation: 'pullRequests', owner: O, repo: R, keywords: ['zzznosuchqq'] } } },
      { kind: 'invalid', loc: { operation: 'commit', owner: O }, pub: { query: { operation: 'commits', owner: O } } },
    ],
  },
  ghGetHistoryItem: {
    pub: 'ghGetHistoryItem',
    checks: [
      { kind: 'happy', name: 'pr-summary', loc: { operation: 'pullRequest', owner: O, repo: R, number: 5595 }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 5595 } } },
      { kind: 'page', name: 'pr-files', loc: { operation: 'pullRequest', owner: O, repo: R, number: 6236, sections: ['files'], pageSize: 10 }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 6236, content: { changedFiles: true }, pageSize: 10 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, unpaged: { loc: { operation: 'pullRequest', owner: O, repo: R, number: 6236, sections: ['files'], pageSize: 100 }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 6236, content: { changedFiles: true }, pageSize: 100 } } }, expectRows: 52 },
      { kind: 'page', name: 'pr-comments', loc: { operation: 'pullRequest', owner: O, repo: R, number: 5595, sections: ['comments'], pageSize: 10 }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 5595, content: { comments: { discussion: true } }, pageSize: 10 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, expectRows: 31 },
      { kind: 'page', name: 'pr-patches', content: true, loc: { operation: 'pullRequest', owner: O, repo: R, number: 6236, sections: ['patches'], include: ['lib/response.js'] }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 6236, content: { patches: { mode: 'selected', files: ['lib/response.js'] } } } }, lead: { loc: 'continuePatch', pub: 'nextChunk' } },
      { kind: 'page', name: 'issue-comments', loc: { operation: 'issue', owner: O, repo: R, number: 6968, sections: ['comments'], pageSize: 5 }, pub: { query: { operation: 'issue', owner: O, repo: R, number: 6968, content: { comments: { discussion: true } }, pageSize: 5 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, expectRows: 17 },
      { kind: 'page', name: 'commit-files', loc: { operation: 'commit', owner: O, repo: R, ref: '41113599afb01040436dba993f06cacbdb8f8e13', sections: ['files'], pageSize: 10 }, pub: { query: { operation: 'commit', owner: O, repo: R, ref: '41113599afb01040436dba993f06cacbdb8f8e13', includeDiff: true, pageSize: 10 } }, lead: { loc: 'nextPage', pub: 'nextFilePage' }, expectRows: 52 },
      { kind: 'page', name: 'compare-commits', loc: { operation: 'compare', owner: O, repo: R, base: 'v5.2.1', head: 'v5.3.0', sections: ['commits'], pageSize: 20 }, pub: { query: { operation: 'compare', owner: O, repo: R, base: 'v5.2.1', head: 'v5.3.0', pageSize: 20 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, expectRows: 76 },
      { kind: 'empty', loc: { operation: 'pullRequest', owner: O, repo: R, number: 6236, sections: ['patches'], matchString: 'zzzNoSuchText' }, pub: { query: { operation: 'pullRequest', owner: O, repo: R, number: 6236, matchString: 'zzzNoSuchText', content: { patches: { mode: 'all' } } } } },
      { kind: 'invalid', loc: { operation: 'pullRequest', owner: O, repo: R }, pub: { query: { operation: 'pullRequest', owner: O, repo: R } } },
    ],
  },
  artifactSearch: {
    pub: 'npmSearch',
    checks: [
      { kind: 'happy', loc: { ecosystem: 'npm', packageName: 'express' }, pub: { query: { packageName: 'express' } } },
      { kind: 'page', loc: { ecosystem: 'npm', keywords: ['express', 'middleware', 'cors'], pageSize: 2 }, pub: { query: { keywords: ['express', 'middleware', 'cors'], pageSize: 2 } }, lead: { loc: 'nextPage', pub: 'nextPage' }, maxPages: 4 },
      { kind: 'empty', loc: { ecosystem: 'npm', keywords: ['zzznosuchpkgqqxx'] }, pub: { query: { keywords: ['zzznosuchpkgqqxx'] } } },
      { kind: 'invalid', loc: { ecosystem: 'npm' }, pub: { query: {} } },
    ],
  },
  ghCloneRepo: {
    pub: 'ghCloneRepo',
    checks: [
      { kind: 'happy', loc: { owner: 'octocat', repo: 'Hello-World' }, pub: { query: { owner: 'octocat', repo: 'Hello-World' } } },
      { kind: 'invalid', loc: { owner: 'octocat' }, pub: { query: { owner: 'octocat' } } },
    ],
  },
  clasify: {
    pub: null,
    naReason: 'the published release has no classifier tool; local clasify is rated by the held-out workflow evaluation (needs a provider key)',
    checks: [
      // state and source are mutually exclusive forms.
      { kind: 'invalid', loc: { state: 'alpha', source: { url: 'https://example.org/' }, questions: { q: { type: 'noul', instructions: 'Is alpha stated?' } } } },
    ],
  },
};
