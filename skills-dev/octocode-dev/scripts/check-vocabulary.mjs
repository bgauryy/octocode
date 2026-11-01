#!/usr/bin/env node
/** Offline gate: dotted field paths in skill and doc prose against the generated tool contract and the config contract. */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { extname, join } from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { contractVocabulary, proseFindings } from './lib/contract-vocabulary.mjs';
import { guidanceFiles } from './lib/guidance-files.mjs';
import { ROOT } from './lib/workspaces.mjs';

const TOOL_CONTRACT = 'packages/octocode-config/contract/tool-contract.json';
const CONFIG_CONTRACT = 'packages/octocode-config/config-contract.json';
const EXCEPTIONS = new URL('./vocabulary-exceptions.json', import.meta.url);
const HELP = `Usage: node skills-dev/octocode-dev/scripts/check-vocabulary.mjs [--json]

Checks every \`parent.child\` field path in inline code across skills, docs and package docs: when the tool contract
(${TOOL_CONTRACT}) or config contract (${CONFIG_CONTRACT}) declares \`parent\` but no such child,
the prose names a retired or invented field. Fenced code, file names and unknown parents are not checked.
A path that is right in another vocabulary is listed with its reason in vocabulary-exceptions.json; an entry
that matches nothing fails, so the list only shrinks. Example JSON calls are validated against live schemas by
check-research-guidance.mjs.`;

export function loadVocabulary(root = ROOT) {
  const read = path => JSON.parse(readFileSync(join(root, path), 'utf8'));
  return contractVocabulary(read(TOOL_CONTRACT), read(CONFIG_CONTRACT));
}

/** Extensions of tracked files, so `data.json` reads as a file name rather than a field path. */
function trackedExtensions(root) {
  const listed = spawnSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  if (listed.status !== 0) return new Set();
  return new Set(listed.stdout.split('\0').map(path => extname(path).slice(1)).filter(Boolean));
}

export function readExceptions() {
  return JSON.parse(readFileSync(EXCEPTIONS, 'utf8')).exceptions;
}

/** Findings without a listed exception, plus each exception that no finding matched. */
function applyExceptions(findings, exceptions) {
  const used = new Set();
  const kept = findings.filter(finding => {
    const index = exceptions.findIndex(entry => entry.file === finding.file && entry.token === finding.token);
    if (index < 0) return true;
    used.add(index);
    return false;
  });
  const stale = exceptions.filter((_, index) => !used.has(index))
    .map(entry => ({ file: entry.file, line: 0, token: entry.token, message: 'stale exception in vocabulary-exceptions.json: remove it' }));
  return [...kept, ...stale];
}

export function checkVocabulary({ root = ROOT, vocabulary = loadVocabulary(root), exceptions = readExceptions() } = {}) {
  const extensions = trackedExtensions(root);
  const files = guidanceFiles(root);
  const raw = files.flatMap(file => proseFindings(readFileSync(join(root, file), 'utf8'), vocabulary, extensions)
    .map(finding => ({ file, ...finding })));
  const findings = applyExceptions(raw, exceptions);
  return { pass: findings.length === 0, files: files.length, findings };
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--help')) {
    console.log(HELP);
  } else if (args.some(arg => arg !== '--json')) {
    console.error('Unknown option; use --help.');
    process.exitCode = 2;
  } else {
    const report = checkVocabulary();
    if (args.includes('--json')) console.log(JSON.stringify(report, null, 2));
    else {
      console.log(`${report.pass ? 'PASS' : 'FAIL'} contract-vocabulary: ${report.files} files, ${report.findings.length} findings`);
      for (const finding of report.findings) console.log(`  ${finding.file}:${finding.line} \`${finding.token}\`: ${finding.message}`);
    }
    process.exitCode = report.pass ? 0 : 1;
  }
}
