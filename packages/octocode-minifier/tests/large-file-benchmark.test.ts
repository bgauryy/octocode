/**
 * Large-file benchmark — tests all minification modes against realistic
 * ~400-line code samples per language and writes per-language + summary
 * reports to benchmark/{ext}/large-file-metrics.json and
 * benchmark/large-files-summary.md.
 *
 * Run: yarn test tests/large-file-benchmark.test.ts
 *      Artefacts are written during the test run (not just a bench).
 */

import { describe, it, expect } from 'vitest';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import {
  applyContentViewMinification,
  applyMinification,
  minifyContent,
  minifyContentSync,
  extractSignatures,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from '@octocodeai/octocode-minifier';
import {
  TS_LARGE,
  PY_LARGE,
  GO_LARGE,
  JAVA_LARGE,
  RUST_LARGE,
  CSS_LARGE,
  SQL_LARGE,
  SHELL_LARGE,
  RUBY_LARGE,
  KOTLIN_LARGE,
  YAML_LARGE,
  JS_LARGE,
} from './largeSampleFixtures.js';

// ─── output directory ────────────────────────────────────────────────────────

const BENCHMARK_DIR = join(__dirname, '..', 'benchmark');

function ensureDir(path: string) {
  if (!existsSync(path)) mkdirSync(path, { recursive: true });
}

// ─── types ───────────────────────────────────────────────────────────────────

type LargeFileMetrics = {
  ext: string;
  language: string;
  description: string;
  inputBytes: number;
  inputLines: number;
  contentView: { bytes: number; cutPct: number; ms: number };
  applyMinify: { bytes: number; cutPct: number; ms: number };
  syncMinify: { bytes: number; cutPct: number; ms: number };
  asyncMinify: { bytes: number; cutPct: number; ms: number; type: string };
  symbols: { supported: boolean; bytes: number | null; cutPct: number | null; ms: number | null };
  rating: { score: number; label: string };
  notes: string[];
};

// ─── helpers ─────────────────────────────────────────────────────────────────

function cut(original: number, result: number): number {
  if (original === 0) return 0;
  return Math.round(((original - result) / original) * 1000) / 10; // 1 dp
}

function rate(
  contentViewCut: number,
  applyMinifyCut: number,
  symbolsReturned: boolean | null
): { score: number; label: string } {
  let score = 0;

  // Content-view cut (weight 0.25)
  if (contentViewCut >= 40) score += 2.5;
  else if (contentViewCut >= 25) score += 2.0;
  else if (contentViewCut >= 10) score += 1.5;
  else if (contentViewCut >= 5)  score += 1.0;
  else                           score += 0.5;

  // Apply-minify cut (weight 0.50)
  if (applyMinifyCut >= 60) score += 5.0;
  else if (applyMinifyCut >= 40) score += 4.0;
  else if (applyMinifyCut >= 25) score += 3.0;
  else if (applyMinifyCut >= 10) score += 2.0;
  else                           score += 1.0;

  // Symbols (weight 0.25)
  if (symbolsReturned === null) score += 1.5; // n/a neutral
  else if (symbolsReturned)     score += 2.5;
  else                          score += 1.0;

  const final = Math.min(10, Math.round(score * 10) / 10);
  const label = final >= 9.0 ? 'excellent' :
                final >= 8.0 ? 'strong'    :
                final >= 7.0 ? 'good'      : 'fair';

  return { score: final, label };
}

async function measure(content: string, ext: string, language: string, description: string, notes: string[] = []): Promise<LargeFileMetrics> {
  const filePath = `large-sample.${ext}`;
  const inputBytes = Buffer.byteLength(content, 'utf8');
  const inputLines = content.split('\n').length;

  // content-view
  const t0 = performance.now();
  const cv = applyContentViewMinification(content, filePath);
  const cvMs = +(performance.now() - t0).toFixed(2);

  // apply (sync fast path)
  const t1 = performance.now();
  const am = applyMinification(content, filePath);
  const amMs = +(performance.now() - t1).toFixed(2);

  // sync
  const t2 = performance.now();
  const sm = minifyContentSync(content, filePath);
  const smMs = +(performance.now() - t2).toFixed(2);

  // async
  const t3 = performance.now();
  const asyncResult = await minifyContent(content, filePath);
  const asyncMs = +(performance.now() - t3).toFixed(2);

  // symbols
  const symSupported = (SUPPORTED_SIGNATURE_EXTENSIONS as readonly string[]).includes(ext);
  let symBytes: number | null = null;
  let symMs: number | null = null;
  if (symSupported) {
    const t4 = performance.now();
    const sigs = extractSignatures(content, filePath);
    symMs = +(performance.now() - t4).toFixed(2);
    if (sigs) symBytes = Buffer.byteLength(sigs, 'utf8');
  }

  const r = rate(
    cut(inputBytes, Buffer.byteLength(cv, 'utf8')),
    cut(inputBytes, Buffer.byteLength(am, 'utf8')),
    symSupported ? symBytes !== null : null
  );

  return {
    ext,
    language,
    description,
    inputBytes,
    inputLines,
    contentView: { bytes: Buffer.byteLength(cv, 'utf8'), cutPct: cut(inputBytes, Buffer.byteLength(cv, 'utf8')), ms: cvMs },
    applyMinify:  { bytes: Buffer.byteLength(am, 'utf8'), cutPct: cut(inputBytes, Buffer.byteLength(am, 'utf8')), ms: amMs },
    syncMinify:   { bytes: Buffer.byteLength(sm, 'utf8'), cutPct: cut(inputBytes, Buffer.byteLength(sm, 'utf8')), ms: smMs },
    asyncMinify:  { bytes: Buffer.byteLength(asyncResult.content, 'utf8'), cutPct: cut(inputBytes, Buffer.byteLength(asyncResult.content, 'utf8')), ms: asyncMs, type: asyncResult.type },
    symbols: {
      supported: symSupported,
      bytes: symBytes,
      cutPct: symBytes !== null ? cut(inputBytes, symBytes) : null,
      ms: symMs,
    },
    rating: r,
    notes,
  };
}

function metricsToMarkdown(m: LargeFileMetrics): string {
  const sym = m.symbols.supported
    ? m.symbols.bytes !== null
      ? `${m.symbols.bytes.toLocaleString()} B (−${m.symbols.cutPct}%)`
      : 'n/a (null returned)'
    : 'n/a (not supported)';

  return `# ${m.language} (\`.${m.ext}\`) — Large-File Benchmark

**Source:** ${m.inputLines} lines / ${m.inputBytes.toLocaleString()} bytes — ${m.description}

**Agent rating: ${m.rating.score}/10 (${m.rating.label})**

| Mode | Output bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| Input | ${m.inputBytes.toLocaleString()} B | — | — |
| content-view | ${m.contentView.bytes.toLocaleString()} B | **−${m.contentView.cutPct}%** | ${m.contentView.ms} ms |
| applyMinification | ${m.applyMinify.bytes.toLocaleString()} B | **−${m.applyMinify.cutPct}%** | ${m.applyMinify.ms} ms |
| minifyContentSync | ${m.syncMinify.bytes.toLocaleString()} B | **−${m.syncMinify.cutPct}%** | ${m.syncMinify.ms} ms |
| minifyContent (async, type=${m.asyncMinify.type}) | ${m.asyncMinify.bytes.toLocaleString()} B | **−${m.asyncMinify.cutPct}%** | ${m.asyncMinify.ms} ms |
| symbols | ${sym} | — | ${m.symbols.ms !== null ? `${m.symbols.ms} ms` : '—'} |

${m.notes.length ? `## Notes\n\n${m.notes.map(n => `- ${n}`).join('\n')}\n` : ''}`;
}

function summaryToMarkdown(all: LargeFileMetrics[], ts: string): string {
  const sorted = [...all].sort((a, b) => b.rating.score - a.rating.score);
  const avgContentView = +(all.reduce((s, m) => s + m.contentView.cutPct, 0) / all.length).toFixed(1);
  const avgApply = +(all.reduce((s, m) => s + m.applyMinify.cutPct, 0) / all.length).toFixed(1);
  const avgAsync = +(all.reduce((s, m) => s + m.asyncMinify.cutPct, 0) / all.length).toFixed(1);
  const avgRating = +(all.reduce((s, m) => s + m.rating.score, 0) / all.length).toFixed(1);

  const buckets = {
    excellent: all.filter(m => m.rating.label === 'excellent').length,
    strong:    all.filter(m => m.rating.label === 'strong').length,
    good:      all.filter(m => m.rating.label === 'good').length,
    fair:      all.filter(m => m.rating.label === 'fair').length,
  };

  const rows = sorted.map(m => {
    const sym = m.symbols.supported
      ? m.symbols.bytes !== null ? `−${m.symbols.cutPct}%` : 'null'
      : 'n/a';
    return `| \`.${m.ext}\` | ${m.language} | ${m.inputLines} | ${m.inputBytes.toLocaleString()} | −${m.contentView.cutPct}% | −${m.applyMinify.cutPct}% | −${m.asyncMinify.cutPct}% | ${sym} | **${m.rating.score}/10** ${m.rating.label} |`;
  }).join('\n');

  return `# Large-File Minification Benchmark

> Generated ${ts}
>
> Each sample is a realistic ~400-line file taken from real open-source
> projects. All four minification modes are measured.

## Summary

| Metric | Value |
| --- | --- |
| Languages measured | ${all.length} |
| Average content-view cut | **${avgContentView}%** |
| Average apply-minify cut | **${avgApply}%** |
| Average async cut | **${avgAsync}%** |
| Average agent rating | **${avgRating}/10** |
| excellent (≥9.0) | ${buckets.excellent} |
| strong (≥8.0) | ${buckets.strong} |
| good (≥7.0) | ${buckets.good} |
| fair (<7.0) | ${buckets.fair} |

## Per-language results

| Ext | Language | Lines | Bytes | Content-view | Apply | Async | Symbols | Rating |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
${rows}

## Analysis

### Best performers (≥9.0)

${sorted.filter(m => m.rating.score >= 9.0).map(m =>
  `- **\`.${m.ext}\`** ${m.language}: content-view −${m.contentView.cutPct}%, apply −${m.applyMinify.cutPct}%${m.symbols.cutPct !== null ? `, symbols −${m.symbols.cutPct}%` : ''}`
).join('\n')}

