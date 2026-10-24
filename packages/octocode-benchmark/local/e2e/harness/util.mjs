// Small helpers the harness suites and benchmark scripts share.
import { createHash } from 'node:crypto';

/** SHA-256 hex of a string or Buffer, or of a value's JSON. */
export const hash = value => createHash('sha256').update(typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value)).digest('hex');
/** Source lines with their newlines; a last line without one is kept. */
export const records = text => text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
/** Length in Unicode code points. */
export const chars = text => Array.from(text).length;
export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

/** Median of the finite values, rounded to `digits` decimals (null when there are none). */
export function median(values, digits) {
  const sorted = values.filter(x => x != null && Number.isFinite(x)).toSorted((a, b) => a - b);
  if (!sorted.length) return null;
  const mid = Math.floor(sorted.length / 2);
  return +(sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2).toFixed(digits);
}
