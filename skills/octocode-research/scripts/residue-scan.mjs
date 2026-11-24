#!/usr/bin/env node
// Writing-tell residue scan: AI tool markers, AI tracking parameters and placeholder patterns, line by line.
// Source: skills/common/scripts/residue-scan.mjs. Skill copies are generated; do not edit them.
// Regenerate with node skills/common/build.mjs.

import fs from 'node:fs';
import path from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

// The deterministic families of references/writing-tells.md; `family` is that page's table row.
// A new marker goes here, not into prose. Every pattern stays within one line.
export const PATTERNS = [
  { family: 'Tool residue', name: 'AI tracking parameter', regex: /\butm_source=(?:chatgpt\.com|openai|copilot\.com)\b/gi },
  { family: 'Tool residue', name: 'content reference', regex: /:?contentReference\[oaicite:\d+\](?:\{index=\d+\})?/g },
  { family: 'Tool residue', name: 'search result id', regex: /\bturn\d+(?:[a-z]+\d+|search)\b/g },
  { family: 'Tool residue', name: 'cite marker', regex: /\[cite(?::\s*\d+(?:\s*,\s*\d+)*|_start|_end)\]/g },
  { family: 'Tool residue', name: 'file citation', regex: /【\d+(?::\d+)?†[^】\n]*】/g },
  { family: 'Placeholders', name: 'bracketed fill-in', regex: /\[(?:your|insert|enter)\s[^\]\n]{1,60}\](?!\()/gi },
  { family: 'Placeholders', name: 'fill-in note', regex: /\((?:add|insert|enter|put|paste)\s[^)\n]{0,60}?\bhere\)/gi },
  { family: 'Placeholders', name: 'placeholder constant', regex: /\b(?:INSERT_[A-Z0-9_]+|[A-Z0-9_]+_HERE)\b/g },
  { family: 'Placeholders', name: 'example citation link', regex: /\bexample\.(?:com|org|net)\/(?:source|link|ref|reference|citation|article|paper|url)[-_]?\d+\b/gi },
  { family: 'Placeholders', name: 'unfilled date', regex: /\b(?:\d{4}|\d{2}XX)-(?:\d{2}|XX)-XX\b/gi },
  { family: 'Placeholders', name: 'filler text', regex: /\blorem ipsum\b/gi },
];

const SKIP_DIRS = new Set(['.git', 'node_modules']);

const HELP = `Scan text for deterministic writing-tell residue (references/writing-tells.md): AI tool markers,
AI tracking parameters and placeholder patterns. Reads files; changes nothing.

Usage:
  node scripts/residue-scan.mjs <file | folder>...
  node scripts/residue-scan.mjs --help

A folder is walked recursively, skipping ${[...SKIP_DIRS].map((d) => `${d}/`).join(' and ')}, symbolic links and files
with NUL bytes (binary). A path named as an argument is read through a symbolic link.

Output, one JSON line on stdout:
  {"ok":true|false,"files":<files scanned>,"findings":[{"file","line","family","match"}]}
file is the path as reached from the argument; line is 1-based; match is the exact text.

A finding is a candidate: quotations, fixtures, test data and documented examples keep theirs.
No finding means none of these patterns matched, not that the text is free of tells.

Exit codes: 0 no findings, 1 findings, 2 invalid input (no path, unknown option, missing path).

Patterns:
${PATTERNS.map((p) => `  ${p.family}: ${p.name}  /${p.regex.source}/${p.regex.flags.replace('g', '')}`).join('\n')}`;

/** Findings in one text: every pattern match, in reading order. */
export function scanText(text, file) {
  return text.split('\n').flatMap((line, i) => PATTERNS
    .flatMap(({ family, regex }) => [...line.matchAll(regex)].map((m) => ({ at: m.index, finding: { file, line: i + 1, family, match: m[0] } })))
    .toSorted((a, b) => a.at - b.at)
    .map(({ finding }) => finding));
}

/** Files under a path, as reached from it (posix separators); links are followed only when named. */
function filesUnder(target, named = false) {
  const stat = named ? fs.statSync(target) : fs.lstatSync(target);
  if (stat.isFile()) return [target];
  if (!stat.isDirectory()) return [];
  return fs.readdirSync(target, { withFileTypes: true })
    .filter((entry) => !(entry.isDirectory() && SKIP_DIRS.has(entry.name)))
    .toSorted((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => filesUnder(path.posix.join(target, entry.name)));
}

/** Scans files and folders; throws on a missing path. */
export function scanPaths(targets) {
  const findings = [];
  let files = 0;
  for (const target of targets) {
    if (!fs.existsSync(target)) throw new Error(`Not found: ${target}`);
    for (const file of filesUnder(target.split(path.sep).join('/'), true)) {
      const bytes = fs.readFileSync(file);
      if (bytes.includes(0)) continue;
      files++;
      findings.push(...scanText(bytes.toString('utf8'), file));
    }
  }
  return { ok: findings.length === 0, files, findings };
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); return 0; }
  const option = argv.find((a) => a.startsWith('-'));
  if (option) fail('invalidInput', `Unknown option: ${option}; see --help`, 2);
  if (!argv.length) fail('invalidInput', 'Give one or more files or folders to scan; see --help', 2);
  let result;
  try { result = scanPaths(argv); } catch (error) { fail('invalidInput', errorMessage(error), 2); }
  console.log(JSON.stringify(result));
  return result.ok ? 0 : 1;
}

if (isMainModule(import.meta.url)) process.exitCode = main(process.argv.slice(2));
