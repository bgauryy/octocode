import type { z } from 'zod';

import { RipgrepQuerySchema } from './schemas.js';

type RuntimeRipgrepQuery = z.infer<typeof RipgrepQuerySchema>;

export interface RipgrepValidationResult {
  isValid: boolean;
  errors: string[];
  warnings: string[];
}

export function validateRipgrepQuery(
  query: RuntimeRipgrepQuery
): RipgrepValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];
  if (!query || typeof query !== 'object') {
    errors.push('Query must be an object');
    return { isValid: false, errors, warnings };
  }
  if (
    query.mode !== 'structural' &&
    (!query.searchText || typeof query.searchText !== 'string')
  ) {
    errors.push('`searchText` is required');
  }
  if (query.matchWindow !== undefined && query.output !== 'matchOnly') {
    errors.push('`matchWindow` requires output:"matchOnly"');
  }
  if (
    (query.unique === 'list' || query.unique === 'count') &&
    query.output !== 'matchOnly'
  ) {
    errors.push('`unique` requires output:"matchOnly"');
  }
  return { isValid: errors.length === 0, errors, warnings };
}

export function applyWorkflowMode<T extends RuntimeRipgrepQuery>(query: T): T {
  if (!query || typeof query !== 'object') return query;
  const mode = query.mode;
  if (!mode || typeof mode !== 'string') return query;
  const next: T = { ...query };
  if (mode === 'discovery') {
    if (next.output === 'content') next.output = 'files';
  } else if (mode === 'detailed' && next.contextLines === undefined) {
    next.contextLines = 3;
  }
  return next;
}
