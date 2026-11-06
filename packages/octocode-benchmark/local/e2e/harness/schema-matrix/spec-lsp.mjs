// lspSearch on the protocol fixture: what the mock server answers is known (hand-read
// from `advanced-local/code` and `advanced-local/rust`), and native owns everything else.
import fs from 'node:fs';
import path from 'node:path';
import { declarations, lspLocations } from '../responses.mjs';

const ROOT = 'matrix-world/lsp';
const RUST_EXECUTION_FLAGS = ['buildScripts', 'procMacros'];
const CODE = `${ROOT}/code`;
const MODEL = `${CODE}/model.ts`;
const USE = `${CODE}/use.ts`;
const RUST = `${ROOT}/rust/src/lib.rs`;
/** Reference lines of `add`, with and without its declaring and importing lines. */
const ADD_USES = [['code/model.ts', 19], ['code/model.ts', 20], ['code/use.ts', 4]];
const ADD_DECLARATIONS = [['code/model.ts', 14], ['code/use.ts', 1], ['code/use.test.ts', 1]];
const ADD_TESTS = [['code/use.test.ts', 2]];
const ANCHORS = {
  definition: { path: USE, line: 4, symbolName: 'add' },
  typeDefinition: { path: USE, line: 3, symbolName: 'square' },
  implementation: { path: MODEL, line: 1, symbolName: 'Shape' },
  references: { path: MODEL, line: 14, symbolName: 'add' },
  callers: { path: MODEL, line: 14, symbolName: 'add' },
  callees: { path: MODEL, line: 18, symbolName: 'calculate' },
  supertypes: { path: MODEL, line: 12, symbolName: 'LargeSquare', depth: 2 },
  subtypes: { path: MODEL, line: 5, symbolName: 'Square' },
  hover: { path: MODEL, line: 14, symbolName: 'add' },
  documentSymbols: { path: MODEL },
  diagnostic: { path: `${CODE}/broken.ts` },
  workspaceSymbol: { symbolQuery: 'calculate' },
};
const OPERATIONS_WITH_WORKSPACE = new Set(['workspaceSymbol']);

function base(row, assignment) {
  if (row.rustContext) {
    if (row.operation !== 'references' || (row.rustContext.procMacros === true && row.rustContext.buildScripts === false)) return null;
    return { workspaceRoot: `${ROOT}/rust`, path: RUST, line: 1, symbolName: 'target' };
  }
  const anchor = ANCHORS[row.operation ?? 'definition'];
  if (!anchor) return null;
  const pathless = OPERATIONS_WITH_WORKSPACE.has(row.operation) && !Object.hasOwn(row, 'path');
  return {
    ...(pathless || row.operation === 'workspaceSymbol' ? { workspaceRoot: CODE } : { workspaceRoot: CODE }),
    ...anchor, ...(assignment.page === 'tiny' && { limit: 1 }),
  };
}

function rustModules(context, declared) {
  const features = context.features ?? [];
  const modules = ['always'];
  if (!context.noDefaultFeatures || features === 'all') modules.push('base_feature');
  if (features === 'all') modules.push('bright_feature', 'dim_feature');
  return [...modules.map(name => [`rust/src/${name}.rs`, 1]), ...(declared ? [['rust/src/lib.rs', 1]] : [])];
}

function referenceTruth(query) {
  if (query.rustContext) return query.testCode === 'only' ? [] : rustModules(query.rustContext, query.includeDeclaration !== false);
  const declaration = query.includeDeclaration !== false;
  const all = [...ADD_USES, ...ADD_TESTS, ...(declaration ? ADD_DECLARATIONS : [])];
  if (query.testCode === 'exclude') return all.filter(([file]) => !file.includes('.test.'));
  if (query.testCode === 'only') return all.filter(([file]) => file.includes('.test.'));
  return all;
}

const TRUTH = {
  definition: () => [['code/model.ts', 14]],
  typeDefinition: () => [['code/model.ts', 5]],
  implementation: () => [['code/model.ts', 5], ['code/model.ts', 12]],
  references: referenceTruth,
  callers: query => [...(query.testCode === 'only' ? [] : [['calculate', [19, 20]], ['<module>', [4]]]), ...(query.testCode === 'exclude' ? [] : [['<module>', [2]]])],
  callees: () => [['add', [19, 20]]],
  supertypes: () => [['Square', 1], ['Shape', 2]],
  subtypes: () => [['LargeSquare', 1]],
  hover: () => [['signature', true]],
  documentSymbols: () => [['add', 14], ['calculate', 18]],
  diagnostic: () => [[2322, 1]],
  workspaceSymbol: () => [['calculate', 18]],
};

