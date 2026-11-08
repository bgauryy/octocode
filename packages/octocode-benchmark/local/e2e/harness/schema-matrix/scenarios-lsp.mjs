// Language-server states from a scripted protocol server: its answers are the fixture,
// so each row shows how native reports what the server did, not what a compiler knows.
import { contract } from '../contract.mjs';

const range = (line, character, length = 1) => ({ start: { line, character }, end: { line, character: character + length } });
const loc = (uri, line, character, length = 1) => ({ uri, range: range(line, character, length) });
const item = (base, file, line, name) => ({ name, kind: 12, uri: `${base}/${file}`, range: range(line, 0, 20), selectionRange: range(line, 0, 1) });
const REPLIES = { error: { code: -32603, message: 'scripted failure' } };

const scene = (id, { files, query, script, codes, reasons, languages, command, env, channels, allow }) => ({
  id: `lsp.${id}`, tool: 'lspSearch', files, query: { workspaceRoot: '$DIR', ...query }, codes, reasons, channels, env, allow,
  lsp: { script, languages, command },
});
const TS_PROJECT = { 'tsconfig.json': '{}\n' };
const CARGO = { 'Cargo.toml': '[package]\nname = "m"\nversion = "0.0.0"\nedition = "2021"\n\n[workspace]\n' };
const TARGET = { path: '$DIR/src/lib.rs', symbolName: 'target', line: 1, operation: 'references' };
const answerLibOnly = base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/src/lib.rs`, 0, 7, 6)] }] });
const fanOut = (base, count, prefix) => Array.from({ length: count }, (_, index) => ({ from: item(base, 'src/lib.rs', 1 + index, `${prefix}${index}`), fromRanges: [range(0, 0)] }));
const CALL_ROOT = { path: '$DIR/src/lib.rs', symbolName: 'root', line: 1, operation: 'callers', depth: 2 };
const callRoot = base => ({ method: 'textDocument/prepareCallHierarchy', result: [item(base, 'src/lib.rs', 0, 'root')] });
const ROOTS = { ...CARGO, 'src/lib.rs': `pub fn root() {}\n${Array.from({ length: 300 }, (_, index) => `pub fn n${index}() {}`).join('\n')}\n` };

// The default context is only the symbol line. Select the body explicitly,
// and make one body line exceed the native row window to provoke its read.
const DECLARATION = `export function f() {\n  body(); // ${'x'.repeat(contract.limits.defaultResponseChars)}\n${'  body();\n'.repeat(69)}}\nf();\n`;
const DECLARATION_END = DECLARATION.split('\n').findIndex(line => line === '}');

// Every graph node occupies its own real declaration; names alone do not
// distinguish nodes because native keys them by URI and selection range.
function cappedNodeScene() {
  const parents = Array.from({ length: 50 }, (_, index) => `n${index}`);
  const children = name => Array.from({ length: 45 }, (_, index) => `${name}child${index}`);
  const names = ['root', ...parents, ...parents.flatMap(children)];
  const lines = new Map(names.map((name, index) => [name, index]));
  const source = `${names.map(name => `pub fn ${name}() {}`).join('\n')}\n`;
  const node = (base, name) => ({
    ...item(base, 'src/lib.rs', lines.get(name), name),
    range: range(lines.get(name), 0, `pub fn ${name}() {}`.length),
    selectionRange: range(lines.get(name), 7, name.length),
  });
  const incoming = (base, parent, callers) => ({
    method: 'callHierarchy/incomingCalls', item: parent,
    result: callers.map(name => ({ from: node(base, name), fromRanges: [range(lines.get(parent), 7, parent.length)] })),
  });
  return {
    files: { ...CARGO, 'src/lib.rs': source },
    script: base => ({ answers: [
      ...names.map(name => ({ method: 'textDocument/prepareCallHierarchy', word: name, result: [node(base, name)] })),
      incoming(base, 'root', parents),
      ...parents.map(name => incoming(base, name, children(name))),
    ] }),
  };
}

const REASONS = [
  scene('aliasRecoveryFailed', {
    files: { ...TS_PROJECT, 'a.ts': 'export function target() {}\n', 'b.ts': "import { target as t } from './a';\nt();\n" },
    query: { path: '$DIR/a.ts', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['aliasRecoveryFailed'],
    script: base => ({ answers: [{ method: 'textDocument/references', uri: '/b.ts', error: REPLIES.error }, { method: 'textDocument/references', result: [loc(`${base}/a.ts`, 0, 16, 6), loc(`${base}/b.ts`, 0, 9, 6)] }] }),
  }),
  scene('cfgNegatedFeatures', {
    files: { ...CARGO, 'src/lib.rs': 'pub fn target() {}\n', 'src/fallback.rs': '#[cfg(not(feature = "rt"))]\nfn t() { crate::target(); }\n' },
    query: { ...TARGET, rustContext: { features: 'all' } }, reasons: ['cfgNegatedFeatures'], script: answerLibOnly,
  }),
  scene('cfgGatedFiles', {
    files: { ...CARGO, 'src/lib.rs': 'pub fn target() {}\n', 'src/gated.rs': '#[cfg(feature = "x")]\nfn t() { crate::target(); }\n' },
    query: TARGET, reasons: ['cfgGatedFiles'], script: answerLibOnly,
  }),
  scene('hierarchyParentUnanchored', {
    files: ROOTS, query: CALL_ROOT, reasons: ['hierarchyParentUnanchored'], allow: ['lead.execution'],
    script: base => ({ answers: [callRoot(base),
      { method: 'callHierarchy/incomingCalls', item: 'root', result: [{ from: item(base, 'src/lib.rs', 1, 'good'), fromRanges: [range(1, 0)] }, { from: { ...item(base, 'src/lib.rs', 2, ''), name: '' }, fromRanges: [range(2, 0)] }] },
      { method: 'callHierarchy/incomingCalls', result: fanOut(base, 52, 'x') }] }),
  }),
  scene('inferredProject', {
    files: { 'a.ts': 'export interface Shape {}\n', 'b.ts': "import type { Shape } from './a';\nexport const s: Shape = {};\n" },
    query: { path: '$DIR/a.ts', symbolName: 'Shape', line: 1, operation: 'implementation' }, reasons: ['inferredProject'], script: () => ({ answers: [{ method: 'textDocument/implementation', result: [] }] }),
  }),
  scene('sharedModuleFile', {
    files: { ...CARGO, 'src/lib.rs': '\n', 'benches/support/frozen.rs': 'pub fn frozen_graph() {}\n', 'benches/graph.rs': '#[path = "support/frozen.rs"]\nmod frozen;\nfn main() { frozen::frozen_graph(); }\n', 'tests/graph.rs': '#[path = "../benches/support/frozen.rs"]\nmod frozen;\nfn main() { frozen::frozen_graph(); }\n' },
    query: { path: '$DIR/benches/support/frozen.rs', symbolName: 'frozen_graph', line: 1, operation: 'references' }, reasons: ['sharedModuleFile'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/benches/support/frozen.rs`, 0, 7, 12), loc(`${base}/tests/graph.rs`, 2, 21, 12)] }] }),
  }),
  scene('anchorLineError', {
    files: { ...TS_PROJECT, 'a.ts': 'const x = 1;\n' }, query: { path: '$DIR/a.ts', symbolName: 'x', line: 1, operation: 'definition' }, reasons: ['anchorLineError'],
    script: () => ({ answers: [{ method: 'textDocument/diagnostic', result: { kind: 'full', items: [{ range: range(0, 0), severity: 1, message: 'scripted error' }] } }] }),
  }),
  scene('buildScriptsOff', {
    files: { ...CARGO, 'build.rs': 'fn main() {}\n', 'src/lib.rs': 'include!(concat!(env!("OUT_DIR"), "/gen.rs"));\npub fn x() {}\n' },
    query: { path: '$DIR/src/lib.rs', symbolName: 'x', line: 2, operation: 'definition' }, reasons: ['buildScriptsOff'], script: () => ({}),
  }),
  scene('includeOutsidePackage', {
    files: { ...CARGO, 'pkg/Cargo.toml': CARGO['Cargo.toml'], 'pkg/src/lib.rs': 'include!("../../outside/x.rs");\npub fn x() {}\n', 'outside/x.rs': 'pub fn y() {}\n' },
    query: { path: '$DIR/pkg/src/lib.rs', symbolName: 'x', line: 2, operation: 'definition' }, reasons: ['includeOutsidePackage'], script: () => ({}),
  }),
  scene('callHierarchyExpansionFailed', {
    files: ROOTS, query: CALL_ROOT, reasons: ['callHierarchyExpansionFailed'],
    script: base => ({ answers: [callRoot(base), { method: 'callHierarchy/incomingCalls', item: 'root', result: fanOut(base, 1, 'a') }, { method: 'callHierarchy/incomingCalls', error: REPLIES.error }] }),
  }),
  scene('typeHierarchyExpansionFailed', {
    files: { ...TS_PROJECT, 'model.ts': 'export interface Shape {}\nexport class Square implements Shape {}\n' },
    query: { path: '$DIR/model.ts', symbolName: 'Shape', line: 1, operation: 'subtypes', depth: 2 }, reasons: ['typeHierarchyExpansionFailed'],
    script: base => ({ answers: [{ method: 'textDocument/prepareTypeHierarchy', result: [{ ...item(base, 'model.ts', 0, 'Shape'), kind: 11 }] }, { method: 'typeHierarchy/subtypes', item: 'Shape', result: [{ ...item(base, 'model.ts', 1, 'Square'), kind: 5 }] }, { method: 'typeHierarchy/subtypes', error: REPLIES.error }] }),
  }),
  scene('hierarchyFanOutLimit', {
    files: ROOTS, query: CALL_ROOT, reasons: ['hierarchyFanOutLimit'], allow: ['lead.execution'],
    script: base => ({ answers: [callRoot(base), { method: 'callHierarchy/incomingCalls', item: 'root', result: fanOut(base, 1, 'hub') }, { method: 'callHierarchy/incomingCalls', result: fanOut(base, 60, 'x') }] }),
  }),
  scene('hierarchyNodeLimit', {
    ...cappedNodeScene(), query: { ...CALL_ROOT, depth: 3 }, reasons: ['hierarchyNodeLimit'],
  }),
  scene('hierarchyCandidateLimit', {
    files: { ...TS_PROJECT, 'a.ts': `export class Base {}\n${Array.from({ length: 205 }, (_, index) => `class C${index} extends Base {}`).join('\n')}\n` },
    query: { path: '$DIR/a.ts', symbolName: 'Base', line: 1, operation: 'subtypes' }, reasons: ['hierarchyCandidateLimit'], channels: ['nextCandidatePage'],
    script: base => ({ capabilities: { typeHierarchyProvider: false }, answers: [{ method: 'textDocument/implementation', result: Array.from({ length: 205 }, (_, index) => loc(`${base}/a.ts`, index + 1, 6, 2)) }] }),
  }),
  scene('declarationClipped', {
    files: { ...TS_PROJECT, 'a.ts': DECLARATION },
    query: { path: '$DIR/a.ts', symbolName: 'f', line: DECLARATION.split('\n').findIndex(line => line === 'f();') + 1, operation: 'definition', view: 'content', contextLines: { before: 0, after: DECLARATION_END } }, reasons: ['declarationClipped'], channels: ['readDeclaration'],
    script: base => ({ answers: [{ method: 'textDocument/definition', result: [{ targetUri: `${base}/a.ts`, targetRange: { start: { line: 0, character: 0 }, end: { line: DECLARATION_END, character: 1 } }, targetSelectionRange: range(0, 16) }] }] }),
  }),
  // An omitted contextLines stays absent: the definition returns its
  // declaration page (up to 60 lines), and readDeclaration reads the rest.
  scene('declarationClippedByDefault', {
    files: { ...TS_PROJECT, 'a.ts': DECLARATION },
    query: { path: '$DIR/a.ts', symbolName: 'f', line: DECLARATION.split('\n').findIndex(line => line === 'f();') + 1, operation: 'definition', view: 'content' }, reasons: ['declarationClipped'], channels: ['readDeclaration'],
    script: base => ({ answers: [{ method: 'textDocument/definition', result: [{ targetUri: `${base}/a.ts`, targetRange: { start: { line: 0, character: 0 }, end: { line: DECLARATION_END, character: 1 } }, targetSelectionRange: range(0, 16) }] }] }),
  }),
  scene('definitionHopFailed', {
    files: { ...TS_PROJECT, 'a.ts': "import { b } from './b';\nb();\n", 'b.ts': "export { b } from './c';\n", 'c.ts': 'export function b() {}\n' },
    query: { path: '$DIR/a.ts', symbolName: 'b', line: 2, operation: 'definition' }, reasons: ['definitionHopFailed'],
    script: base => ({ answers: [{ method: 'textDocument/definition', uri: '/b.ts', error: REPLIES.error }, { method: 'textDocument/definition', result: [loc(`${base}/b.ts`, 0, 9)] }] }),
  }),
  scene('diagnosticsNotPublished', {
    files: { ...TS_PROJECT, 'a.ts': 'const a = 1;\n' }, query: { path: '$DIR/a.ts', operation: 'diagnostic' }, reasons: ['diagnosticsNotPublished'],
    script: () => ({ capabilities: { diagnosticProvider: null } }),
  }),
  scene('diagnosticsTruncated', {
    files: { ...TS_PROJECT, 'a.ts': 'const a = 1;\n' }, query: { path: '$DIR/a.ts', operation: 'diagnostic' }, reasons: ['diagnosticsTruncated'],
    script: () => ({ capabilities: { diagnosticProvider: null }, afterOpen: [{ method: 'textDocument/publishDiagnostics', params: { diagnostics: Array.from({ length: 2001 }, () => ({ range: range(0, 0), severity: 2, message: 'scripted' })) } }] }),
  }),
  scene('documentAnalysisErrors', {
    files: { ...TS_PROJECT, 'a.ts': 'broken here\n\n\n\nconst x = 1;\n' }, query: { path: '$DIR/a.ts', symbolName: 'x', line: 5, operation: 'definition' }, reasons: ['documentAnalysisErrors'],
    script: () => ({ answers: [{ method: 'textDocument/diagnostic', result: { kind: 'full', items: [{ range: range(0, 0), severity: 1, message: 'scripted error' }] } }] }),
  }),
  scene('dynamicDispatch', {
    languages: { '.py': 'python' }, files: { 'a.py': 'def target():\n    pass\n', 'b.py': 'obj.target()\n' },
    query: { path: '$DIR/a.py', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['dynamicDispatch'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/a.py`, 0, 4, 6)] }] }),
  }),
  scene('importUnresolved', {
    languages: { '.py': 'python' }, files: { 'a.py': 'def target():\n    pass\n', 'b.py': 'from a import target\ntarget()\n' },
    query: { path: '$DIR/a.py', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['importUnresolved'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/a.py`, 0, 4, 6)] }] }),
  }),
  scene('serverUnanswered', {
    languages: { '.go': 'go' }, files: { 'go.mod': 'module m\n', 'a.go': 'package m\nfunc Helper() {}\n', 'b.go': 'package m\nfunc C() { Helper() }\n' },
    query: { path: '$DIR/a.go', symbolName: 'Helper', line: 2, operation: 'references' }, reasons: ['serverUnanswered'], script: () => ({ answers: [{ method: 'textDocument/references', result: [] }] }),
  }),
  scene('noCompileDatabase', {
    languages: { '.c': 'c' }, command: 'clangd', files: { 'lib.c': 'int helper(void) { return 1; }\n', 'main.c': 'int helper(void);\nint main(void) { return helper(); }\n', 'lib.h': 'int helper(void);\n', 'compile_flags.txt': '-xc\n' },
    query: { path: '$DIR/lib.c', symbolName: 'helper', line: 1, operation: 'references' }, reasons: ['noCompileDatabase'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/lib.c`, 0, 4, 6)] }] }),
  }),
  scene('noProject', {
    files: { 'a.rs': 'pub fn x() {}\n' }, query: { path: '$DIR/a.rs', symbolName: 'x', line: 1, operation: 'definition' }, reasons: ['noProject'], script: () => ({}),
  }),
  scene('scanOnlyImplementations', {
    files: { ...TS_PROJECT, 'a.ts': 'export class Base {}\n', 'b.ts': "import { Base } from './a';\nclass X extends Base {}\n" },
    query: { path: '$DIR/a.ts', symbolName: 'Base', line: 1, operation: 'implementation' }, reasons: ['scanOnlyImplementations'], script: () => ({ answers: [{ method: 'textDocument/implementation', result: [] }] }),
  }),
  scene('languageServerIndexing', {
    languages: { '.sh': 'shellscript' }, files: { 'a.sh': 'x() { :; }\n' }, query: { path: '$DIR/a.sh', symbolName: 'x', line: 1, operation: 'definition' }, reasons: ['languageServerIndexing'],
    script: () => ({ afterInitialized: [{ method: '$/progress', params: { token: 'index', value: { kind: 'begin', title: 'indexing' } } }] }),
  }),
  scene('locationsWithheld', {
    files: { ...TS_PROJECT, 'a.ts': 'export function target() {}\n' }, query: { path: '$DIR/a.ts', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['locationsWithheld'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/a.ts`, 0, 16, 6), loc('file:///etc/hosts', 0, 0, 1)] }] }),
  }),
  scene('importerScanCapped', {
    files: { ...TS_PROJECT, 'a.ts': 'export function target() {}\n', ...Object.fromEntries(Array.from({ length: 26 }, (_, index) => [`f${String(index).padStart(3, '0')}.ts`, "import { target } from './a';\ntarget();\n"])) },
    query: { path: '$DIR/a.ts', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['importerScanCapped'], channels: ['nextImporterWindow'],
    script: base => ({ answers: [{ method: 'textDocument/references', uri: '/a.ts', result: [loc(`${base}/a.ts`, 0, 16, 6)] }, { method: 'textDocument/definition', result: [loc(`${base}/a.ts`, 0, 16, 6)] }, { method: 'textDocument/references', result: [] }] }),
  }),
  scene('importerRequestFailed', {
    files: { ...TS_PROJECT, 'a.ts': 'export function target() {}\n', 'f000.ts': "import { target } from './a';\ntarget();\n" },
    query: { path: '$DIR/a.ts', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['importerRequestFailed'],
    script: base => ({ answers: [{ method: 'textDocument/references', uri: '/a.ts', result: [loc(`${base}/a.ts`, 0, 16, 6)] }, { method: 'textDocument/definition', result: [loc(`${base}/a.ts`, 0, 16, 6)] }, { method: 'textDocument/references', uri: '/f000.ts', error: REPLIES.error }] }),
  }),
  scene('importerScanFailed', {
    files: { ...TS_PROJECT, 'a.ts': 'export function target() {}\n', 'u000.ts': "import { target } from './a';\ntarget();\n" },
    query: { path: '$DIR/a.ts', symbolName: 'target', line: 1, operation: 'references' }, reasons: ['importerScanFailed'],
    script: base => ({ answers: [{ method: 'textDocument/references', result: [loc(`${base}/a.ts`, 0, 16, 6)] }, { method: 'textDocument/definition', result: [] }] }),
  }),
  scene('workspaceSymbolCoverageUnknown', {
    command: 'tsgo', files: { ...TS_PROJECT, 'a.ts': 'export const Cap256 = 1;\n' }, query: { path: '$DIR/a.ts', symbolQuery: 'Cap256', operation: 'workspaceSymbol' }, reasons: ['workspaceSymbolCoverageUnknown'],
    script: base => ({ answers: [{ method: 'workspace/symbol', result: Array.from({ length: 256 }, (_, index) => ({ name: `Cap256${index}`, kind: 13, location: loc(`${base}/a.ts`, 0, 13, 6) })) }] }),
  }),
];

const CODES = [
  scene('anchorUnresolved', { files: { ...TS_PROJECT, 'a.ts': 'const a = 1;\n' }, query: { path: '$DIR/a.ts', symbolName: 'nope', line: 1, operation: 'definition' }, codes: ['anchorUnresolved'], script: () => ({}) }),
  scene('capabilityUnavailable', { files: { ...TS_PROJECT, 'a.ts': 'function f() {}\n' }, query: { path: '$DIR/a.ts', symbolName: 'f', line: 1, operation: 'hover' }, codes: ['capabilityUnavailable'], script: () => ({ capabilities: { hoverProvider: false } }) }),
  scene('requestFailed', { files: { ...TS_PROJECT, 'a.ts': 'function f() {}\n' }, query: { path: '$DIR/a.ts', symbolName: 'f', line: 1, operation: 'hover' }, codes: ['requestFailed'], script: () => ({ answers: [{ method: 'textDocument/hover', error: { code: -32002, message: 'scripted' } }] }) }),
  scene('serverCrashed', { files: { ...TS_PROJECT, 'a.ts': 'function f() {}\n' }, query: { path: '$DIR/a.ts', symbolName: 'f', line: 1, operation: 'hover' }, codes: ['serverCrashed'], script: () => ({ answers: [{ method: 'textDocument/hover', exit: true }] }) }),
  scene('serverUnavailable', { files: { 'a.xyz': 'f\n' }, query: { path: '$DIR/a.xyz', symbolName: 'f', line: 1, operation: 'definition' }, codes: ['serverUnavailable'], script: () => ({}) }),
  scene('workspaceRootInvalid', { files: { 'a.ts': 'export const f = 1;\n' }, query: { workspaceRoot: '$DIR/a.ts', operation: 'workspaceSymbol', symbolQuery: 'f' }, codes: ['workspaceRootInvalid'], script: () => ({}) }),
  scene('fileTooLarge', { files: { ...TS_PROJECT, 'big.ts': { sparse: 8 * 1024 * 1024 + 1, head: 'function f() {}\n' } }, query: { path: '$DIR/big.ts', symbolName: 'f', line: 1, operation: 'definition' }, codes: ['fileTooLarge'], script: () => ({}) }),
  scene('documentSyncFailed', { files: { ...TS_PROJECT, 'big.ts': `export const a = 1;\n${'// filler line for a large didOpen\n'.repeat(60_000)}` }, query: { path: '$DIR/big.ts', operation: 'documentSymbols' }, codes: ['documentSyncFailed'], script: () => ({ pauseAfterInitialized: true }) }),
];

export const LSP_SCENARIOS = [...REASONS, ...CODES];
