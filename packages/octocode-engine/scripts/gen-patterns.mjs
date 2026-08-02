#!/usr/bin/env node
/**
 * gen-patterns.mjs
 *
 * Generates src/patterns.rs from the canonical allRegexPatterns order in
 * this package's TypeScript source (ensures Rust pattern evaluation order ==
 * TS order).
 *
 * Previously this parsed TS source files directly (alphabetical file order).
 * That caused pattern-priority differences (e.g. credentialsInUrl firing before
 * postgresqlConnectionString). Now we bundle and import the source TS array
 * directly so generation never depends on a possibly stale dist/ directory.
 *
 * Conversion rules:
 *   - JS flag `g`  → dropped (Rust find_iter / replace_all are global)
 *   - JS flag `i`  → Rust inline (?i)
 *   - JS flag `m`  → Rust inline (?m)
 *   - JS flag `s`  → Rust inline (?s)
 *   - Named groups (?<name>...) → (?P<name>...)
 *   - [\d-_] invalid range → [\d\-_] (escaped hyphen)
 *   - Lookaheads / lookbehinds / backreferences → SKIPPED (none found in current patterns)
 */

import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import * as esbuild from 'esbuild';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SRC_ENTRY  = join(__dirname, '..', 'src', 'security', 'regexes', 'index.ts');
const OUT_FILE   = join(__dirname, '..', 'src', 'security', 'patterns.rs');

