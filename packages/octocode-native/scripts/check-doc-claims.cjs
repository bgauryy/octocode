#!/usr/bin/env node
'use strict';
// Docs-drift gate: pins machine-derivable README/doc claims against the
// source that owns them. Prose drifted repeatedly this cycle (exit-code
// table, tool counts, continuation format, env-var names); schemas are
// pinned by tests but prose was not. Claims are derived from source text —
// not a built binary — so the gate is deterministic and never flakes on
// build state.
//
// Pinned claims (keep this list small and machine-derivable; see RFC
// post-audit-hardening-2026-09 R3 — shrink the set rather than loosen it):
//   1. README exit-code table ↔ EXIT CODES block in cli/mod.rs long_about
//   2. Root-README tool count ↔ the ToolId enum in tools/id.rs
//   3. docs/CONFIGURATION.md env-var names ↔ config sources (both ways for
//      resolver SOURCE_KEYS)
//   4. Active source/docs/examples contain no retired pre-v20 CLI grammar

const fs = require('node:fs');
const path = require('node:path');

const nativeRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(nativeRoot, '..', '..');

const read = (p) => fs.readFileSync(p, 'utf8');
const cliSource = read(path.join(nativeRoot, 'crates/runtime/src/cli/mod.rs'));
const resolverSource = read(path.join(nativeRoot, 'crates/runtime/src/config/resolver.rs'));
const nativeReadme = read(path.join(nativeRoot, 'README.md'));
const rootReadme = read(path.join(repoRoot, 'README.md'));
const configDoc = read(path.join(repoRoot, 'docs/CONFIGURATION.md'));

