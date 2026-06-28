#!/usr/bin/env node
/**
 * Run all local tool patterns (LSP, AST, text search) against each cloned repo
 * and write per-repo results to results/repo/<name>/results.md.
 *
 * Usage:
 *   node benchmark/repo/run.mjs               # all repos
 *   node benchmark/repo/run.mjs react nextjs  # specific repos
 *
 * Repos must be cloned first:
 *   node benchmark/repo/clone.mjs
 *
 * Each test probe exercises one engine layer (the layers actually run below):
 *   text    — structuralSearch text/ripgrep-style search over a real repo file
 *   ast     — structuralSearch mode:"structural" (metavar/rule patterns)
 *   symbols — structuralSearch parse-probe: counts AST nodes a real file yields
 *             (NOT a language server — it does not call lspGetSemantics)
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dir = dirname(fileURLToPath(import.meta.url));
const TARGET  = join(__dir, '../../target');
const RESULTS = join(__dir, '../../results/repo'); // results live in results/, not benchmark source
const PINS_FILE = join(__dir, 'pins.json');
const ENGINE_MJS = join(__dir, '../../benchmark/_engine.mjs');

// ── Load engine ───────────────────────────────────────────────────────────────

let engine;
try {
  const mod = await import(ENGINE_MJS);
  engine = mod.engine;
} catch (e) {
  console.error('Could not load octocode-engine. Run `yarn build` in packages/octocode-engine first.');
  console.error(e.message);
  process.exit(1);
}

// ── Per-repo probe suites ─────────────────────────────────────────────────────

/**
 * Each probe returns { name, layer, input, result, pass, notes }.
 * `input` describes the query; `result` is a truncated summary string.
 */

async function runTextSearch(repoPath, pattern, ext) {
  const start = Date.now();
  try {
    const r = await engine.searchRipgrep({
      path: repoPath,
      pattern,
      maxCount: 3,
      ...(ext ? { glob: [`*.${ext}`] } : {}),
    });
    const elapsed = Date.now() - start;
    const hits = r?.files?.length ?? 0;
    return { ok: hits > 0, hits, elapsed, detail: `${hits} files` };
  } catch (err) {
    return { ok: false, hits: 0, elapsed: Date.now() - start, detail: err.message };
  }
}

function runStructural(repoPath, pattern, ext) {
  const start = Date.now();
  try {
    const r = engine.structuralSearchFiles({
      path: repoPath,
      pattern,
      include: [`*.${ext}`],
      maxFiles: 10,
    });
    const elapsed = Date.now() - start;
    const hits = r?.totalMatches ?? 0;
    return { ok: hits > 0, hits, elapsed, detail: `${hits} matches in ${r?.files?.length ?? 0} files` };
  } catch (err) {
    return { ok: false, hits: 0, elapsed: Date.now() - start, detail: err.message };
  }
}

function runDocumentSymbols(repoPath, filePath) {
  const start = Date.now();
  try {
    const absPath = join(repoPath, filePath);
    const content = readFileSync(absPath, 'utf8');
    const ext = filePath.split('.').pop() ?? 'ts';
    const r = engine.structuralSearch(content, `probe.${ext}`, '$$$', null);
    const elapsed = Date.now() - start;
    const count = Array.isArray(r) ? r.length : 0;
    return { ok: count > 0, hits: count, elapsed, detail: `${count} nodes parsed` };
  } catch (err) {
    return { ok: false, hits: 0, elapsed: Date.now() - start, detail: err.message };
  }
}

// ── Probe suites per repo ─────────────────────────────────────────────────────

