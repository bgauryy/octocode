import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  finishedIndexProblems,
  indexedReferencesProblems,
  semanticLocationEvidence,
  selectedGrammarCases,
  unindexedReferencesProblems,
} from './grammar-live-oracle.mjs';

const expected = [
  { start: { line: 2, character: 4 } },
  { start: { line: 5, character: 16 } },
];
const locations = [
  { path: 'build.sbt', line: 3, column: 5 },
  { path: 'build.sbt', line: 6, column: 17 },
];
const evidence = rows =>
  semanticLocationEvidence(rows, expected, '/fixture/build.sbt', '/fixture');
test('semantic oracle accepts reordered evidence and resolves relative file identity', () => {
  assert.equal(evidence(locations.toReversed()).ok, true);
});
test('semantic oracle rejects duplicate, missing, wrong-file and wrong Unicode-column evidence', () => {
  assert.equal(evidence([locations[0], locations[0]]).ok, false);
  assert.equal(evidence(locations.slice(1)).ok, false);
  assert.equal(
    evidence(locations.map(row => ({ ...row, path: 'other.sbt' }))).ok,
    false
  );
  assert.equal(
    evidence([
      locations[0],
      { ...locations[1], line: 6, column: 16 },
    ]).ok,
    false
  );
});
test('selection retains every owned extension and rejects empty accidental selections', () => {
  const capabilities = [
    { language: 'Scala', extensions: ['scala', 'sbt', 'sc'] },
    { language: 'Java', extensions: ['java'] },
  ];
  assert.equal(selectedGrammarCases(capabilities, new Set()).length, 4);
  assert.deepEqual(
    selectedGrammarCases(capabilities, new Set(['Scala'])).map(row => row.ext),
    ['scala', 'sbt', 'sc']
  );
  assert.deepEqual(
    selectedGrammarCases(capabilities, new Set(['sc'])).map(row => row.ext),
    ['sc']
  );
  assert.throws(
    () => selectedGrammarCases(capabilities, new Set(['scalaa'])),
    /Unknown grammar selection/
  );
});

const indexWarningText =
  "clangd's index does not cover every file here yet. A compile_commands.json gives clangd an index.";
const noDatabase = { reason: 'noCompileDatabase', configuration: 'notFound' };
const page = data => ({ json: { results: [{ index: 0, data }] } });
const partialRow = (overrides = {}) => ({
  isPartial: true,
  partialReasons: ['noCompileDatabase'],
  coverage: { state: 'partial', textOnlyFiles: 0 },
  warnings: [indexWarningText],
  lsp: { analysisContext: { compilation: { configuration: 'notFound' } } },
  ...overrides,
});

test('an unindexed row is honest only when every page says partial, why, and how to fix it', () => {
  assert.deepEqual(
    unindexedReferencesProblems(
      [page(partialRow()), page(partialRow())],
      noDatabase
    ),
    []
  );
  const wrong = {
    'a complete row': {
      isPartial: undefined,
      partialReasons: undefined,
      coverage: { state: 'complete', textOnlyFiles: 0 },
      warnings: undefined,
    },
    'another reason': { partialReasons: ['serverUnanswered'] },
    'an extra reason': {
      partialReasons: ['noCompileDatabase', 'documentAnalysisErrors'],
    },
    'no warning': { warnings: [] },
    'another warning': { warnings: ['clangd reported an error'] },
    'coverage says complete': {
      coverage: { state: 'complete', textOnlyFiles: 0 },
    },
    'an unanswered spelling': {
      coverage: { state: 'partial', textOnlyFiles: 1 },
    },
    'configuration found': {
      lsp: { analysisContext: { compilation: { configuration: 'found' } } },
    },
  };
  for (const [name, overrides] of Object.entries(wrong)) {
    assert.notDeepEqual(
      unindexedReferencesProblems([page(partialRow(overrides))], noDatabase),
      [],
      name
    );
    assert.notDeepEqual(
      unindexedReferencesProblems(
        [page(partialRow()), page(partialRow(overrides))],
        noDatabase
      ),
      [],
      `later page: ${name}`
    );
  }
});

test('an unindexed answer with no rows, or an error, proves nothing', () => {
  assert.notDeepEqual(unindexedReferencesProblems([], noDatabase), []);
  assert.notDeepEqual(
    unindexedReferencesProblems([{ json: { results: [] } }], noDatabase),
    []
  );
  assert.notDeepEqual(
    unindexedReferencesProblems(
      [{ ...page(partialRow()), isError: true }],
      noDatabase
    ),
    []
  );
});

const database = '/fixture/compile_commands.json';
const completeRow = (overrides = {}) => ({
  coverage: { state: 'complete', textOnlyFiles: 0 },
  lsp: {
    analysisContext: {
      compilation: { configuration: 'found', sources: [database] },
    },
  },
  ...overrides,
});

test('an indexed row is complete only from the database the project holds', () => {
  assert.deepEqual(
    indexedReferencesProblems([page(completeRow()), page(completeRow())], {
      database,
    }),
    []
  );
  for (const overrides of [
    { isPartial: true, partialReasons: ['serverUnanswered'] },
    { coverage: { state: 'partial', textOnlyFiles: 0 } },
    { coverage: { state: 'complete', textOnlyFiles: 1 } },
    { warnings: [indexWarningText] },
    {
      lsp: {
        analysisContext: {
          compilation: {
            configuration: 'found',
            sources: ['/fixture/compile_flags.txt'],
          },
        },
      },
    },
  ])
    assert.notDeepEqual(
      indexedReferencesProblems(
        [page(completeRow()), page(completeRow(overrides))],
        { database }
      ),
      [],
      JSON.stringify(overrides)
    );
  assert.notDeepEqual(indexedReferencesProblems([], { database }), []);
});

test('a finished index is a mirror with a background index, a shard for the file, and a clean project', () => {
  const tmp = fs.mkdtempSync(
    path.join(os.tmpdir(), 'octocode-live-index-test-')
  );
  try {
    const home = path.join(tmp, 'home');
    const mirror = path.join(home, 'lsp-workspaces/clangd/abc');
    const cwd = path.join(tmp, 'c-database');
    const file = path.join(cwd, 'main.c');
    fs.mkdirSync(path.join(mirror, '.cache/clangd/index'), { recursive: true });
    fs.mkdirSync(cwd);
    const args = [`--compile-commands-dir=${mirror}`, '--background-index'];
    const problems = (overrides = {}) =>
      finishedIndexProblems({
        status: { server: { args } },
        home,
        cwd,
        file,
        ...overrides,
      });
    assert.match(problems().join(), /no index shard/u);
    fs.writeFileSync(
      path.join(mirror, '.cache/clangd/index/main.cpp.ABC.idx'),
      ''
    );
    assert.match(problems().join(), /no index shard/u, "another file's shard");
    fs.writeFileSync(
      path.join(mirror, '.cache/clangd/index/main.c.ABC.idx'),
      ''
    );
    assert.deepEqual(problems(), []);
    assert.notDeepEqual(
      problems({ status: { server: { args: args.slice(0, 1) } } }),
      [],
      'no background index'
    );
    assert.notDeepEqual(
      problems({
        status: {
          server: {
            args: ['--compile-commands-dir=/elsewhere', '--background-index'],
          },
        },
      }),
      [],
      'not under the home'
    );
    assert.notDeepEqual(problems({ status: {} }), [], 'no server');
    fs.mkdirSync(path.join(cwd, '.cache'));
    assert.match(problems().join(), /into the project/u);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