const failures = [];
const fail = (claim, detail) => failures.push(`- ${claim}\n    ${detail}`);
const normalize = (s) =>
  s
    .replace(/[—–]/g, '-')
    .replace(/[`*]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();

// 1. Exit-code table --------------------------------------------------------
// Source of truth: the EXIT CODES block inside long_about.
const longAbout = cliSource.match(/EXIT CODES:(.*?)"\s*\)]/s);
if (!longAbout) {
  fail('exit codes', 'EXIT CODES block not found in cli/mod.rs long_about');
} else {
  const sourceCodes = new Map();
  for (const line of longAbout[1].split('\\n')) {
    const m = line.replace(/\\\s*/g, '').match(/^\s*(\d+)\s+(.*)$/);
    if (m) sourceCodes.set(m[1], normalize(m[2]));
  }
  const readmeCodes = new Map();
  for (const m of nativeReadme.matchAll(/^\|\s*`(\d+)`\s*\|\s*(.+?)\s*\|\s*$/gm)) {
    readmeCodes.set(m[1], normalize(m[2]));
  }
  for (const [code, meaning] of sourceCodes) {
    if (!readmeCodes.has(code)) {
      fail('exit codes', `code ${code} in cli/mod.rs long_about is missing from packages/octocode-native/README.md`);
    } else if (!readmeCodes.get(code).startsWith(meaning)) {
      fail(
        'exit codes',
        `code ${code}: README says "${readmeCodes.get(code)}" but cli/mod.rs says "${meaning}"`
      );
    }
  }
  for (const code of readmeCodes.keys()) {
    if (!sourceCodes.has(code)) {
      fail('exit codes', `code ${code} documented in README but absent from cli/mod.rs long_about`);
    }
  }
  if (sourceCodes.size === 0) fail('exit codes', 'no codes parsed from long_about (parser drift)');
}

// 2. Tool count -------------------------------------------------------------
// Canonical registry: the ToolId enum (tools/id.rs), stable across CLI
// surface refactors.
const toolIdSource = read(path.join(nativeRoot, 'crates/runtime/src/tools/id.rs'));
const toolEnum = toolIdSource.match(/pub enum ToolId \{(.*?)\}/s);
const claim = rootReadme.match(/(\d+) tools in the full discovery catalog/);
if (!toolEnum) {
  fail('tool count', 'ToolId enum not found in tools/id.rs');
} else if (!claim) {
  fail('tool count', '"N tools in the full discovery catalog" claim not found in root README.md');
} else {
  const count = [...toolEnum[1].matchAll(/^\s*([A-Z][A-Za-z0-9]*),/gm)].length;
  if (String(count) !== claim[1]) {
    fail('tool count', `root README claims ${claim[1]} tools; ToolId enum has ${count}`);
  }
}

// 3. Env-var names ----------------------------------------------------------
// Docs → code: every env var named in a CONFIGURATION.md table row must be
// read somewhere in product source — the native runtime or a package's
// TS/JS source (name typos, renames, and retired keys surface).
const sourceDirs = [path.join(nativeRoot, 'crates/runtime/src')];
for (const pkg of fs.readdirSync(path.join(repoRoot, 'packages'))) {
  const src = path.join(repoRoot, 'packages', pkg, 'src');
  if (fs.existsSync(src)) sourceDirs.push(src);
}
let runtimeSource = '';
const sourceExts = new Set(['.rs', '.ts', '.mts', '.cts', '.js', '.mjs', '.cjs']);
(function walkAll(dirs) {
  for (const dir of dirs) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) walkAll([p]);
      else if (sourceExts.has(path.extname(entry.name))) runtimeSource += read(p);
    }
  }
})(sourceDirs);
const documentedKeys = new Set();
for (const m of configDoc.matchAll(/^\|\s*`([A-Z][A-Z0-9_]{2,})`\s*\|/gm)) {
  documentedKeys.add(m[1]);
}
for (const key of documentedKeys) {
  if (!runtimeSource.includes(key)) {
    fail('env names', `docs/CONFIGURATION.md documents ${key} but crates/runtime/src never reads it`);
  }
}
// Code → docs: every resolver SOURCE_KEYS entry must be mentioned in
// CONFIGURATION.md (table row or prose) so no config key ships undocumented.
const sourceKeys = resolverSource.match(/const SOURCE_KEYS[^=]*= \[(.*?)\];/s);
if (!sourceKeys) {
  fail('env names', 'SOURCE_KEYS array not found in config/resolver.rs');
} else {
  for (const m of sourceKeys[1].matchAll(/"([A-Z][A-Z0-9_]+)"/g)) {
    if (!configDoc.includes(m[1])) {
      fail('env names', `resolver SOURCE_KEYS contains ${m[1]} but docs/CONFIGURATION.md never mentions it`);
    }
  }
}

// 4. Retired CLI grammar ----------------------------------------------------
// Historical benchmark receipts keep their original commands. Everything
// else that is maintained must use `scheme` and direct root tool commands.
const retiredCliPatterns = [
  /\bnode\s+[^\n]*octocode(?:\.js)?\s+tools(?:\s|$)/,
  /\bnpx(?:\s+-y)?\s+octocode\s+tools(?:\s|$)/,
  /\bcontext\s+--(?:json|compact|minimal)\b/,
  /--scheme(?:-view)?\b/,
  /--queries\b/,
  /\[\s*['"]tools['"]\s*,\s*['"][A-Za-z]/,
];
const retiredCliSelfTest = [
  'node packages/octocode/out/octocode.js tools localFetch --scheme --json',
  "['tools', 'localFetch', '--queries', '{}']",
  'octocode context --compact',
];
if (retiredCliSelfTest.some((sample) => !retiredCliPatterns.some((pattern) => pattern.test(sample)))) {
  fail('retired CLI grammar', 'the detector self-test no longer recognizes a retired command shape');
}
if (
  retiredCliPatterns.some((pattern) =>
    pattern.test("node packages/octocode/out/octocode.js scheme localFetch --view query --compact"),
  )
) {
  fail('retired CLI grammar', 'the detector self-test rejects the current scheme grammar');
}

const ignoredDirs = new Set([
  '.git',
  '.octocode',
  'node_modules',
  'out',
  'dist',
  'target',
  'coverage',
  'octocode-benchmark',
]);
const ignoredFiles = new Set([
  path.join(repoRoot, 'docs/JEV_BENCHMARK.md'),
  path.join(repoRoot, 'packages/octocode-native/scripts/check-doc-claims.cjs'),
  path.join(repoRoot, 'skills/octocode-research/scripts/check-guidance.mjs'),
  path.join(repoRoot, 'packages/octocode/skills/octocode-research/scripts/check-guidance.mjs'),
]);
const activeExtensions = new Set(['.md', '.rs', '.ts', '.mts', '.cts', '.js', '.mjs', '.cjs']);
function scanRetiredCliGrammar(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (ignoredDirs.has(entry.name)) continue;
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      scanRetiredCliGrammar(file);
      continue;
    }
    if (ignoredFiles.has(file) || !activeExtensions.has(path.extname(entry.name))) continue;
    const lines = read(file).split('\n');
    for (let index = 0; index < lines.length; index += 1) {
      if (retiredCliPatterns.some((pattern) => pattern.test(lines[index]))) {
        fail(
          'retired CLI grammar',
          `${path.relative(repoRoot, file)}:${index + 1} still contains: ${lines[index].trim()}`,
        );
      }
    }
  }
}
scanRetiredCliGrammar(repoRoot);

if (failures.length > 0) {
  console.error('check-doc-claims: pinned doc claims drifted from source:\n');
  console.error(failures.join('\n'));
  console.error('\nFix the doc (or the source) rather than loosening this gate.');
  process.exit(1);
}
console.log('check-doc-claims: all pinned doc claims match source.');
