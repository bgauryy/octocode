import { bench, describe } from 'vitest';
import {
  discoverRealFileSamples,
  measureRealFileSample,
  realBenchmarkRootsFromEnv,
} from './realLanguageBenchmark.js';

const samples = discoverRealFileSamples({
  roots: realBenchmarkRootsFromEnv(),
  maxFilesPerExtension: 2,
});

describe('real language files benchmark', () => {
  if (samples.length === 0) {
    bench.skip(
      'no real files discovered; set OCTOCODE_MINIFIER_REAL_BENCH_ROOTS',
      () => {
        // skipped
      }
    );
    return;
  }

  for (const sample of samples) {
    bench(
      `${sample.ext}: ${sample.relativePath}`,
      () => {
        measureRealFileSample(sample);
      },
      { time: 20, warmupTime: 5 }
    );
  }
});
