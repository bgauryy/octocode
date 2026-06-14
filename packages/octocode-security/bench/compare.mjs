#!/usr/bin/env node
/**
 * compare.mjs — Head-to-head benchmark: octocode-security (Rust) vs octocode-security-utils (TS)
 *
 * Usage:
 *   node bench/compare.mjs
 *   node bench/compare.mjs --runs 2000 --warmup 200
 */

import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { writeFileSync, mkdirSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// Parse CLI args
// ---------------------------------------------------------------------------
const args = process.argv.slice(2);
const getArg = (name, def) => {
  const idx = args.indexOf(name);
  return idx !== -1 ? Number(args[idx + 1]) : def;
};
const RUNS = getArg('--runs', 1000);
const WARMUP = getArg('--warmup', 100);

// ---------------------------------------------------------------------------
// Load both implementations
// ---------------------------------------------------------------------------
const pkg     = await import('../dist/index.js');
const native  = await import('../dist/native.js');

// ---------------------------------------------------------------------------
// Rust implementation — goes through NAPI bridge
// ---------------------------------------------------------------------------
const rustSanitize = (s) => pkg.ContentSanitizer.sanitizeContent(s);
const rustMask     = (s) => pkg.maskSensitiveData(s);

// ---------------------------------------------------------------------------
// Pure-JS baseline — same generated patterns applied in JavaScript, no Rust
// This is the real comparison: V8 regex engine vs Rust regex crate
// ---------------------------------------------------------------------------
const { allRegexPatterns } = pkg;

function jsSanitizeContent(content) {
  if (!content || typeof content !== 'string') {
    return { content: content ?? '', hasSecrets: false, secretsDetected: [], warnings: [] };
  }
  let sanitized = content;
  const secrets = [];
  for (const p of allRegexPatterns) {
    if (p.fileContext) continue;          // skip file-context-only patterns
    p.regex.lastIndex = 0;
    if (p.regex.test(sanitized)) {
      secrets.push(p.name);
      p.regex.lastIndex = 0;
      sanitized = sanitized.replace(p.regex, `[REDACTED-${p.name.toUpperCase()}]`);
    }
    p.regex.lastIndex = 0;
  }
  const hasSecrets = secrets.length > 0;
  return { content: sanitized, hasSecrets, secretsDetected: secrets,
           warnings: hasSecrets ? [`${secrets.length} secret(s) redacted`] : [] };
}

function jsMaskSensitiveData(text) {
  if (!text) return text;
  let result = text;
  for (const p of allRegexPatterns) {
    if (p.fileContext) continue;
    p.regex.lastIndex = 0;
    result = result.replace(p.regex, (match) => {
      let masked = '';
      for (let i = 0; i < match.length; i++) masked += i % 2 === 0 ? '*' : match[i];
      return masked;
    });
    p.regex.lastIndex = 0;
  }
  return result;
}

const tsSanitize = jsSanitizeContent;   // pure JS — real baseline
const tsMask     = jsMaskSensitiveData;

console.log(`\n🔬  octocode-security benchmark  (Rust/NAPI  vs  pure-JS/V8)`);
console.log(`   Patterns loaded: ${native.nativePatternCount()} (Rust/native loader)  /  ${allRegexPatterns.filter(p => !p.fileContext).length} applicable (JS)`);
console.log(`   Note: "TS" = pure-JS V8 regex engine,  "Rust" = NAPI bridge to Rust regex crate`);
console.log(`   Runs: ${RUNS}  Warmup: ${WARMUP}\n`);

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------
const secrets = {
  awsKey:       'AKIAIOSFODNN7EXAMPLE',
  ghToken:      'ghp_16C7e42F292c6912E7710c838347Ae178B4a',
  openaiKey:    'sk-proj-abcdefghijklmnopqrstuvwxyz1234567890ABCDEF',
  jwtToken:     'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c',
  stripeKey:    'sk_live_4eC39HqLyjWDarjtT1zdp7dc4eC39HqLyjWDarjtT1zdp7dc',
};

function makePayload(size, withSecrets = false) {
  const clean = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. ';
  let base = clean.repeat(Math.ceil(size / clean.length)).slice(0, size);
  if (withSecrets) {
    // Embed 5 different secrets at evenly spaced positions
    const entries = Object.values(secrets);
    const step = Math.floor(size / (entries.length + 1));
    for (let i = 0; i < entries.length; i++) {
      const pos = step * (i + 1);
      base = base.slice(0, pos) + ` TOKEN=${entries[i]} ` + base.slice(pos);
    }
  }
  return base;
}

const SIZES = [
  { label: '100B',   size: 100 },
  { label: '1KB',    size: 1_000 },
  { label: '10KB',   size: 10_000 },
  { label: '100KB',  size: 100_000 },
  { label: '500KB',  size: 500_000 },
];

// Sizes that exercise the chunked path (detect_chunked, content > 500 000 chars).
// The Rust implementation runs REGEX_SET once on the full content to pre-filter
// candidate patterns, so clean large payloads should early-return in near-zero
// time.  These benchmarks validate that optimisation.
const CHUNKED_SIZES = [
  { label: '600KB',  size: 600_000  },
  { label: '1MB',    size: 1_000_000 },
  { label: '2MB',    size: 2_000_000 },
  { label: '5MB',    size: 5_000_000 },
];

// ---------------------------------------------------------------------------
// Measurement helpers
// ---------------------------------------------------------------------------
function measure(fn, runs) {
  const times = new Array(runs);
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    fn();
    times[i] = performance.now() - t0;
  }
  times.sort((a, b) => a - b);
  const sum = times.reduce((a, b) => a + b, 0);
  return {
    mean: sum / runs,
    p50:  times[Math.floor(runs * 0.50)],
    p95:  times[Math.floor(runs * 0.95)],
    p99:  times[Math.floor(runs * 0.99)],
    min:  times[0],
    max:  times[runs - 1],
  };
}

