import { getConfigSync, DEFAULT_OUTPUT_CONFIG } from '@octocodeai/config';

export function getOutputCharLimit(): number {
  try {
    return getConfigSync().output.pagination.defaultCharLength;
  } catch {
    return DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  }
}
