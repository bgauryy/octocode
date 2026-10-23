/**
 * Byte-stable JSON text and strict UTF-8 decoding.
 *
 * `dumps` writes JSON text byte for byte as the persisted format defines it for every option combination in use (ASCII or
 * Unicode escapes, spacing, key order), because some of its output is hashed, compared or stored.
 * Numbers that are whole are printed as integers: JSON cannot tell `5.0` from `5`.
 */
import { ValidationError } from './errors.js';
import { compareCodePoints } from './text.js';

const STRICT_UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/** The text of `bytes` as strict UTF-8: invalid UTF-8 throws and a leading U+FEFF is kept (a `TextDecoder` drops it by default). */
export function decodeUtf8(bytes: Uint8Array): string {
  return STRICT_UTF8.decode(bytes);
}

/** Parses JSON from strict UTF-8 bytes; one leading byte order mark is tolerated. */
export function parseJsonBytes(bytes: Uint8Array): unknown {
  const marked = bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
  return JSON.parse(decodeUtf8(marked ? bytes.subarray(3) : bytes));
}

interface DumpsOptions {
  /** Escape every character outside printable ASCII as \uXXXX (the default). */
  ensureAscii?: boolean;
  /** Item and key separators; the default is `', '` and `': '`. */
  separators?: readonly [string, string];
  sortKeys?: boolean;
  /** Emit NaN and Infinity (the default) instead of failing. */
  allowNan?: boolean;
}

/** Compact separators, characters kept as written, NaN refused: how stored bodies, hashes and wire frames are written. */
export const COMPACT: DumpsOptions = { ensureAscii: false, separators: [',', ':'], allowNan: false };

/** `dumps` with `COMPACT`. */
export function compact(value: unknown): string {
  return dumps(value, COMPACT);
}

/** Compact separators with every non-ASCII character escaped: MCP binding text and ID lists. */
export function compactAscii(value: unknown): string {
  return dumps(value, { separators: [',', ':'] });
}

/** The shortest round-trip text of a float, with an exponent outside 1e-4..1e16; safe integers are printed as integers instead. */
function formatFloat(value: number): string {
  const [mantissa = '', exponentText = '0'] = value.toExponential().split('e');
  const exponent = Number(exponentText);
  if (exponent >= -4 && exponent < 16) return String(value);
  const digits = Math.abs(exponent);
  return `${mantissa}e${exponent < 0 ? '-' : '+'}${digits < 10 ? '0' : ''}${digits}`;
}

function formatNumber(value: number, allowNan: boolean): string {
  if (!Number.isFinite(value)) {
    if (!allowNan) throw new ValidationError('Out of range float values are not JSON compliant');
    return Number.isNaN(value) ? 'NaN' : value > 0 ? 'Infinity' : '-Infinity';
  }
  if (Number.isSafeInteger(value)) return Object.is(value, -0) ? '0' : String(value);
  return formatFloat(value);
}

/** JSON text of `value` per `options`. Object properties that are `undefined` are omitted, as JSON.stringify does. */
export function dumps(value: unknown, { ensureAscii = true, separators = [', ', ': '], sortKeys = false, allowNan = true }: DumpsOptions = {}): string {
  const [itemSeparator, keySeparator] = separators;
  const text = (string: string): string => {
    const quoted = JSON.stringify(string);
    return ensureAscii ? quoted.replace(/[^\x20-\x7e]/g, character => '\\u' + character.charCodeAt(0).toString(16).padStart(4, '0')) : quoted;
  };
  const write = (item: unknown, inArray: boolean): string => {
    if (item === null || (item === undefined && inArray)) return 'null';
    switch (typeof item) {
      case 'string': return text(item);
      case 'boolean': return item ? 'true' : 'false';
      case 'number': return formatNumber(item, allowNan);
      case 'bigint': return item.toString();
      case 'object': break;
      default: throw new TypeError(`Object of type ${typeof item} is not JSON serializable`);
    }
    if (Array.isArray(item)) return '[' + item.map(element => write(element, true)).join(itemSeparator) + ']';
    if (Object.getPrototypeOf(item) !== Object.prototype && Object.getPrototypeOf(item) !== null) throw new TypeError('Object is not JSON serializable');
    const defined = Object.entries(item).filter(([, member]) => member !== undefined);
    const entries = sortKeys ? defined.toSorted(([a], [b]) => compareCodePoints(a, b)) : defined;
    return '{' + entries.map(([key, member]) => text(key) + keySeparator + write(member, false)).join(itemSeparator) + '}';
  };
  if (value === undefined) throw new TypeError('Object of type undefined is not JSON serializable');
  return write(value, false);
}

/** One wire frame without its newline: compact UTF-8 JSON. */
export function encode(value: unknown): Buffer {
  return Buffer.from(compact(value));
}