function fmt(n) { return n.toFixed(3).padStart(8); }
function speedup(ts, rust) {
  const r = ts / rust;
  if (r >= 1) return `${r.toFixed(1)}x faster`;
  return `${(1/r).toFixed(1)}x slower`;
}

// ---------------------------------------------------------------------------
// Run benchmarks
// ---------------------------------------------------------------------------
const results = [];

function header(title) {
  console.log(`\n${'─'.repeat(90)}`);
  console.log(`  ${title}`);
  console.log(`${'─'.repeat(90)}`);
  console.log(`  ${'Payload'.padEnd(10)} ${'Impl'.padEnd(6)} ${'mean(ms)'.padStart(10)} ${'p50'.padStart(10)} ${'p95'.padStart(10)} ${'p99'.padStart(10)}`);
  console.log(`  ${'-'.repeat(56)}`);
}

function runSuite(label, payloads, fnTs, fnRust) {
  header(label);
  for (const { label: szLabel, payload } of payloads) {
    // Warmup
    for (let i = 0; i < WARMUP; i++) { fnTs(payload); fnRust(payload); }

    const tsStats   = measure(() => fnTs(payload),   RUNS);
    const rustStats = measure(() => fnRust(payload), RUNS);

    const row = { suite: label, size: szLabel, ts: tsStats, rust: rustStats };
    results.push(row);

    const winner = speedup(tsStats.p50, rustStats.p50);
    console.log(`  ${szLabel.padEnd(10)} ${'TS'.padEnd(6)} ${fmt(tsStats.mean)} ${fmt(tsStats.p50)} ${fmt(tsStats.p95)} ${fmt(tsStats.p99)}`);
    console.log(`  ${szLabel.padEnd(10)} ${'Rust'.padEnd(6)} ${fmt(rustStats.mean)} ${fmt(rustStats.p50)} ${fmt(rustStats.p95)} ${fmt(rustStats.p99)}  ← ${winner}`);
  }
}

// -- sanitizeContent (clean, no secrets) — single path (< 500KB) --
const cleanPayloads = SIZES.map(s => ({ label: s.label, payload: makePayload(s.size, false) }));
runSuite('sanitizeContent — clean input (single path, < 500KB)', cleanPayloads, tsSanitize, rustSanitize);

// -- sanitizeContent (with secrets) — single path --
const dirtyPayloads = SIZES.map(s => ({ label: s.label, payload: makePayload(s.size, true) }));
runSuite('sanitizeContent — 5 embedded secrets (single path, < 500KB)', dirtyPayloads, tsSanitize, rustSanitize);

// -- maskSensitiveData --
const maskPayloads = SIZES.map(s => ({ label: s.label, payload: makePayload(s.size, true) }));
runSuite('maskSensitiveData', maskPayloads, tsMask, rustMask);

// ---------------------------------------------------------------------------
// Chunked path benchmarks (content > 500 000 chars → detect_chunked)
// ---------------------------------------------------------------------------
// Clean content: the REGEX_SET pre-filter should early-return, making the
// chunked path nearly as fast as the single path on clean data.
// With secrets: the pre-filter narrows to only matching patterns, then
// processes only those across chunks.

const CHUNKED_RUNS = Math.min(RUNS, 100); // fewer runs — payloads are large

