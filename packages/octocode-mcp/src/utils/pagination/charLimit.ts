import { getConfigSync } from 'octocode-shared';
import { DEFAULT_OUTPUT_CONFIG } from 'octocode-shared';
import type { MinifyMode } from 'octocode-shared';

export function getOutputCharLimit(): number {
  try {
    return getConfigSync().output.pagination.defaultCharLength;
  } catch {
    return DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  }
}

export function getOutputMinifyDefault(): MinifyMode {
  try {
    return getConfigSync().output.defaultMinify;
  } catch {
    return DEFAULT_OUTPUT_CONFIG.defaultMinify;
  }
}

export const MAX_DEFAULT_OUTPUT_CHAR_LENGTH = 100_000;

export function getBulkDefaultCharLength(queryCount: number): number {
  const base = Math.min(
    Math.max(getOutputCharLimit(), 1),
    MAX_DEFAULT_OUTPUT_CHAR_LENGTH
  );
  const count = Math.max(Math.floor(queryCount) || 0, 1);
  return Math.min(base * count, MAX_DEFAULT_OUTPUT_CHAR_LENGTH);
}
