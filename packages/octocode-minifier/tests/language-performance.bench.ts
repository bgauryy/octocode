import { bench, describe } from 'vitest';
import {
  applyContentViewMinification,
  minifyContentSync,
} from '@octocodeai/octocode-minifier';
import { buildLanguageBenchmarkCases } from './languageBenchmarkFixtures.js';

describe('language performance benchmark — all configured extensions', () => {
  for (const testCase of buildLanguageBenchmarkCases()) {
    bench(
      `${testCase.ext}: content-view + sync`,
      () => {
        applyContentViewMinification(testCase.content, testCase.filePath);
        minifyContentSync(testCase.content, testCase.filePath);
      },
      { time: 20, warmupTime: 5 }
    );
  }
});
