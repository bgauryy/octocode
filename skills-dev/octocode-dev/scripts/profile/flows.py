"""Named flows over the frozen corpus (harness.corpus). `gate` flows form perf_check.py: cheap and free of
any language server. The rest are the costlier flows of the 2026-11-05 audit's waste list, then the
2026-11-06 next-wave flows (one per tool that had none, the long-line flows, the live clasify flows).

`stub` flows run against stub.mjs (loopback GitHub, registry proxy, OSV): counts show a mechanism, never a
real answer's cost. `live` flows send public text (`public/vite`, MIT) to the configured classification
provider with the key from ~/.octocode/.env (harness.live_env); they spend provider tokens. In a query,
the string '{stub}' becomes the stub's base URL."""

CORPUS_REV = '6ea1ad094'
MID = 'crates/engine/src/lsp/client/lifecycle.rs'
TYPES = 'big/tool_types.rs'
ENGINE = 'crates/runtime/src/runtime/engine.rs'
OWNER = {'owner': 'octo', 'repo': 'demo'}
TERM = 'unwrap_or_default'
PATTERN = '$X.unwrap_or_default()'
AST = {'operation': 'match', 'path': 'crates', 'parser': 'rust'}
AST_IO = {'operation': 'match', 'path': 'crates/io', 'parser': 'rust'}


LONG = 'longline/bundle.min.js'
VITE = 'public/vite'
STATUS = 'Does this hit set or send an HTTP error status (400 or above) on the response? A 2xx or 304 status, or only reading a status, does not count.'


def flow(id, tool, query, phase='first', gate=False, stub=False, server=None, build=None, why='', live=None, storage='memory'):
    return {'id': id, 'tool': tool, 'query': query, 'phase': phase, 'gate': gate, 'stub': stub, 'server': server, 'build': build, 'why': why, 'live': live, 'storage': storage}