const truth = (ctx, testCase) => {
  const operation = testCase.query.operation ?? 'definition';
  return (TRUTH[operation] ?? (() => { throw new Error(`no oracle for lspSearch operation ${operation}`); }))(testCase.query);
};

const dataOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
const rel = (ctx, value) => path.relative(fs.realpathSync(path.join(ctx.cwd, ROOT)), fs.realpathSync(path.resolve(ctx.cwd, value))).split(path.sep).join('/');
const fileRows = data => data.flatMap(row => row.files ?? []);

const VIEWS = {
  callers: data => fileRows(data).flatMap(file => (file.matches ?? []).map(match => [match.symbolName ?? '<module>', match.sites.map(site => site.line)])),
  callees: data => fileRows(data).flatMap(file => (file.matches ?? []).map(match => [match.symbolName, match.sites.map(site => site.line)])),
  supertypes: data => data.flatMap(row => row.matches ?? []).map(match => [match.symbolName, match.level]),
  subtypes: data => data.flatMap(row => row.matches ?? []).map(match => [match.symbolName, match.level ?? 1]),
  hover: data => [['signature', data.some(row => row.hover?.contents?.value?.includes('add(a: number, b: number): number'))]],
  documentSymbols: data => data.flatMap(row => declarations({ sc: { results: [{ data: row }] } })).filter(item => item.kind === 'function').map(item => [item.name, item.line]),
  diagnostic: data => data.flatMap(row => row.matches ?? []).map(item => [item.code, item.displayRange.startLine]),
  workspaceSymbol: data => data.flatMap(row => row.matches ?? []).map(match => [match.symbolName, match.displayRange.startLine]),
};

function observed(ctx, testCase, pages) {
  const { query } = testCase;
  const operation = query.operation ?? 'definition';
  const data = dataOf(pages);
  if (VIEWS[operation]) return VIEWS[operation](data);
  if (query.view === 'files') return fileRows(data).flatMap(file => file.lines.map(line => [rel(ctx, file.path), line]));
  return data.flatMap(row => lspLocations({ sc: { results: [{ data: row }] } })).map(item => [rel(ctx, item.path), item.displayRange.startLine]);
}

/** The fields the server is started with, read back from its own protocol trace. */
function check(ctx, testCase) {
  const context = testCase.query.rustContext;
  if (!context) return null;
  const trace = fs.readFileSync(path.join(ctx.home, '.octocode', 'mock-lsp.jsonl'), 'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line)).filter(entry => entry.method === 'initialize');
  const options = trace.map(entry => entry.params?.initializationOptions).filter(Boolean);
  const enabled = value => value === true;
  const started = options.find(option => (context.buildScripts === undefined || enabled(option.cargo?.buildScripts?.enable) === context.buildScripts)
    && (context.procMacros === undefined || enabled(option.procMacro?.enable) === context.procMacros));
  return started ? null : `rustContext ${JSON.stringify(context)}: no server started with those options: ${JSON.stringify(options).slice(0, 300)}`;
}

export const lspSearch = {
  tool: 'lspSearch',
  // This hermetic world's host policy keeps repository-code execution disabled.
  // Schema-valid requests to enable it must be refused before a server starts.
  refusal: (_ctx, { query }) => (RUST_EXECUTION_FLAGS.some(flag => query.rustContext?.[flag] === true) ? 'policy' : null),
  refusalValues: ({ values }) => values.filter(([field, value]) => value === true && RUST_EXECUTION_FLAGS.some(flag => field === `rustContext.${flag}`)),
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }],
  base,
  // Procedural macros need build scripts: the row that wants one asks for both.
  shape: query => (query.rustContext?.procMacros === true && query.rustContext.buildScripts === undefined ? { ...query, rustContext: { ...query.rustContext, buildScripts: true } } : query),
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 300; },
  truth,
  observed,
  check,
  indistinct: {
    operation: 'each value has its own anchor and oracle',
    view: 'the same locations by file or by row; both are compared with the same truth',
    'rustContext.buildScripts': 'true is judged as a host-policy refusal; false is checked in the server protocol trace',
    'rustContext.procMacros': 'true is judged as a host-policy refusal; false is checked in the server protocol trace',
  },
};
