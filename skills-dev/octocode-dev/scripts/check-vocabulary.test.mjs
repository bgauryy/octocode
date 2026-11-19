import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { checkVocabulary, loadVocabulary, nativeHintFiles } from './check-vocabulary.mjs';
import { contractVocabulary, proseFindings, rustFindings, rustStringLiterals } from './lib/contract-vocabulary.mjs';

const vocabulary = loadVocabulary();
const findingsIn = (text, extensions) => proseFindings(text, vocabulary, extensions).map(finding => finding.token);

test('a retired continuation name is flagged against the live contract', () => {
  // The research checker passed 9/9 while shared docs said `next.page`; the contract emits `next.nextPage`.
  assert.deepEqual(findingsIn('Follow `next.page` to the last page.'), ['next.page']);
});

test('output fields, continuation channels and config keys are valid vocabulary', () => {
  const text = [
    'Read `rows`, `pagination.total`, `pagination.hasMore` and `responsePagination.next`.',
    'Then `next.nextPage`, `next.restart`, `next.unfiltered`, `related[]` and `lsp.autoInstall`.',
  ].join('\n');
  assert.deepEqual(findingsIn(text), []);
});

test('error-row continuation channels are valid next children', () => {
  const own = contractVocabulary({ tools: [], continuationChannels: {}, errorContinuationChannels: { retry: 'branch' } }, { sections: {} });
  assert.deepEqual(proseFindings('Run `next.retry`.', own), []);
  assert.deepEqual(findingsIn('A rateLimited row offers `next.retry`.'), []);
});

test('schema fields behind IR-root shared definitions remain valid vocabulary', () => {
  const shared = {
    $defs: { Coverage: { type: 'object', properties: { importerScan: { type: 'object' } } } },
    tools: [{ name: 'lspSearch', querySchema: { properties: { coverage: { $ref: '#/$defs/Coverage' } } } }],
  };
  const own = contractVocabulary(shared, { sections: {} });
  assert.deepEqual([...own.children.get('coverage')], ['importerScan']);
  assert.deepEqual(proseFindings('Read `coverage.importerScan`.', own), []);
});

test('file names, method calls, fenced code and unknown parents are not field claims', () => {
  const text = [
    'Writes `data.json` and `graph.rs`; calls `error.format()`; `process.env.HOME` is the shell.',
    '```js',
    'const page = response.next.page;',
    '```',
  ].join('\n');
  assert.deepEqual(findingsIn(text, new Set(['json', 'rs'])), []);
});

test('a finding names the undeclared child and its parent, with its line', () => {
  const own = contractVocabulary({ tools: [], continuationChannels: { t: { row: { nextPage: 'linear' } } } }, { sections: {} });
  assert.deepEqual(proseFindings('ok\n`next.page`', own), [
    { line: 2, token: 'next.page', message: '`page` is not a field of `next` in the tool or config contract' },
  ]);
});

test('the check reports each finding once, at the shared source rather than its copies', () => {
  const root = mkdtempSync(join(tmpdir(), 'vocabulary-'));
  try {
    const shared = 'Follow `next.page`.\n';
    for (const dir of ['skills/common/references', 'skills/example/references', 'docs']) mkdirSync(join(root, dir), { recursive: true });
    writeFileSync(join(root, 'skills/common/references/usage.md'), shared);
    writeFileSync(join(root, 'skills/example/references/usage.md'), shared);
    writeFileSync(join(root, 'skills/example/SKILL.md'), '# Example\n');
    writeFileSync(join(root, 'docs/TOOLS.md'), 'Read `next.nextPage`.\n');
    const report = checkVocabulary({ root, vocabulary, exceptions: [] });
    assert.equal(report.pass, false);
    assert.deepEqual(report.findings.map(finding => `${finding.file}:${finding.line} ${finding.token}`), ['skills/common/references/usage.md:1 next.page']);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('the vocabulary check runs inside docs:verify', () => {
  const gate = readFileSync(new URL('./docs-verify.mjs', import.meta.url), 'utf8');
  assert.match(gate, /^import \{ checkVocabulary \} from '\.\/check-vocabulary\.mjs';$/m);
  assert.match(gate, /function main\(\) \{[^}]*\.\.\.checkVocabulary\(\)\.findings/s);
});

test('an exception that matches no finding fails as stale', () => {
  const root = mkdtempSync(join(tmpdir(), 'vocabulary-'));
  try {
    mkdirSync(join(root, 'docs'));
    writeFileSync(join(root, 'docs/TOOLS.md'), 'Read `next.nextPage`.\n');
    const report = checkVocabulary({ root, vocabulary, exceptions: [{ file: 'docs/TOOLS.md', token: 'next.page', reason: 'fixed since' }] });
    assert.equal(report.pass, false);
    assert.match(report.findings[0].message, /stale exception/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('every listed exception still matches a finding in the repository', () => {
  const stale = checkVocabulary().findings.filter(finding => finding.message.startsWith('stale exception'));
  assert.deepEqual(stale, []);
});

test('reads Rust string literals, plain and raw, past comments and char literals', () => {
  const source = [
    'let a = "next.nextPage";',
    "let quote = '\"'; // \"in.comment\"",
    '/* "in.block" */ let raw = r#"say "hi" to data.files"#;',
    "fn f<'a>(x: &'a str) -> &'a str { \"tail\\\" quoted\" }",
  ].join('\n');
  assert.deepEqual(rustStringLiterals(source), [
    { line: 1, value: 'next.nextPage' },
    { line: 3, value: 'say "hi" to data.files' },
    { line: 4, value: 'tail\\" quoted' },
  ]);
});

test('a native hint naming an undeclared child of a contract field is a finding', () => {
  const vocabulary = loadVocabulary();
  const source = 'fn hint() -> String { format!("run next.nextPage, then next.nextPgae; see Cargo.toml and osv.dev") }';
  assert.deepEqual(rustFindings(source, vocabulary, new Set(['toml'])), [
    { line: 1, token: 'next.nextPgae', message: '`nextPgae` is not a field of `next` in the tool or config contract' },
  ]);
});

test('the native scan covers lead, hint, warning and notice modules, never tests', () => {
  const files = nativeHintFiles();
  assert.ok(files.includes('packages/octocode-native/crates/tools-shared/src/leads/skipped.rs'), files.join('\n'));
  assert.ok(files.includes('packages/octocode-native/crates/runtime-base/src/response/notices.rs'));
  assert.ok(files.every(file => !/(^|\/|_)tests?(\/|\.rs$)/.test(file)), files.join('\n'));
});
