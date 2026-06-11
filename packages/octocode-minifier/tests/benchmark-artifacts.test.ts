import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { MINIFY_CONFIG } from '@octocodeai/octocode-minifier';

type BenchmarkSummary = {
  configuredExtensions: number;
  coveredExtensions: number;
  missingExtensions: string[];
  asyncTypeDistribution: Record<
    string,
    { count: number; extensions: string[] }
  >;
  commonLanguageTypes: Array<{
    ext: string;
    strategy: string;
    asyncType: string;
    applyCut: number;
    asyncCut: number;
  }>;
  quality: {
    averageAgent: number;
    averageMinify: number;
    averageSymbolsWhenSupported: number;
    symbolsSupported: number;
    symbolsReturned: number;
    weakest: Array<{ ext: string; agent: number }>;
  };
  metrics: Array<{ ext: string }>;
};

const benchmarkRoot = new URL('../benchmark/', import.meta.url);
const summaryPath = new URL('summary.json', benchmarkRoot);

const IGNORED_FORMAT_EXTENSIONS = [
  'sol',
  'cu',
  'cuh',
  'qml',
  'gleam',
  'cr',
  'raku',
  'rakumod',
  'rakutest',
  'apex',
  'cls',
  'trigger',
  'd',
  'ml',
  'mli',
  'sml',
  're',
  'rei',
  'm',
] as const;

const EXPECTED_COMMON_ASYNC_TYPES = {
  js: 'terser',
  cjs: 'terser',
  mjs: 'terser',
  jsx: 'terser',
  ts: 'conservative',
  tsx: 'conservative',
  json: 'json',
  jsonc: 'json',
  css: 'aggressive',
  scss: 'aggressive',
  html: 'aggressive',
  vue: 'aggressive',
  svelte: 'aggressive',
  py: 'conservative',
  java: 'conservative',
  go: 'conservative',
  rs: 'conservative',
  sql: 'conservative',
  md: 'markdown',
} as const;

function readSummary(): BenchmarkSummary {
  return JSON.parse(readFileSync(summaryPath, 'utf8')) as BenchmarkSummary;
}

describe('benchmark artifacts', () => {
  it('stays aligned with the configured minifier extension matrix', () => {
    const summary = readSummary();
    const configuredExtensions = Object.keys(MINIFY_CONFIG.fileTypes).sort();
    const measuredExtensions = summary.metrics.map(metric => metric.ext).sort();

    expect(summary.configuredExtensions).toBe(configuredExtensions.length);
    expect(summary.coveredExtensions).toBe(summary.metrics.length);
    expect(summary.coveredExtensions).toBe(measuredExtensions.length);
    expect(new Set(measuredExtensions).size).toBe(measuredExtensions.length);
    expect(summary.missingExtensions).toEqual(
      configuredExtensions.filter(ext => !measuredExtensions.includes(ext))
    );
  });

  it('keeps required benchmark permutations for each measured language', () => {
    const summary = readSummary();

    for (const { ext } of summary.metrics) {
      const languageDir = join(benchmarkRoot.pathname, ext);
      expect(existsSync(join(languageDir, 'raw', 'source.excerpt.txt'))).toBe(
        true
      );
      expect(
        existsSync(join(languageDir, 'minified', 'content-view.excerpt.txt'))
      ).toBe(true);
      expect(
        existsSync(
          join(languageDir, 'minified', 'apply-minification.excerpt.txt')
        )
      ).toBe(true);
      expect(
        existsSync(
          join(languageDir, 'minified', 'minify-content-sync.excerpt.txt')
        )
      ).toBe(true);
      expect(
        existsSync(
          join(languageDir, 'minified', 'minify-content-async.excerpt.txt')
        )
      ).toBe(true);
      expect(existsSync(join(languageDir, 'symbol', 'signatures.txt'))).toBe(
        true
      );
    }
  });

  it('keeps benchmark JSON valid and agent-quality floors explicit', () => {
    const summary = readSummary();
    let jsonFiles = 0;

    function visit(directory: string): void {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) {
          visit(path);
          continue;
        }
        if (!entry.name.endsWith('.json')) continue;
        JSON.parse(readFileSync(path, 'utf8'));
        jsonFiles++;
      }
    }

    visit(benchmarkRoot.pathname);

    expect(jsonFiles).toBeGreaterThan(100);
    expect(summary.coveredExtensions).toBeGreaterThanOrEqual(40);
    expect(summary.quality.averageAgent).toBeGreaterThanOrEqual(8);
    expect(summary.quality.averageMinify).toBeGreaterThanOrEqual(8);
    expect(summary.quality.averageSymbolsWhenSupported).toBeGreaterThanOrEqual(
      9
    );
    expect(summary.quality.symbolsReturned).toBe(
      summary.quality.symbolsSupported
    );
    expect(summary.asyncTypeDistribution.terser.count).toBeGreaterThan(0);
    expect(summary.asyncTypeDistribution.conservative.count).toBeGreaterThan(0);
    expect(summary.asyncTypeDistribution.aggressive.count).toBeGreaterThan(0);
  });

  it('keeps ignored formats out of configured and benchmarked support', () => {
    const summary = readSummary();
    const measuredExtensions = new Set(
      summary.metrics.map(metric => metric.ext)
    );

    for (const ext of IGNORED_FORMAT_EXTENSIONS) {
      expect(MINIFY_CONFIG.fileTypes[ext], ext).toBeUndefined();
      expect(measuredExtensions.has(ext), ext).toBe(false);
      expect(summary.missingExtensions.includes(ext), ext).toBe(false);
    }
  });

  it('keeps weakest benchmark list sorted by agent score', () => {
    const weakestScores = readSummary().quality.weakest.map(
      metric => metric.agent
    );

    expect(weakestScores).toEqual([...weakestScores].sort((a, b) => a - b));
  });

  it('documents competitor framing for benchmark scores', () => {
    const readme = readFileSync(new URL('README.md', benchmarkRoot), 'utf8');

    expect(readme).toContain('Competitor Baseline');
    expect(readme).toContain('Terser');
    expect(readme).toContain('esbuild');
    expect(readme).toContain('SWC');
    expect(readme).toContain('Lightning CSS');
    expect(readme).toContain('html-minifier-terser');
    expect(readme).toContain('agent-context compressor');
  });

  it('documents real minification types for JS, TS, and common languages', () => {
    const summary = readSummary();
    const readme = readFileSync(new URL('README.md', benchmarkRoot), 'utf8');
    const commonTypes = new Map(
      summary.commonLanguageTypes.map(metric => [metric.ext, metric])
    );

    expect(readme).toContain('Real Minification Type Matrix');
    expect(readme).toContain('Configured strategy');
    expect(readme).toContain('Async type');

    for (const [ext, asyncType] of Object.entries(
      EXPECTED_COMMON_ASYNC_TYPES
    )) {
      expect(commonTypes.get(ext)?.asyncType, ext).toBe(asyncType);
      expect(commonTypes.get(ext)?.asyncCut, ext).toBeGreaterThanOrEqual(0);
    }

    expect(commonTypes.get('ts')?.strategy).toBe('conservative');
    expect(commonTypes.get('tsx')?.strategy).toBe('conservative');
    expect(commonTypes.get('jsx')?.strategy).toBe('terser');
  });
});
