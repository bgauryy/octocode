import type { ParsedArgs } from './types.js';

type Options = ParsedArgs['options'];

export function getBool(opts: Options, ...keys: string[]): boolean {
  return keys.some(k => Boolean(opts[k]));
}

export function getString(opts: Options, ...keys: string[]): string {
  for (const k of keys) {
    const v = opts[k];
    if (typeof v === 'string') return v;
  }
  return '';
}

export function resolveHostname(opts: Options): string {
  const v = opts['hostname'];
  return (
    (typeof v === 'string' && v.length > 0 ? v : undefined) ?? 'github.com'
  );
}