// ---------------------------------------------------------------------------
// Convert a JS RegExp to a Rust regex string
// ---------------------------------------------------------------------------
function jsRegexToRust(regex) {
  const source = regex.source;
  const flags  = regex.flags; // 'gi', 'i', 'g', '', etc.
  const warnings = [];

  if (/\(\?[=!]/.test(source)) warnings.push('lookahead');
  if (/\(\?<[=!]/.test(source)) warnings.push('lookbehind');
  if (/\\[1-9]/.test(source))   warnings.push('backreference');

  let rustSource = source;

  // Named capture groups: (?<name>...) → (?P<name>...)
  rustSource = rustSource.replace(/\(\?<([a-zA-Z_][a-zA-Z0-9_]*)>/g, '(?P<$1>');

  // Fix invalid range boundary in char classes: \d-X → \d\-X
  rustSource = rustSource.replace(/\[([^\]]+)\]/g, charClass =>
    charClass.replace(/(\\[dDwWsS])-(?!\])/g, '$1\\-')
  );

  // Build inline flags (drop `g` — Rust iterators are always global)
  const rustFlags = [];
  if (flags.includes('i')) rustFlags.push('i');
  if (flags.includes('m')) rustFlags.push('m');
  if (flags.includes('s')) rustFlags.push('s');
  const prefix = rustFlags.length > 0 ? `(?${rustFlags.join('')})` : '';

  return { rustSource: prefix + rustSource, warnings };
}

function rustStringEscape(s) {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

// ---------------------------------------------------------------------------
// Load allRegexPatterns in canonical TS source order.
// ---------------------------------------------------------------------------
console.log(`Loading allRegexPatterns from source: ${SRC_ENTRY}`);

async function loadAllRegexPatternsFromSource() {
  const tempDir = mkdtempSync(join(tmpdir(), 'octocode-security-patterns-'));
  const bundledEntry = join(tempDir, 'patterns.mjs');

  try {
    await esbuild.build({
      entryPoints: [SRC_ENTRY],
      outfile: bundledEntry,
      bundle: true,
      platform: 'node',
      target: 'node20',
      format: 'esm',
      logLevel: 'silent',
    });

    const { allRegexPatterns } = await import(
      `file://${bundledEntry}?t=${Date.now()}`
    );
    return allRegexPatterns;
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

const allRegexPatterns = await loadAllRegexPatternsFromSource();

console.log(`  Loaded ${allRegexPatterns.length} patterns in TS canonical order\n`);

const finalPatterns = [];
let skipped = 0;

for (const p of allRegexPatterns) {
  const { rustSource, warnings } = jsRegexToRust(p.regex);

  if (warnings.length > 0) {
    console.warn(`  SKIP "${p.name}": ${warnings.join(', ')}`);
    skipped++;
    continue;
  }

  // Quick JS sanity check (not a Rust compile, but catches obvious syntax)
  try {
    new RegExp(rustSource.replace(/^\(\?[ims]+\)/, ''));
  } catch (e) {
    console.warn(`  SKIP "${p.name}": JS validation failed — ${e.message}`);
    skipped++;
    continue;
  }

  finalPatterns.push({
    name:          p.name,
    fileContext:   p.fileContext
      ? { source: p.fileContext.source, flags: p.fileContext.flags }
      : null,
    rustSource,
  });
}

console.log(`Converted: ${finalPatterns.length} (${skipped} skipped)\n`);

if (skipped > 0) {
  console.error(
    `ERROR: ${skipped} pattern(s) could not be converted to Rust regex and would ` +
      `silently vanish from the runtime detector. Rewrite them without ` +
      `lookaheads/lookbehinds/backreferences, or remove them from allRegexPatterns.`
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Emit patterns.rs
// ---------------------------------------------------------------------------
const lines = [
  '// AUTO-GENERATED by scripts/gen-patterns.mjs — DO NOT EDIT',
  '// Source order: octocode-security-utils allRegexPatterns (canonical TS order)',
  '//',
  '// Conversion rules:',
  '//   g flag      → dropped (Rust iterators are global)',
  '//   i/m/s flags → Rust inline (?i)/(?m)/(?s)',
  '//   (?<name>)   → (?P<name>)',
  '//   [\\d-X]     → [\\d\\-X] (invalid range boundary fix)',
  '',
  'use regex::{Regex, RegexSet, RegexSetBuilder};',
  'use std::sync::{LazyLock, OnceLock};',
  '',
  '#[derive(Debug, Clone)]',
  'pub struct Pattern {',
  "    pub name: &'static str,",
  '    pub file_context: Option<&\'static str>,',
  '}',
  '',
  '/// All patterns in the same order as TypeScript allRegexPatterns',
  'pub static PATTERNS: &[Pattern] = &[',
];

for (const p of finalPatterns) {
  const fc = p.fileContext
    ? `Some("${rustStringEscape(p.fileContext.source)}")`
    : 'None';
  lines.push(
    '    Pattern {',
    `        name: "${p.name}",`,
    `        file_context: ${fc},`,
    '    },'
  );
}
lines.push('];', '');

// RegexSet for fast detection
lines.push(
  `/// Legacy full RegexSet — retained ONLY as the differential-test oracle for`,
  `/// the literal prescan (see detector.rs prescan_agrees_with_legacy_regexset_*).`,
  `/// Runtime code must use the prescan + pattern_regex(idx) path instead.`,
  '#[allow(dead_code)]',
  'pub static REGEX_SET: LazyLock<RegexSet> = LazyLock::new(|| {',
  '    RegexSetBuilder::new([',
);
for (const p of finalPatterns) {
  lines.push(`        r###"${p.rustSource}"###,`);
}
lines.push(
  '    ])',
  '    .size_limit(256 * 1024 * 1024)',
  '    .dfa_size_limit(256 * 1024 * 1024)',
  '    .build()',
  '    .expect("All patterns must be valid Rust regex")',
  '});',
  ''
);

// Raw pattern strings — single source for lazy per-index compilation and
// prescan literal extraction (see detector.rs). Index-aligned with PATTERNS.
lines.push(
  '/// Raw regex strings, index-aligned with PATTERNS.',
  'pub static PATTERN_STRINGS: &[&str] = &[',
);
for (const p of finalPatterns) {
  lines.push(`    r###"${p.rustSource}"###,`);
}
lines.push('];', '');

// Lazy per-index Regex cells: first call compiles ONLY the patterns the
// prescan flagged, not all of them (the old eager Vec<Regex> cost hundreds of
// ms on every fresh process).
lines.push(
  '/// Lazily-compiled per-pattern Regex instances (compile on first use per index).',
  'static PATTERN_REGEX_CELLS: LazyLock<Vec<OnceLock<Regex>>> = LazyLock::new(|| {',
  `    (0..${finalPatterns.length}).map(|_| OnceLock::new()).collect()`,
  '});',
  '',
  '/// Get (compiling at most once) the Regex for pattern `idx`.',
  'pub fn pattern_regex(idx: usize) -> &\'static Regex {',
  '    PATTERN_REGEX_CELLS[idx].get_or_init(|| {',
  '        Regex::new(PATTERN_STRINGS[idx]).expect(PATTERNS[idx].name)',
  '    })',
  '}',
  '');

writeFileSync(OUT_FILE, lines.join('\n'), 'utf8');
console.log(`✅ Wrote ${finalPatterns.length} patterns → ${OUT_FILE}`);