const SUITES = {
  react: {
    lang: 'JavaScript/TypeScript',
    probes: [
      { name: 'text: find useState',        layer: 'text',      fn: p => runTextSearch(p, 'useState') },
      { name: 'text: find useEffect',       layer: 'text',      fn: p => runTextSearch(p, 'useEffect') },
      { name: 'text: find export default',  layer: 'text',      fn: p => runTextSearch(p, 'export default') },
      { name: 'ast: arrow functions (jsx)', layer: 'ast',       fn: p => runStructural(p, 'const $N = ($$$A) => $B', 'jsx') },
      { name: 'ast: function declarations', layer: 'ast',       fn: p => runStructural(p, 'function $NAME($$$ARGS) { $$$BODY }', 'js') },
      { name: 'symbols: packages/react/src/ReactHooks.js', layer: 'symbols', fn: p => runDocumentSymbols(p, 'packages/react/src/ReactHooks.js') },
    ],
  },
  tokio: {
    lang: 'Rust',
    probes: [
      { name: 'text: find async fn',        layer: 'text',      fn: p => runTextSearch(p, 'async fn') },
      { name: 'text: find tokio::spawn',    layer: 'text',      fn: p => runTextSearch(p, 'tokio::spawn') },
      { name: 'text: find #[tokio::main]',  layer: 'text',      fn: p => runTextSearch(p, '#[tokio::main]') },
      { name: 'ast: fn items (rs)',         layer: 'ast',       fn: p => runStructural(p, 'fn $NAME($$$ARGS) { $$$BODY }', 'rs') },
      { name: 'ast: impl blocks',           layer: 'ast',       fn: p => runStructural(p, 'impl $TYPE { $$$BODY }', 'rs') },
      { name: 'symbols: tokio/src/lib.rs',  layer: 'symbols',   fn: p => runDocumentSymbols(p, 'tokio/src/lib.rs') },
    ],
  },
  'spring-boot': {
    lang: 'Java',
    probes: [
      { name: 'text: @SpringBootApplication',     layer: 'text',    fn: p => runTextSearch(p, '@SpringBootApplication') },
      { name: 'text: @RestController',            layer: 'text',    fn: p => runTextSearch(p, '@RestController') },
      { name: 'text: @Autowired',                 layer: 'text',    fn: p => runTextSearch(p, '@Autowired') },
      { name: 'ast: class declarations (java)',    layer: 'ast',     fn: p => runStructural(p, 'class $CLASS { $$$BODY }', 'java') },
      { name: 'ast: method declarations (java)',   layer: 'ast',     fn: p => runStructural(p, 'public $RET $NAME($$$ARGS) { $$$BODY }', 'java') },
      {
        name: 'symbols: spring-boot-project/spring-boot/src/main/java/org/springframework/boot/SpringApplication.java',
        layer: 'symbols',
        fn: p => runDocumentSymbols(
          p,
          'spring-boot-project/spring-boot/src/main/java/org/springframework/boot/SpringApplication.java'
        ),
      },
    ],
  },
  chromium: {
    lang: 'C++',
    probes: [
      { name: 'text: #include in base/',        layer: 'text',    fn: p => runTextSearch(join(p, 'base'), '#include') },
      { name: 'text: CHECK macro',              layer: 'text',    fn: p => runTextSearch(join(p, 'base'), 'CHECK(') },
      { name: 'text: DCHECK macro',             layer: 'text',    fn: p => runTextSearch(join(p, 'base'), 'DCHECK(') },
      { name: 'ast: class declarations (cpp)',  layer: 'ast',     fn: p => runStructural(join(p, 'base'), 'class $CLASS { $$$BODY }', 'cpp') },
      { name: 'ast: function bodies (c)',       layer: 'ast',     fn: p => runStructural(join(p, 'base'), '$RET $NAME($$$ARGS) { $$$BODY }', 'c') },
      { name: 'symbols: base/logging.h',        layer: 'symbols', fn: p => runDocumentSymbols(p, 'base/logging.h') },
    ],
  },
  nextjs: {
    lang: 'JavaScript/TypeScript',
    probes: [
      { name: 'text: getServerSideProps',       layer: 'text',    fn: p => runTextSearch(p, 'getServerSideProps') },
      { name: 'text: useRouter',                layer: 'text',    fn: p => runTextSearch(p, 'useRouter') },
      { name: 'text: export default function',  layer: 'text',    fn: p => runTextSearch(p, 'export default function') },
      { name: 'ast: React components (tsx)',    layer: 'ast',     fn: p => runStructural(p, 'function $NAME($$$ARGS): $RET { $$$BODY }', 'tsx') },
      { name: 'ast: arrow components (tsx)',    layer: 'ast',     fn: p => runStructural(p, 'const $N = ($$$A): $R => { $$$B }', 'tsx') },
      { name: 'symbols: packages/next/src/server/next.ts', layer: 'symbols', fn: p => runDocumentSymbols(p, 'packages/next/src/server/next.ts') },
    ],
  },
};

