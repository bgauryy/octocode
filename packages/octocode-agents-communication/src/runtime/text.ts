/** Pure string helpers shared across the runtime: code-point counting, ordering and truncation, whitespace and UTF-8 bounds. */

/** String ordering by code point (JavaScript compares UTF-16 units, which differs for astral characters). */
export function compareCodePoints(a: string, b: string): number {
  const left = a[Symbol.iterator]();
  const right = b[Symbol.iterator]();
  for (;;) {
    const x = left.next();
    const y = right.next();
    if (x.done || y.done) return x.done === y.done ? 0 : x.done ? -1 : 1;
    const delta = x.value.codePointAt(0)! - y.value.codePointAt(0)!;
    if (delta) return delta;
  }
}

/** The first `count` code points. */
export function truncateCodePoints(text: string, count: number): string {
  return text.length <= count ? text : Array.from(text).slice(0, count).join('');
}

/** The number of code points; a lone surrogate counts as one. */
export function codePointLength(text: string): number {
  let count = 0;
  for (const _ of text) count += 1;
  return count;
}

/** Unicode whitespace as the catalog contract defines it, as the inside of a character class: it includes U+001C–U+001F and U+0085 and excludes U+FEFF. */
export const WHITESPACE_CLASS = '\\t-\\r\\x1c-\\x20\\x85\\xa0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000';
const UNICODE_BLANK = new RegExp(`^[${WHITESPACE_CLASS}]*$`);

/**
 * A regular expression source with `\s` and `\S` meaning the catalog's whitespace, not JavaScript's (which keeps U+FEFF and
 * lacks U+001C–U+001F and U+0085). `\S` inside a character class has no such rewrite and is refused.
 */
export function catalogWhitespace(source: string): string {
  let result = '';
  let inClass = false;
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index]!;
    if (character === '\\') {
      const next = source[(index += 1)] ?? '';
      if (next === 's') result += inClass ? WHITESPACE_CLASS : `[${WHITESPACE_CLASS}]`;
      else if (next === 'S') {
        if (inClass) throw new Error('\\S inside a character class');
        result += `[^${WHITESPACE_CLASS}]`;
      } else result += character + next;
      continue;
    }
    if (character === '[') inClass = true;
    else if (character === ']') inClass = false;
    result += character;
  }
  return result;
}

/** True when `text` is empty or only Unicode whitespace. */
export function isBlank(text: string): boolean {
  return UNICODE_BLANK.test(text);
}