function runChunkedSuite(label, payloads, fnTs, fnRust) {
  header(label);
  for (const { label: szLabel, payload } of payloads) {
    // Warmup (fewer iterations for large payloads)
    for (let i = 0; i < Math.min(WARMUP, 10); i++) { fnTs(payload); fnRust(payload); }

    const tsStats   = measure(() => fnTs(payload),   CHUNKED_RUNS);
    const rustStats = measure(() => fnRust(payload), CHUNKED_RUNS);

    const row = { suite: label, size: szLabel, ts: tsStats, rust: rustStats };
    results.push(row);

    const winner = speedup(tsStats.p50, rustStats.p50);
    console.log(`  ${szLabel.padEnd(10)} ${'TS'.padEnd(6)} ${fmt(tsStats.mean)} ${fmt(tsStats.p50)} ${fmt(tsStats.p95)} ${fmt(tsStats.p99)}`);
    console.log(`  ${szLabel.padEnd(10)} ${'Rust'.padEnd(6)} ${fmt(rustStats.mean)} ${fmt(rustStats.p50)} ${fmt(rustStats.p95)} ${fmt(rustStats.p99)}  ← ${winner}`);
  }
}

const chunkedCleanPayloads = CHUNKED_SIZES.map(s => ({ label: s.label, payload: makePayload(s.size, false) }));
runChunkedSuite('sanitizeContent — clean input (chunked path, > 500KB) — REGEX_SET pre-filter', chunkedCleanPayloads, tsSanitize, rustSanitize);

const chunkedDirtyPayloads = CHUNKED_SIZES.map(s => ({ label: s.label, payload: makePayload(s.size, true) }));
runChunkedSuite('sanitizeContent — 5 embedded secrets (chunked path, > 500KB)', chunkedDirtyPayloads, tsSanitize, rustSanitize);

// ---------------------------------------------------------------------------
// ReDoS adversarial test
// ---------------------------------------------------------------------------
console.log(`\n${'─'.repeat(90)}`);
console.log(`  ReDoS adversarial test  (100K repeated 'a' chars — must complete in <50ms)`);
console.log(`${'─'.repeat(90)}`);
const redosInput = 'a'.repeat(100_000);

const t0ts = performance.now();
tsSanitize(redosInput);
const tsRedos = performance.now() - t0ts;

const t0rs = performance.now();
rustSanitize(redosInput);
const rsRedos = performance.now() - t0rs;

console.log(`  TS   completed in: ${tsRedos.toFixed(2)}ms  ${tsRedos < 50 ? '✅' : '⚠️ SLOW'}`);
console.log(`  Rust completed in: ${rsRedos.toFixed(2)}ms  ${rsRedos < 50 ? '✅' : '⚠️ SLOW'}`);

// ---------------------------------------------------------------------------
// Correctness spot-check
// ---------------------------------------------------------------------------
console.log(`\n${'─'.repeat(90)}`);
console.log(`  Correctness spot-check`);
console.log(`${'─'.repeat(90)}`);

const testCases = [
  { input: `AWS key: AKIAIOSFODNN7EXAMPLE end`, expectSecret: true  },
  { input: `token: ghp_16C7e42F292c6912E7710c838347Ae178B4a end`, expectSecret: true  },
  { input: `Hello world, no secrets here.`, expectSecret: false },
  { input: `jwt: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.abc`, expectSecret: true  },
];

let pass = 0;
for (const tc of testCases) {
  const tsR  = tsSanitize(tc.input);
  const rsR  = rustSanitize(tc.input);
  const tsOk = tsR.hasSecrets === tc.expectSecret;
  const rsOk = rsR.hasSecrets === tc.expectSecret;
  const parity = tsR.hasSecrets === rsR.hasSecrets;
  const status = tsOk && rsOk ? '✅' : rsOk ? '⚠️ TS differs' : '❌ Rust wrong';
  const parityMark = parity ? '✓ parity' : '✗ DIVERGE';
  console.log(`  ${status.padEnd(16)} ${parityMark}  "${tc.input.slice(0, 50)}..."`);
  if (tsOk && rsOk) pass++;
}
console.log(`\n  Passed: ${pass}/${testCases.length}`);

// ---------------------------------------------------------------------------
// Save results
// ---------------------------------------------------------------------------
const outDir = join(__dirname, 'results');
mkdirSync(outDir, { recursive: true });
const outFile = join(outDir, `compare-${new Date().toISOString().split('T')[0]}.json`);
writeFileSync(outFile, JSON.stringify({ runs: RUNS, warmup: WARMUP, results }, null, 2));
console.log(`\n📄 Raw results saved to: ${outFile}\n`);