FLOWS = [
    # Cheap enough to run any time: scopes are small, repeats and later pages show lost reuse.
    flow('ls.content', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, gate=True),
    flow('ls.content.p3', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, 'page:3', gate=True, why='W6'),
    flow('ls.miss', 'localSearch', {'path': 'crates', 'matchString': ['zzqqnonexistent']}, gate=True),
    flow('st.files', 'structureSearch', {'path': 'crates', 'operation': 'files', 'view': 'paths'}, gate=True),
    flow('st.lineCount', 'structureSearch', {'path': 'crates/io', 'operation': 'files', 'entryType': 'file', 'sort': {'by': 'lineCount', 'order': 'desc'}}, gate=True, why='W10'),
    flow('lf.range', 'localFetch', {'path': MID, 'ranges': [{'startLine': 21, 'endLine': 60}]}, 'repeat', gate=True, why='W9'),
    flow('lf.symbols', 'localFetch', {'path': MID, 'view': 'symbols'}, 'repeat', gate=True, why='W4'),
    flow('lf.compact', 'localFetch', {'path': MID, 'view': 'compact'}, gate=True, why='minify'),
    flow('as.match', 'astSearch', {**AST_IO, 'rule': {'pattern': PATTERN}}, gate=True),
    flow('as.symbols.dir', 'astSearch', {'operation': 'symbols', 'path': 'crates/runtime/src/tools/local_search'}, 'repeat', gate=True, why='W4'),
    flow('gb.io', 'graphBuild', {'path': 'crates/io'}, gate=True),
    flow('gb.io.repeat', 'graphBuild', {'path': 'crates/io'}, 'repeat', gate=True, why='W1'),
    flow('gq.find.io', 'graphQuery', {'path': 'crates/io', 'operation': 'find', 'target': 'read_bounded'}, 'repeat', gate=True, build='crates/io', why='W7'),
    flow('gq.stats.io', 'graphQuery', {'path': 'crates/io', 'operation': 'stats'}, 'repeat', gate=True, build='crates/io', why='W7'),
    flow('gh.code', 'ghSearchCode', {**OWNER, 'keywords': ['greet']}, gate=True, stub=True, why='W3 mechanism'),
    flow('gh.pr.bulk', 'ghGetHistoryItem', {**OWNER, 'operation': 'pullRequest', 'number': 4, 'sections': ['files']}, gate=True, stub=True, why='W8'),
    # The audit's waste list, on the whole corpus. Too dear for perf_check.py.
    flow('gb.runtime.repeat', 'graphBuild', {'path': 'crates/runtime'}, 'repeat', why='W1'),
    flow('lsp.references', 'lspSearch', {'operation': 'references', 'path': ENGINE, 'symbolName': 'ToolRuntime', 'line': 72}, 'repeat', server='rust-analyzer', why='W2'),
    flow('lsp.implementation', 'lspSearch', {'operation': 'implementation', 'path': 'crates/runtime/src/execution/cancel.rs', 'symbolName': 'CancellationCheck', 'line': 4}, 'repeat', server='rust-analyzer', why='W2'),
    flow('lsp.definition', 'lspSearch', {'operation': 'definition', 'path': ENGINE, 'symbolName': 'ToolRuntime', 'line': 72}, 'repeat', server='rust-analyzer', why='W9'),
    flow('gh.code.repeat', 'ghSearchCode', {**OWNER, 'keywords': ['greet']}, 'repeat', stub=True, why='W3 mechanism'),
    flow('as.symbols.crates', 'astSearch', {'operation': 'symbols', 'path': 'crates'}, 'repeat', why='W4'),
    flow('lf.types.symbols', 'localFetch', {'path': TYPES, 'view': 'symbols'}, 'repeat', why='W4'),
    flow('lf.compact.p2', 'localFetch', {'path': TYPES, 'view': 'compact'}, 'page:2', why='minify reuse'),
    flow('lf.types.block', 'localFetch', {'path': TYPES, 'matchString': ['pub struct GhSearchRepo'], 'block': True}, 'repeat', why='W4'),
    flow('as.match.p3', 'astSearch', {**AST, 'rule': {'pattern': PATTERN}}, 'page:3', why='W5'),
    flow('ls.content.p5', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, 'page:5', why='W6'),
    flow('gq.find.runtime', 'graphQuery', {'path': 'crates/runtime', 'operation': 'find', 'target': 'ToolRuntime'}, 'repeat', build='crates/runtime', why='W7'),
    flow('gq.stats.runtime', 'graphQuery', {'path': 'crates/runtime', 'operation': 'stats'}, 'repeat', build='crates/runtime', why='W7'),
    flow('st.lineCount.crates', 'structureSearch', {'path': 'crates', 'operation': 'files', 'entryType': 'file', 'sort': {'by': 'lineCount', 'order': 'desc'}}, why='W10'),
    flow('as.symbols.filter', 'astSearch', {'operation': 'symbols', 'path': 'crates', 'symbolFilter': {'terms': ['ToolRuntime'], 'mode': 'exact'}}, why='W10'),
    # Next wave (NEXT-WAVE.md, WP0): tools with no flow before.
    flow('st.sort.size', 'structureSearch', {'path': 'crates', 'operation': 'files', 'entryType': 'file', 'sort': {'by': 'sizeBytes', 'order': 'desc'}}, why='WP2 SS-7'),
    flow('ls.testcode', 'localSearch', {'path': 'crates', 'matchString': [TERM], 'testCode': 'exclude'}, why='WP2 SHR-14'),
    flow('gh.repo.search', 'ghSearchRepo', {'operation': 'search', 'keywords': ['greeter']}, stub=True, why='WP4'),
    flow('gh.structure.tree', 'ghStructure', {**OWNER, 'operation': 'tree', 'ref': 'feature-4', 'include': ['pkg/**']}, stub=True, why='WP4: 351 paths'),
    flow('gh.structure.materialize', 'ghStructure', {**OWNER, 'operation': 'tree', 'materialize': True}, stub=True, storage='persistent', why='WP4 GST-4'),
    flow('gh.file.big', 'ghGetFileContent', {'owner': 'octo', 'repo': 'demo-docs', 'path': 'big.txt'}, stub=True, why='WP1: 20,000 lines'),
    flow('gh.history.search', 'ghSearchHistory', {**OWNER, 'operation': 'issue', 'keywords': ['Paged history']}, stub=True, why='WP4: 51 issues'),
    flow('gh.item.issue.comments', 'ghGetHistoryItem', {**OWNER, 'operation': 'issue', 'number': 7, 'sections': ['comments']}, stub=True, why='WP1 RR-D1: 365 comments'),
    flow('gh.item.compare', 'ghGetHistoryItem', {**OWNER, 'operation': 'compare', 'baseRef': 'prof-base', 'headRef': 'main'}, stub=True, why='WP1'),
    flow('artifact.resolve', 'artifactSearch', {'operation': 'resolve', 'ecosystem': 'npm', 'packageName': 'tiny-health', 'registryUrl': '{stub}/'}, stub=True, why='WP4'),
    flow('security.package', 'securityCheck', {'operation': 'package', 'ecosystem': 'npm', 'packageName': 'proof', 'version': '1.0.0'}, stub=True, why='WP4: 120 OSV advisories, 3 OSV pages'),
    flow('gh.clone', 'ghCloneRepo', {**OWNER}, stub=True, storage='persistent', why='WP4 GCR-2; git children not counted'),
    # Long lines: one 1 MiB line (bundle.min.js, about 19,000 needle_call hits), and one 12 MiB line. *.min.js is a
    # default exclude, so the searches pass defaultExcludes:false, the call the tool's own related[] offers.
    flow('ls.longline', 'localSearch', {'path': 'longline', 'matchString': ['needle_call'], 'defaultExcludes': False}, why='WP1/WP2 long lines'),
    flow('ls.longline.huge', 'localSearch', {'path': 'longline-huge', 'matchString': ['needle_call'], 'defaultExcludes': False}, why='WP2: line over 10 MiB'),
    flow('lf.longline', 'localFetch', {'path': LONG}, why='WP1 LF-10'),
    flow('as.longline', 'astSearch', {'operation': 'match', 'path': 'longline', 'parser': 'javascript', 'defaultExcludes': False, 'rule': {'pattern': 'needle_call($A, $B)'}}, why='WP2 long lines'),
    flow('gh.item.patch.long', 'ghGetHistoryItem', {**OWNER, 'operation': 'pullRequest', 'number': 5, 'sections': ['patches']}, stub=True, why='WP1: 1 MiB one-line patch'),
    # clasify, live with the configured key, public text only.
    flow('clasify.evaluate', 'clasify', {'operation': 'evaluate', 'source': {'tool': 'localFetch', 'query': {'path': f'{VITE}/middlewares__error.ts'}},
         'questions': {'status500': {'type': 'noul', 'instructions': 'Does this file set the response status to 500 when an error reaches the middleware? Logging the error alone does not count.'}}},
         live='classification', why='WP5'),
    flow('clasify.sections', 'clasify', {'operation': 'sections', 'resources': {'error': {'tool': 'localFetch', 'query': {'path': f'{VITE}/middlewares__error.ts'}},
         'static': {'tool': 'localFetch', 'query': {'path': f'{VITE}/middlewares__static.ts'}}},
         'questions': {'status': {'type': 'noul', 'instructions': STATUS}}}, live='classification', why='WP5'),
    flow('ls.classification', 'localSearch', {'path': VITE, 'matchString': ['statusCode'], 'contextLines': {'before': 2, 'after': 2},
         'classification': {'predicates': [STATUS]}}, live='classification', why='WP5 LS-12; RC T1 query'),
]
BY_ID = {flow_['id']: flow_ for flow_ in FLOWS}