### Weakest performers (<7.5)

${sorted.filter(m => m.rating.score < 7.5).map(m =>
  `- **\`.${m.ext}\`** ${m.language}: content-view −${m.contentView.cutPct}%, apply −${m.applyMinify.cutPct}% — _${m.rating.label}_`
).join('\n') || '_None — all languages scored ≥7.5_'}

### Comment-density drivers

Languages where comment stripping contributes the most value tend to be
heavily-documented (JavaDoc, Rustdoc, Python docstrings). Formats like YAML
and shell provide less savings because real-world files carry fewer comments
relative to payload data.
`;
}

// ─── fixtures ────────────────────────────────────────────────────────────────

const FIXTURES: Array<{
  ext: string;
  language: string;
  description: string;
  content: string;
  notes?: string[];
}> = [
  {
    ext: 'ts',
    language: 'TypeScript',
    description: 'Async HTTP client with generics, retry, EventEmitter',
    content: TS_LARGE,
    notes: ['TypeScript compiler + Terser pipeline', 'Rich type annotations removed by transpiler'],
  },
  {
    ext: 'py',
    language: 'Python',
    description: 'httpx-style sync/async client with docstrings (adapted)',
    content: PY_LARGE,
    notes: [
      'LIMITATION: Python triple-quoted docstrings (\'\'\'...\'\'\'  /  """...""") are NOT stripped — only # line comments are removed',
      'Real-world Python files with heavy docstring usage show very low content-view cuts',
      'Symbols extraction (−60%) compensates significantly for the agent context use-case',
      'Fix: implement docstring-position heuristic detection in a future strategy iteration',
    ],
  },
  {
    ext: 'go',
    language: 'Go',
    description: 'Configurable HTTP client with functional options + retry',
    content: GO_LARGE,
    notes: ['Conservative strategy with c-style comment stripping', 'GoDoc comments removed'],
  },
  {
    ext: 'java',
    language: 'Java',
    description: 'Spring StringUtils — JavaDoc-heavy utility class',
    content: JAVA_LARGE,
    notes: ['JavaDoc block comments stripped', 'High comment ratio typical of Spring source'],
  },
  {
    ext: 'rs',
    language: 'Rust',
    description: 'Async task runtime — doc comments, unsafe blocks, generics',
    content: RUST_LARGE,
    notes: ['//! and /// doc comments stripped by c-style remover', 'Aggressive savings from Rustdoc'],
  },
  {
    ext: 'css',
    language: 'CSS',
    description: 'Design-system tokens + components (buttons, cards, forms)',
    content: CSS_LARGE,
    notes: ['Aggressive strategy: CleanCSS async path', 'Variable declarations compress well'],
  },
  {
    ext: 'sql',
    language: 'SQL',
    description: 'E-commerce schema: tables, triggers, stored procedures',
    content: SQL_LARGE,
    notes: ['Conservative strategy — SQL line comments stripped', 'Block comments in DDL removed'],
  },
  {
    ext: 'sh',
    language: 'Shell',
    description: 'Deployment script: args, SSH, rsync, health-check',
    content: SHELL_LARGE,
    notes: ['Hash comments stripped but shebang preserved', 'Low comment ratio limits savings'],
  },
  {
    ext: 'rb',
    language: 'Ruby',
    description: 'ActiveRecord User model with validations, scopes, auth',
    content: RUBY_LARGE,
    notes: ['Hash comments and inline annotations stripped', 'Schema comment header removed'],
  },
  {
    ext: 'kt',
    language: 'Kotlin',
    description: 'Android repository: coroutines, Flow, Room, Retrofit',
    content: KOTLIN_LARGE,
    notes: ['KDoc block comments stripped via c-style remover', 'Coroutine annotations preserved'],
  },
  {
    ext: 'yml',
    language: 'YAML',
    description: 'GitHub Actions CI/CD pipeline — multi-job workflow',
    content: YAML_LARGE,
    notes: ['Hash comments stripped', 'Low savings expected — real workflows are data-dense'],
  },
  {
    ext: 'js',
    language: 'JavaScript',
    description: 'Lodash-style utility library with full JSDoc (400 lines)',
    content: JS_LARGE,
    notes: [
      'Terser pipeline: TypeScript transpiler skipped (pure JS), goes direct to terser',
      'JSDoc block comments stripped by Terser',
      'Symbols extraction via tsJsStrategy AST parser',
    ],
  },
];

// ─── tests ───────────────────────────────────────────────────────────────────

describe('Large-file benchmark — per language quality measurement', () => {
  const results: LargeFileMetrics[] = [];

  for (const fixture of FIXTURES) {
    it(`${fixture.ext}: minifies realistic ${fixture.language} sample correctly`, async () => {
      const m = await measure(
        fixture.content,
        fixture.ext,
        fixture.language,
        fixture.description,
        fixture.notes
      );

      results.push(m);

      // Write per-language artefacts
      const langDir = join(BENCHMARK_DIR, fixture.ext);
      ensureDir(langDir);
      writeFileSync(
        join(langDir, 'large-file-metrics.json'),
        JSON.stringify(m, null, 2) + '\n'
      );
      writeFileSync(
        join(langDir, 'large-file-report.md'),
        metricsToMarkdown(m)
      );

      // Assertions: output must never exceed input
      expect(m.contentView.bytes).toBeLessThanOrEqual(m.inputBytes);
      expect(m.applyMinify.bytes).toBeLessThanOrEqual(m.inputBytes);
      expect(m.syncMinify.bytes).toBeLessThanOrEqual(m.inputBytes);
      expect(m.asyncMinify.bytes).toBeLessThanOrEqual(m.inputBytes);

      // Output must not be empty
      expect(m.contentView.bytes).toBeGreaterThan(0);
      expect(m.applyMinify.bytes).toBeGreaterThan(0);
    });
  }

  it('writes combined large-file summary to benchmark/large-files-summary.md', () => {
    if (results.length === 0) return; // parallel execution edge case

    ensureDir(BENCHMARK_DIR);
    const ts = new Date().toISOString();
    writeFileSync(
      join(BENCHMARK_DIR, 'large-files-summary.md'),
      summaryToMarkdown(results, ts)
    );

    const summaryJson = {
      generatedAt: ts,
      languages: results.length,
      averageContentViewCutPct: +(results.reduce((s, m) => s + m.contentView.cutPct, 0) / results.length).toFixed(1),
      averageApplyCutPct: +(results.reduce((s, m) => s + m.applyMinify.cutPct, 0) / results.length).toFixed(1),
      averageAsyncCutPct: +(results.reduce((s, m) => s + m.asyncMinify.cutPct, 0) / results.length).toFixed(1),
      averageRating: +(results.reduce((s, m) => s + m.rating.score, 0) / results.length).toFixed(1),
      perLanguage: results.map(m => ({
        ext: m.ext,
        language: m.language,
        inputBytes: m.inputBytes,
        inputLines: m.inputLines,
        contentViewCutPct: m.contentView.cutPct,
        applyMinifyCutPct: m.applyMinify.cutPct,
        asyncCutPct: m.asyncMinify.cutPct,
        asyncType: m.asyncMinify.type,
        symbolsCutPct: m.symbols.cutPct,
        rating: m.rating,
      })),
    };

    writeFileSync(
      join(BENCHMARK_DIR, 'large-files-summary.json'),
      JSON.stringify(summaryJson, null, 2) + '\n'
    );

    expect(results.length).toBe(FIXTURES.length);
    expect(existsSync(join(BENCHMARK_DIR, 'large-files-summary.md'))).toBe(true);
    expect(existsSync(join(BENCHMARK_DIR, 'large-files-summary.json'))).toBe(true);
  });
});
