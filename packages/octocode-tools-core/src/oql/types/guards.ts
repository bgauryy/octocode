/**
 * OQL type guards — runtime discriminators for the input/output union types
 * (single query vs batch, single envelope vs batch envelope).
 */

import type {
  OqlBatch,
  OqlCanonicalInput,
  OqlInputBatch,
  OqlSearchInput,
} from './query.js';
import type { OqlBatchResultEnvelope, OqlRunResult } from './envelope.js';

export function isBatchInput(input: OqlSearchInput): input is OqlInputBatch {
  return Array.isArray((input as OqlInputBatch).queries);
}

export function isCanonicalBatch(input: OqlCanonicalInput): input is OqlBatch {
  return Array.isArray((input as OqlBatch).queries);
}

export function isBatchEnvelope(
  result: OqlRunResult
): result is OqlBatchResultEnvelope {
  return Array.isArray((result as OqlBatchResultEnvelope).children);
}
