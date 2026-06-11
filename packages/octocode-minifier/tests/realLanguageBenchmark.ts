import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import {
  applyContentViewMinification,
  extractSignatures,
  getExtension,
  MINIFY_CONFIG,
  minifyContent,
  minifyContentSync,
} from '@octocodeai/octocode-minifier';

export const REAL_BENCHMARK_ROOTS_ENV = 'OCTOCODE_MINIFIER_REAL_BENCH_ROOTS';

export const COMMON_REAL_LANGUAGE_EXTENSIONS: readonly string[] = Object.freeze(
  Object.keys(MINIFY_CONFIG.fileTypes).sort()
);

export type RealFileBenchmarkOptions = {
  readonly roots: readonly string[];
  readonly extensions?: readonly string[];
  readonly maxFilesPerExtension?: number;
  readonly maxBytesPerFile?: number;
  readonly minBytesPerFile?: number;
};

export type RealFileSample = {
  readonly ext: string;
  readonly path: string;
  readonly relativePath: string;
  readonly content: string;
  readonly bytes: number;
};

export type RealFileMetric = {
  readonly ext: string;
  readonly path: string;
  readonly bytes: number;
  readonly contentViewBytes: number;
  readonly syncBytes: number;
  readonly contentViewSavingsPercent: number;
  readonly syncSavingsPercent: number;
  readonly signatureBytes: number | null;
  readonly signatureSavingsPercent: number | null;
};

export type RealFileAsyncMetric = RealFileMetric & {
  readonly asyncBytes: number;
  readonly asyncSavingsPercent: number;
  readonly asyncFailed: boolean;
};

export type RealLanguageSummary = {
  readonly ext: string;
  readonly files: number;
  readonly bytes: number;
  readonly contentViewSavingsPercent: number;
  readonly syncSavingsPercent: number;
  readonly asyncSavingsPercent: number | null;
  readonly asyncFailures: number | null;
  readonly signatureSavingsPercent: number | null;
};

const SKIPPED_DIRECTORY_NAMES = new Set([
  '.git',
  '.octocode',
  'node_modules',
  'dist',
  'out',
  'coverage',
  '.next',
  'target',
  'build',
  '.gradle',
]);

function isSupportedExtension(ext: string): boolean {
  return Object.prototype.hasOwnProperty.call(MINIFY_CONFIG.fileTypes, ext);
}

export function realBenchmarkRootsFromEnv(
  envValue = process.env[REAL_BENCHMARK_ROOTS_ENV]
): readonly string[] {
  if (!envValue) return [];

  return envValue
    .split(',')
    .map(root => root.trim())
    .filter(root => root.length > 0)
    .map(root => resolve(root));
}

function shouldSkipDirectory(name: string): boolean {
  return SKIPPED_DIRECTORY_NAMES.has(name);
}

function savingsPercent(originalBytes: number, outputBytes: number): number {
  if (originalBytes === 0) return 0;
  return ((originalBytes - outputBytes) / originalBytes) * 100;
}

function sampleLimitReached(
  samplesByExt: ReadonlyMap<string, number>,
  ext: string,
  maxFilesPerExtension: number
): boolean {
  return (samplesByExt.get(ext) ?? 0) >= maxFilesPerExtension;
}

function collectFileSample(
  filePath: string,
  rootPath: string,
  options: Required<Omit<RealFileBenchmarkOptions, 'roots'>>
): RealFileSample | null {
  const ext = getExtension(filePath, { lowercase: true, fallback: '' });
  if (!options.extensions.includes(ext) || !isSupportedExtension(ext)) {
    return null;
  }

  const fileStats = statSync(filePath);
  if (
    fileStats.size < options.minBytesPerFile ||
    fileStats.size > options.maxBytesPerFile
  ) {
    return null;
  }

  const content = readFileSync(filePath, 'utf8');
  if (content.trim().length === 0) return null;

  return {
    ext,
    path: filePath,
    relativePath: relative(rootPath, filePath),
    content,
    bytes: Buffer.byteLength(content, 'utf8'),
  };
}

export function discoverRealFileSamples(
  options: RealFileBenchmarkOptions
): readonly RealFileSample[] {
  const resolvedOptions: Required<Omit<RealFileBenchmarkOptions, 'roots'>> = {
    extensions: options.extensions ?? COMMON_REAL_LANGUAGE_EXTENSIONS,
    maxFilesPerExtension: options.maxFilesPerExtension ?? 3,
    maxBytesPerFile: options.maxBytesPerFile ?? 256_000,
    minBytesPerFile: options.minBytesPerFile ?? 64,
  };
  const samples: RealFileSample[] = [];
  const samplesByExt = new Map<string, number>();

  const visit = (entryPath: string, rootPath: string): void => {
    const entryStats = statSync(entryPath);
    if (entryStats.isDirectory()) {
      const name = entryPath.split(/[\\/]/).pop() ?? '';
      if (shouldSkipDirectory(name)) return;

      for (const child of readdirSync(entryPath).sort()) {
        visit(resolve(entryPath, child), rootPath);
      }
      return;
    }

    if (!entryStats.isFile()) return;

    const ext = getExtension(entryPath, { lowercase: true, fallback: '' });
    if (
      sampleLimitReached(
        samplesByExt,
        ext,
        resolvedOptions.maxFilesPerExtension
      )
    ) {
      return;
    }

    const sample = collectFileSample(entryPath, rootPath, resolvedOptions);
    if (!sample) return;

    samples.push(sample);
    samplesByExt.set(ext, (samplesByExt.get(ext) ?? 0) + 1);
  };

  for (const root of options.roots.map(root => resolve(root))) {
    if (!existsSync(root)) continue;
    visit(root, root);
  }

  return samples.sort((a, b) =>
    a.ext === b.ext
      ? a.relativePath.localeCompare(b.relativePath)
      : a.ext.localeCompare(b.ext)
  );
}

