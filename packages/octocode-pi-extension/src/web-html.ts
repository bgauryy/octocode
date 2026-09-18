/** HTML decode and text-extraction utilities used by web.ts. No external dependencies. */

const ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
  '#39': "'",
  '#x27': "'",
};

/** Valid Unicode code point range; String.fromCodePoint throws (RangeError) outside it. */
function codePointOrEntity(cp: number, original: string): string {
  return Number.isInteger(cp) && cp >= 0 && cp <= 0x10ffff
    ? String.fromCodePoint(cp)
    : original; // out-of-range (e.g. &#999999999999;) → leave the raw entity, never throw
}

export function decodeEntities(str: string): string {
  return str
    .replace(/&#(\d+);/g, (m, d: string) => codePointOrEntity(Number(d), m))
    .replace(/&#x([0-9a-f]+);/gi, (m, h: string) => codePointOrEntity(parseInt(h, 16), m))
    .replace(
      /&([a-z]+|#x?\w+);/gi,
      (m, name: string) => ENTITIES[name] ?? ENTITIES[name.toLowerCase()] ?? m,
    );
}

export function extractTitle(html: string): string {
  const m = html.match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  return m?.[1] ? decodeEntities(m[1]).replace(/\s+/g, ' ').trim() : '';
}

/** Strip a page to readable plain text: drop script/style/nav chrome, tags, collapse whitespace. */
export function htmlToText(html: string): string {
  const text = html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(
      /<(script|style|noscript|template|svg|nav|aside|footer)([\s>][\s\S]*?)<\/\1>/gi,
      ' ',
    )
    .replace(/<([a-z][a-z0-9]*(?:-[a-z0-9]+)+)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<(\w+)[^>]+\baria-label="announcement"[^>]*>[\s\S]*?<\/\1>/gi, ' ')
    .replace(
      /<(\w+)[^>]+\btypeof="BreadcrumbList"[^>]*>[\s\S]*?<\/\1>/gi,
      ' ',
    )
    .replace(/<a[^>]+href="#[^"]*"[^>]*>\s*Skip[^<]*<\/a\s*>/gi, ' ')
    .replace(/<\/(p|div|section|article|h[1-6]|li|tr|br|header)>/gi, '\n')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, ' ');
  return decodeEntities(text)
    .replace(/[ \t\f\v]+/g, ' ')
    .replace(/\n\s*\n\s*\n+/g, '\n\n')
    .split('\n')
    .map((l) => l.trim())
    .join('\n')
    .trim();
}
