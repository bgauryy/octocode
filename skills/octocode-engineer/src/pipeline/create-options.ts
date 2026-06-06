import path from 'node:path';

import { ALL_CATEGORIES, PILLAR_CATEGORIES } from '../types/index.js';

import type { AnalysisOptions } from '../types/index.js';

export interface CreateOptionsInput {
  args: AnalysisOptions;
}


export function createOptions({ args }: CreateOptionsInput): AnalysisOptions {
  const opts = { ...args };

  opts.packageRoot = path.join(opts.root, 'packages');
  autoEnableTestQuality(opts);

  return opts;
}

function autoEnableTestQuality(opts: AnalysisOptions): void {
  if (opts.features === null) return;

  const testQualityCats = new Set(PILLAR_CATEGORIES['test-quality']);
  if ([...opts.features].some(f => testQualityCats.has(f))) {
    opts.includeTests = true;
  }
}


export function resolveExcludeToFeatures(
  excludeSet: Set<string>
): Set<string> {
  return new Set([...ALL_CATEGORIES].filter(c => !excludeSet.has(c)));
}

export class OptionsError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'OptionsError';
  }
}