// ── Markdown renderer ─────────────────────────────────────────────────────────

function renderResults(key, suite, probes, pins, totalMs) {
  const pin = pins[key] ?? {};
  const date = new Date().toISOString().slice(0, 10);
  const pass = probes.filter(p => p.r.ok).length;
  const total = probes.length;
  const score = `${pass}/${total}`;

  const rows = probes.map(p => {
    const icon = p.r.ok ? '✅' : '❌';
    const ms   = `${p.r.elapsed}ms`;
    const detail = p.r.detail ?? '';
    return `| ${icon} | ${p.name} | ${p.layer} | ${ms} | ${detail} |`;
  }).join('\n');

  return `# ${key} — Repo Benchmark Results

**Language**: ${suite.lang}
**Repo**: ${pin.url ?? 'unknown'}
**Tag**: ${pin.tag ?? 'HEAD'}
**SHA**: ${pin.sha ?? 'unknown'}
**Date**: ${date}
**Score**: ${score} probes passed  |  Total: ${totalMs}ms

## Probe Results

| Status | Probe | Layer | Time | Detail |
|--------|-------|-------|------|--------|
${rows}

## Layer Summary

| Layer | Pass | Fail |
|-------|------|------|
${['text','ast','symbols'].map(layer => {
  const layerProbes = probes.filter(p => p.layer === layer);
  const lPass = layerProbes.filter(p => p.r.ok).length;
  const lFail = layerProbes.length - lPass;
  return `| ${layer} | ${lPass} | ${lFail} |`;
}).join('\n')}

## Notes

${pass === total
  ? '> All probes passed — engine can handle this language/repo.'
  : `> ${total - pass} probe(s) failed. Check engine support, LSP server availability, and whether the sparse clone captured the needed files.`
}

_Generated by \`benchmark/repo/run.mjs\` on ${date}_
`;
}

// ── Main ──────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2).filter(a => !a.startsWith('--'));
const targets = args.length ? args : Object.keys(SUITES);

let pins = {};
try {
  pins = JSON.parse(readFileSync(PINS_FILE, 'utf8'));
} catch (e) {
  // Don't silently render "SHA: unknown" — surface that pins are unreadable.
  console.error(`warning: could not read ${PINS_FILE}: ${e.message} (results will show unknown pins)`);
}

const unknown = targets.filter(k => !SUITES[k]);
if (unknown.length) {
  console.error(`Unknown repo keys: ${unknown.join(', ')}`);
  console.error(`Available: ${Object.keys(SUITES).join(', ')}`);
  process.exit(1);
}

let allPass = true;

for (const key of targets) {
  const repoPath = join(TARGET, key);
  if (!existsSync(join(repoPath, '.git'))) {
    console.log(`\n── ${key}: NOT CLONED — run clone.mjs first ──`);
    allPass = false;
    continue;
  }

  const suite = SUITES[key];
  console.log(`\n── ${key} (${suite.lang}) ──`);

  const suiteStart = Date.now();
  const probes = [];

  for (const probe of suite.probes) {
    process.stdout.write(`  ${probe.name} ... `);
    const r = await probe.fn(repoPath);
    process.stdout.write(`${r.ok ? '✓' : '✗'} ${r.detail} (${r.elapsed}ms)\n`);
    probes.push({ name: probe.name, layer: probe.layer, r });
  }

  const totalMs = Date.now() - suiteStart;
  const pass = probes.filter(p => p.r.ok).length;
  console.log(`  → ${pass}/${probes.length} passed in ${totalMs}ms`);

  const md = renderResults(key, suite, probes, pins, totalMs);
  const outPath = join(RESULTS, key, 'results.md');
  mkdirSync(join(RESULTS, key), { recursive: true });
  writeFileSync(outPath, md);
  console.log(`  → written: ${outPath}`);

  if (pass < probes.length) allPass = false;
}

console.log('\n── Done ──');
if (!allPass) process.exit(1);
