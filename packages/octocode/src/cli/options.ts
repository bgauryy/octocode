import type { ParsedArgs } from './types.js';

type Options = ParsedArgs['options'];

export function getBool(opts: Options, key: string): boolean {
  return Boolean(opts[key]);
}

export function getString(opts: Options, key: string): string {
  const value = opts[key];
  return typeof value === 'string' ? value : '';
}
