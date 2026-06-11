import { describe, expect, it } from 'vitest';
import {
  COMMON_REAL_LANGUAGE_EXTENSIONS,
  discoverRealFileSamples,
  formatRealLanguageSummary,
  measureRealFileSampleWithAsync,
  realBenchmarkRootsFromEnv,
  summarizeRealFileMetrics,
} from './realLanguageBenchmark.js';

const roots = realBenchmarkRootsFromEnv();
const samples = discoverRealFileSamples({
  roots,
  extensions: COMMON_REAL_LANGUAGE_EXTENSIONS,
  maxFilesPerExtension: 3,
});

describe('real language cut report', () => {
  const runWhenRealCorpusExists = samples.length > 0 ? it : it.skip;

  runWhenRealCorpusExists(
    'measures real files without increasing content size',
    async () => {
      const metrics = await Promise.all(
        samples.map(measureRealFileSampleWithAsync)
      );
      const summaries = summarizeRealFileMetrics(metrics);

      console.info(`\n${formatRealLanguageSummary(summaries)}\n`);

      expect(samples.length).toBeGreaterThan(0);
      for (const metric of metrics) {
        expect(
          metric.contentViewBytes,
          `${metric.ext}:${metric.path} content-view`
        ).toBeLessThanOrEqual(metric.bytes);
        expect(
          metric.syncBytes,
          `${metric.ext}:${metric.path} sync`
        ).toBeLessThanOrEqual(metric.bytes);
        expect(
          metric.asyncBytes,
          `${metric.ext}:${metric.path} async`
        ).toBeLessThanOrEqual(metric.bytes);
      }
    }
  );
});
