// The TypeScript runtime and the golden outputs the golden-vector tests (paths-validation-catalog, wire, database) compare it with. fixtures/foundation-golden.json holds
// what the previous runtime produced for the same inputs, captured once.
import { readFileSync } from 'node:fs';
import type { JsonRecord } from './json-record.ts';

export const runtime = await import('../dist/runtime.js');
export const golden: JsonRecord = JSON.parse(readFileSync(new URL('./fixtures/foundation-golden.json', import.meta.url), 'utf8'));

/** Expands the `$DIR` and `$BASE` placeholders of a golden case for the scratch directory `directory`. */
export const expand = (directory: string, value: string) => value.replaceAll('$DIR', directory).replaceAll('$BASE', directory.split('/').pop() ?? '');
/** The inverse for results: the scratch directory becomes `$DIR`. */
export const contract = (directory: string, value: unknown): JsonRecord => JSON.parse(JSON.stringify(value).replaceAll(JSON.stringify(directory).slice(1, -1), '$DIR'));