export function measureRealFileSample(sample: RealFileSample): RealFileMetric {
  const contentView = applyContentViewMinification(sample.content, sample.path);
  const sync = minifyContentSync(sample.content, sample.path);
  const signatures = extractSignatures(sample.content, sample.path);
  const contentViewBytes = Buffer.byteLength(contentView, 'utf8');
  const syncBytes = Buffer.byteLength(sync, 'utf8');
  const signatureBytes =
    signatures === null ? null : Buffer.byteLength(signatures, 'utf8');

  return {
    ext: sample.ext,
    path: sample.relativePath,
    bytes: sample.bytes,
    contentViewBytes,
    syncBytes,
    contentViewSavingsPercent: savingsPercent(sample.bytes, contentViewBytes),
    syncSavingsPercent: savingsPercent(sample.bytes, syncBytes),
    signatureBytes,
    signatureSavingsPercent:
      signatureBytes === null
        ? null
        : savingsPercent(sample.bytes, signatureBytes),
  };
}

export async function measureRealFileSampleWithAsync(
  sample: RealFileSample
): Promise<RealFileAsyncMetric> {
  const syncMetric = measureRealFileSample(sample);
  const asyncResult = await minifyContent(sample.content, sample.path);
  const asyncBytes = Buffer.byteLength(asyncResult.content, 'utf8');

  return {
    ...syncMetric,
    asyncBytes,
    asyncSavingsPercent: savingsPercent(sample.bytes, asyncBytes),
    asyncFailed: asyncResult.failed,
  };
}

function hasAsyncMetric(
  metric: RealFileMetric | RealFileAsyncMetric
): metric is RealFileAsyncMetric {
  return 'asyncBytes' in metric;
}

export function summarizeRealFileMetrics(
  metrics: readonly (RealFileMetric | RealFileAsyncMetric)[]
): readonly RealLanguageSummary[] {
  const byExt = new Map<string, (RealFileMetric | RealFileAsyncMetric)[]>();
  for (const metric of metrics) {
    byExt.set(metric.ext, [...(byExt.get(metric.ext) ?? []), metric]);
  }

  return [...byExt.entries()]
    .map(([ext, extMetrics]) => {
      const bytes = extMetrics.reduce((sum, metric) => sum + metric.bytes, 0);
      const contentViewBytes = extMetrics.reduce(
        (sum, metric) => sum + metric.contentViewBytes,
        0
      );
      const syncBytes = extMetrics.reduce(
        (sum, metric) => sum + metric.syncBytes,
        0
      );
      const asyncMetrics = extMetrics.filter(hasAsyncMetric);
      const asyncBytes =
        asyncMetrics.length === 0
          ? null
          : asyncMetrics.reduce((sum, metric) => sum + metric.asyncBytes, 0);
      const asyncFailures =
        asyncMetrics.length === 0
          ? null
          : asyncMetrics.filter(metric => metric.asyncFailed).length;
      const signatureMetrics = extMetrics.filter(
        metric => metric.signatureBytes !== null
      );
      const signatureBytes =
        signatureMetrics.length === 0
          ? null
          : signatureMetrics.reduce(
              (sum, metric) => sum + (metric.signatureBytes ?? 0),
              0
            );

      return {
        ext,
        files: extMetrics.length,
        bytes,
        contentViewSavingsPercent: savingsPercent(bytes, contentViewBytes),
        syncSavingsPercent: savingsPercent(bytes, syncBytes),
        asyncSavingsPercent:
          asyncBytes === null ? null : savingsPercent(bytes, asyncBytes),
        asyncFailures,
        signatureSavingsPercent:
          signatureBytes === null
            ? null
            : savingsPercent(bytes, signatureBytes),
      };
    })
    .sort((a, b) => a.ext.localeCompare(b.ext));
}

export function formatRealLanguageSummary(
  summaries: readonly RealLanguageSummary[]
): string {
  const header =
    '| ext | files | bytes | content-view cut | sync cut | async cut | async failed | symbols cut |';
  const divider = '|---|---:|---:|---:|---:|---:|---:|---:|';
  const rows = summaries.map(summary => {
    const asyncSavings =
      summary.asyncSavingsPercent === null
        ? 'n/a'
        : `${summary.asyncSavingsPercent.toFixed(1)}%`;
    const asyncFailures =
      summary.asyncFailures === null ? 'n/a' : `${summary.asyncFailures}`;
    const symbols =
      summary.signatureSavingsPercent === null
        ? 'n/a'
        : `${summary.signatureSavingsPercent.toFixed(1)}%`;
    return `| ${summary.ext} | ${summary.files} | ${summary.bytes} | ${summary.contentViewSavingsPercent.toFixed(1)}% | ${summary.syncSavingsPercent.toFixed(1)}% | ${asyncSavings} | ${asyncFailures} | ${symbols} |`;
  });

  return [header, divider, ...rows].join('\n');
}
