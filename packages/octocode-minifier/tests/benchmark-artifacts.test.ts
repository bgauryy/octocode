import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { MINIFY_CONFIG } from '@octocodeai/octocode-minifier';

type BenchmarkSummary = {
  configuredExtensions: number;
  coveredExtensions: number;
  missingExtensions: string[];
  readmeMinification: {
    source: string;
    inputBytes: number;
    outputBytes: number;
    cutPercent: number;
    readabilityScore: number;
    byteReductionScore: number;
    score: number;
    label: string;
    signals: Array<{ name: string; passed: boolean }>;
  } | null;
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
  agentUnderstanding: {
    averageScore: number;
    buckets: Record<string, number>;
    weakest: Array<{ ext: string; score: number }>;
  };
  agentObservations: {
    overallAgentUsefulness: number;
    levels: Record<
      'none' | 'standard' | 'minify' | 'symbols',
      {
        count: number;
        averageScore: number;
        averageCut: number;
        buckets: Record<string, number>;
        weakest: Array<{ ext: string; score: number }>;
      }
    >;
  };
  metrics: Array<{
    ext: string;
    agentUnderstanding: {
      output: string;
      score: number;
      label: string;
      syntaxAnchors: { score: number; hits: number; total: number };
      structure: number;
      outputHealth: number;
      contextBudget: number;
      symbols: number;
      signals: Array<{ name: string; passed: boolean }>;
    };
    agentObservations: Record<
      'none' | 'standard' | 'minify',
      {
        output: string;
        bytes: number;
        cutPercent: number;
        score: number;
        label: string;
        syntaxAnchors: { score: number; hits: number; total: number };
        structure: number;
        outputHealth: number;
        contextBudget: number;
        symbols: number;
        signals: Array<{ name: string; passed: boolean }>;
      }
    > & {
      symbols: {
        output: string;
        bytes: number;
        cutPercent: number;
        score: number;
        label: string;
        syntaxAnchors: { score: number; hits: number; total: number };
        structure: number;
        outputHealth: number;
        contextBudget: number;
        symbols: number;
        signals: Array<{ name: string; passed: boolean }>;
      } | null;
    };
  }>;
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

const REQUESTED_AGENT_UNDERSTANDING_EXTENSIONS = [
  'py',
  'c',
  'cpp',
  'java',
  'cs',
  'js',
  'vb',
  'sql',
  'html',
  'jsx',
  'ts',
  'tsx',
] as const;

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

  it('documents real README minification rating', () => {
    const summary = readSummary();
    const readme = readFileSync(new URL('README.md', benchmarkRoot), 'utf8');
    const rating = summary.readmeMinification;

    if (rating === null) {
      throw new Error('Expected real README minification rating');
    }

    expect(rating.source).toContain('md/');
    expect(rating.inputBytes).toBeGreaterThan(0);
    expect(rating.outputBytes).toBeGreaterThan(0);
    expect(rating.outputBytes).toBeLessThanOrEqual(rating.inputBytes);
    expect(rating.readabilityScore).toBeGreaterThanOrEqual(8);
    expect(rating.byteReductionScore).toBeGreaterThan(0);
    expect(rating.score).toBeGreaterThanOrEqual(6);
    expect(rating.signals.every(signal => signal.passed)).toBe(true);
    expect(readme).toContain('Real README Minification Rating');
    expect(readme).toContain('Readability preservation');
    expect(readme).toContain('Byte reduction');
  });

  it('documents agent understanding quality for every minified output', () => {
    const summary = readSummary();
    const readme = readFileSync(new URL('README.md', benchmarkRoot), 'utf8');
    const metricsByExtension = new Map(
      summary.metrics.map(metric => [metric.ext, metric])
    );

    expect(summary.agentUnderstanding.averageScore).toBeGreaterThanOrEqual(7);
    expect(summary.agentUnderstanding.weakest.length).toBeGreaterThan(0);
    expect(readme).toContain(
      'Agent Understanding Quality From Minified Output'
    );
    expect(readme).toContain('Syntax anchors');
    expect(readme).toContain('Symbol context');

    for (const metric of summary.metrics) {
      const understanding = metric.agentUnderstanding;
      const nonEmptySignal = understanding.signals.find(
        signal => signal.name === 'standard output is non-empty'
      );

      expect(understanding.output, metric.ext).toBe('standard');
      expect(understanding.score, metric.ext).toBeGreaterThan(0);
      expect(understanding.score, metric.ext).toBeLessThanOrEqual(10);
      expect(understanding.syntaxAnchors.total, metric.ext).toBeGreaterThan(0);
      expect(
        understanding.syntaxAnchors.hits,
        metric.ext
      ).toBeGreaterThanOrEqual(0);
      expect(understanding.structure, metric.ext).toBeGreaterThanOrEqual(0);
      expect(understanding.outputHealth, metric.ext).toBeGreaterThanOrEqual(0);
      expect(understanding.contextBudget, metric.ext).toBeGreaterThan(0);
      expect(understanding.symbols, metric.ext).toBeGreaterThan(0);
      expect(nonEmptySignal?.passed, metric.ext).toBe(true);
    }

    for (const ext of REQUESTED_AGENT_UNDERSTANDING_EXTENSIONS) {
      expect(
        metricsByExtension.get(ext)?.agentUnderstanding.score,
        ext
      ).toBeGreaterThanOrEqual(6);
    }
  });

  it('documents agent observations for every output level', () => {
    const summary = readSummary();
    const readme = readFileSync(new URL('README.md', benchmarkRoot), 'utf8');

    expect(
      summary.agentObservations.overallAgentUsefulness
    ).toBeGreaterThanOrEqual(8);
    expect(summary.agentObservations.levels.none.averageScore).toBe(10);
    expect(summary.agentObservations.levels.none.averageCut).toBe(0);
    expect(summary.agentObservations.levels.standard.count).toBe(
      summary.metrics.length
    );
    expect(summary.agentObservations.levels.minify.count).toBe(
      summary.metrics.length
    );
    expect(summary.agentObservations.levels.symbols.count).toBe(
      summary.quality.symbolsReturned
    );
    expect(readme).toContain('Agent Observation By Output Level');
    expect(readme).toContain('| none |');
    expect(readme).toContain('| standard |');
    expect(readme).toContain('| minify |');
    expect(readme).toContain('| symbols |');

    for (const metric of summary.metrics) {
      expect(metric.agentObservations.none.output, metric.ext).toBe('none');
      expect(metric.agentObservations.none.score, metric.ext).toBe(10);
      expect(metric.agentObservations.standard.output, metric.ext).toBe(
        'standard'
      );
      expect(metric.agentObservations.minify.output, metric.ext).toBe('minify');
      expect(
        metric.agentObservations.standard.score,
        metric.ext
      ).toBeGreaterThan(0);
      expect(metric.agentObservations.minify.score, metric.ext).toBeGreaterThan(
        0
      );
    }
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
