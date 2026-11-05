"""Named flows over the frozen corpus (harness.corpus). `gate` flows form perf_check.py: cheap and free of
any language server. The rest are the costlier flows of the 2026-11-05 audit's waste list."""

CORPUS_REV = '6ea1ad094'
MID = 'crates/engine/src/lsp/client/lifecycle.rs'
TYPES = 'big/tool_types.rs'
ENGINE = 'crates/runtime/src/runtime/engine.rs'
OWNER = {'owner': 'octo', 'repo': 'demo'}
TERM = 'unwrap_or_default'
PATTERN = '$X.unwrap_or_default()'
AST = {'operation': 'match', 'path': 'crates', 'parser': 'rust'}
AST_IO = {'operation': 'match', 'path': 'crates/io', 'parser': 'rust'}


def flow(id, tool, query, phase='first', gate=False, stub=False, server=None, build=None, why=''):
    return {'id': id, 'tool': tool, 'query': query, 'phase': phase, 'gate': gate, 'stub': stub, 'server': server, 'build': build, 'why': why}


FLOWS = [
    # Cheap enough to run any time: scopes are small, repeats and later pages show lost reuse.
    flow('ls.content', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, gate=True),
    flow('ls.content.p3', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, 'page:3', gate=True, why='W6'),
    flow('ls.miss', 'localSearch', {'path': 'crates', 'matchString': ['zzqqnonexistent']}, gate=True),
    flow('st.files', 'structureSearch', {'path': 'crates', 'operation': 'files', 'view': 'paths'}, gate=True),
    flow('st.lineCount', 'structureSearch', {'path': 'crates/io', 'operation': 'files', 'entryType': 'file', 'sort': {'by': 'lineCount', 'order': 'desc'}}, gate=True, why='W10'),
    flow('lf.range', 'localFetch', {'path': MID, 'ranges': [{'startLine': 21, 'endLine': 60}]}, 'repeat', gate=True, why='W9'),
    flow('lf.symbols', 'localFetch', {'path': MID, 'view': 'symbols'}, 'repeat', gate=True, why='W4'),
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
    flow('lf.types.block', 'localFetch', {'path': TYPES, 'matchString': ['pub struct GhSearchRepo'], 'block': True}, 'repeat', why='W4'),
    flow('as.match.p3', 'astSearch', {**AST, 'rule': {'pattern': PATTERN}}, 'page:3', why='W5'),
    flow('ls.content.p5', 'localSearch', {'path': 'crates', 'matchString': [TERM]}, 'page:5', why='W6'),
    flow('gq.find.runtime', 'graphQuery', {'path': 'crates/runtime', 'operation': 'find', 'target': 'ToolRuntime'}, 'repeat', build='crates/runtime', why='W7'),
    flow('gq.stats.runtime', 'graphQuery', {'path': 'crates/runtime', 'operation': 'stats'}, 'repeat', build='crates/runtime', why='W7'),
    flow('st.lineCount.crates', 'structureSearch', {'path': 'crates', 'operation': 'files', 'entryType': 'file', 'sort': {'by': 'lineCount', 'order': 'desc'}}, why='W10'),
    flow('as.symbols.filter', 'astSearch', {'operation': 'symbols', 'path': 'crates', 'symbolFilter': {'terms': ['ToolRuntime'], 'mode': 'exact'}}, why='W10'),
]
BY_ID = {flow_['id']: flow_ for flow_ in FLOWS}
