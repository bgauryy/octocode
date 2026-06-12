import type { ParsedArgs } from './types.js';

type Options = ParsedArgs['options'];

/**
 * Read a boolean flag from parsed args, checking all provided key aliases.
 *
 * Usage: getBool(opts, 'json', 'j')
 */
export function getBool(opts: Options, ...keys: string[]): boolean {
  return keys.some(k => Boolean(opts[k]));
}

/**
 * Read a string option from parsed args, returning the first matching key's
 * value. Returns '' when absent or the stored value is not a string.
 *
 * Usage: getString(opts, 'path') or getString(opts, 'mode', 'm')
 */
export function getString(opts: Options, ...keys: string[]): string {
  for (const k of keys) {
    const v = opts[k];
    if (typeof v === 'string') return v;
  }
  return '';
}

/**
 * Resolve the GitHub hostname option, defaulting to 'github.com'.
 *
 * Usage: resolveHostname(opts)  →  'github.com' | '<enterprise-host>'
 */
export function resolveHostname(opts: Options): string {
  const v = opts['hostname'] ?? opts['H'];
  return (
    (typeof v === 'string' && v.length > 0 ? v : undefined) ?? 'github.com'
  );
}
