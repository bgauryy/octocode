import { capChars } from '../shared/util.js';

/** Page metadata (JSON-LD, price and date meta tags) shown ahead of the text, capped. */
const METADATA_MAX_CHARS = 1_500;
/** Less visible text than this in the static HTML means the page is built by JavaScript. */
const SHELL_TEXT_CHARS = 500;

const ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
  lsquo: '‘',
  rsquo: '’',
  ldquo: '“',
  rdquo: '”',
  ndash: '–',
  mdash: '—',
  hellip: '…',
  middot: '·',
  bull: '•',
  copy: '©',
  reg: '®',
  trade: '™',
  times: '×',
  euro: '€',
  pound: '£',
  yen: '¥',
  cent: '¢',
};

export function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (match, code: string) => {
    if (code[0] === '#') {
      const point = code[1]?.toLowerCase() === 'x' ? parseInt(code.slice(2), 16) : parseInt(code.slice(1), 10);
      return Number.isFinite(point) && point > 0 && point <= 0x10ffff ? String.fromCodePoint(point) : match;
    }
    return ENTITIES[code.toLowerCase()] ?? match;
  });
}

/** Readable text from HTML: drops scripts/styles/chrome, keeps headings, links, list items and paragraphs. */
export function htmlToText(html: string): string {
  let text = stripHidden(
    html
      .replace(/<(script|style|noscript|svg|template|iframe)[\s\S]*?<\/\1>/gi, '')
      .replace(/<!--[\s\S]*?-->/g, ''),
  )
    // The title is extracted separately; nothing else in <head> is page text.
    .replace(/<head\b[\s\S]*?<\/head>/gi, '')
    .replace(/<(nav|footer|header|aside)\b[\s\S]*?<\/\1>/gi, '');
  text = text
    .replace(/<h([1-6])[^>]*>([\s\S]*?)<\/h\1>/gi, (_, level: string, inner: string) => `\n\n${'#'.repeat(Number(level))} ${inner}\n\n`)
    // href may be double-quoted, single-quoted or bare (`<a href=https://iana.org/help>` on example.com).
    .replace(
      /<a\b(?:[^>"']|"[^"]*"|'[^']*')*?\bhref=(?:"([^"#][^"]*)"|'([^'#][^']*)'|([^\s"'>#][^\s"'>]*))(?:[^>"']|"[^"]*"|'[^']*')*>([\s\S]*?)<\/a>/gi,
      (_, double: string | undefined, single: string | undefined, bare: string | undefined, inner: string) => `[${inner}](${double ?? single ?? bare})`,
    )
    .replace(/\[\s*\]\([^)]*\)/g, '')
    .replace(/<li\b[^>]*>/gi, '\n- ')
    .replace(/<(br|hr)\s*\/?>/gi, '\n')
    // Opening tags break too: HTML may omit `</p>`, and a block must never run into the text before it.
    .replace(/<\/?(p|div|section|article|tr|table|ul|ol|pre|blockquote)\b[^>]*>/gi, '\n\n')
    // Quoted attribute values may contain `>` (data-mw JSON on Wikipedia), so a tag ends at the first `>` outside quotes.
    .replace(/<(?:[^>"']|"[^"]*"|'[^']*')*>/g, '');
  return decodeEntities(text)
    .split('\n')
    .map((line) => line.replace(/[ \t]+/g, ' ').trim())
    // Empty list items (icon-only nav links) carry nothing.
    .filter((line) => line !== '-')
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

const VOID_TAGS = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr']);

/** True for an opening tag's attributes that hide it: `hidden`, `aria-hidden="true"`, or an inline `display:none` / `visibility:hidden`. */
function hiddenByAttributes(attributes: string): boolean {
  for (const match of attributes.matchAll(/([^\s=/>]+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s"'>]+))?/g)) {
    const name = match[1]!.toLowerCase();
    const value = (match[2] ?? '').replace(/^["']|["']$/g, '').toLowerCase();
    if (name === 'hidden' || (name === 'aria-hidden' && value === 'true')) return true;
    if (name === 'style' && /(?:^|;)\s*(?:display\s*:\s*none|visibility\s*:\s*hidden)/.test(value)) return true;
  }
  return false;
}

/**
 * Removes elements the page marks hidden (templates, placeholders, off-screen copies), so the text shows what a reader
 * would see; hidden text is also a common place for injected instructions.
 */
export function stripHidden(html: string): string {
  const open = /<([a-z][a-z0-9-]*)\b((?:[^>"']|"[^"]*"|'[^']*')*)>/gi;
  let out = '';
  let last = 0;
  for (let match = open.exec(html); match; match = open.exec(html)) {
    const attributes = match[2]!;
    if (!/hidden|none/i.test(attributes) || !hiddenByAttributes(attributes)) continue;
    const tag = match[1]!.toLowerCase();
    let end = open.lastIndex;
    if (!VOID_TAGS.has(tag) && !attributes.trimEnd().endsWith('/')) {
      const element = elementAt(html, match.index, tag);
      if (element) end = match.index + element.length;
    }
    out += html.slice(last, match.index);
    last = end;
    open.lastIndex = end;
  }
  return out + html.slice(last);
}

/**
 * The page's machine-readable facts: JSON-LD blocks and the meta tags for prices, availability, locale and dates.
 * Often the cleanest source for a product's price or an article's date, even when the visible text is filled in by
 * JavaScript. Empty when the page has none.
 */
export function pageMetadata(html: string, max = METADATA_MAX_CHARS): string {
  const lines = new Set<string>();
  for (const tag of html.match(/<meta\b[^>]*>/gi) ?? []) {
    const key = /\b(?:property|name|itemprop)=["']([^"']+)["']/i.exec(tag)?.[1];
    const content = /\bcontent=(?:"([^"]*)"|'([^']*)')/i.exec(tag);
    const value = content?.[1] ?? content?.[2];
    if (!key || !value?.trim()) continue;
    if (/^(og:(description|locale|price:amount|price:currency|updated_time)|product:(price:amount|price:currency|availability)|article:(published_time|modified_time)|price|pricecurrency|availability)$/i.test(key)) {
      lines.add(`${key}: ${decodeEntities(value).replace(/\s+/g, ' ').trim().slice(0, 300)}`);
    }
  }
  for (const match of html.matchAll(/<script\b[^>]*type=["']?application\/ld\+json["']?[^>]*>([\s\S]*?)<\/script>/gi)) {
    try {
      const data = JSON.parse(match[1]!.trim()) as unknown;
      lines.add(`JSON-LD: ${JSON.stringify(data, (key, value: unknown) => (key === '@context' ? undefined : value)).slice(0, 800)}`);
    } catch {
      // A malformed block is skipped: the page text still stands.
    }
  }
  if (lines.size === 0) return '';
  return capChars(`## Page metadata (from the HTML; untrusted)\n${[...lines].join('\n')}`, max, 'the rest is in the page source');
}

/** Markers of a page rendered or hydrated by a JavaScript framework. */
const APP_MARKERS = /__NEXT_DATA__|__NUXT__|window\.__INITIAL_STATE__|__APOLLO_STATE__|data-reactroot|ng-version=|wix-warmup-data|data-server-rendered|id=["']__next["']|id=["']root["']><\/div>|id=["']app["']><\/div>/;

/**
 * A warning when the static HTML may not be what a browser shows: little text (a JavaScript shell or a bot check), or
 * a JavaScript app whose values (prices, currency, stock) are filled in or localized after load.
 */
export function staticHtmlNote(html: string, text: string): string | undefined {
  const gate = /enable javascript|javascript is (?:disabled|required)|checking your browser|just a moment|verify you are human|cf-chl/i.test(html);
  if (text.length < SHELL_TEXT_CHARS && (gate || /<script\b/i.test(html))) {
    return `Note: the static HTML has little text${gate ? ' and asks for JavaScript or a bot check' : ''}; the page is probably built by JavaScript. Read it with the browser tool (navigate).`;
  }
  const scripts = html.match(/<script\b[\s\S]*?<\/script>/gi) ?? [];
  const scriptChars = scripts.reduce((sum, script) => sum + script.length, 0);
  if (APP_MARKERS.test(html) || scriptChars > html.length * 0.3 || scripts.length >= 25) {
    return 'Note: static HTML of a JavaScript app (no script ran). Values the browser fills in or localizes after load (prices, currency, stock, dates) may be missing or placeholders here: confirm them in the browser.';
  }
  return undefined;
}

/**
 * The HTML of the element that opens at `start` (`<tag …>`), through its matching close tag, counting nested elements
 * of the same tag; undefined when it never closes.
 */
function elementAt(html: string, start: number, tag: string): string | undefined {
  const pattern = new RegExp(`<(/?)${tag}\\b[^>]*>`, 'gi');
  pattern.lastIndex = start;
  let depth = 0;
  for (let match = pattern.exec(html); match; match = pattern.exec(html)) {
    depth += match[1] ? -1 : 1;
    if (depth === 0) return html.slice(start, pattern.lastIndex);
  }
  return undefined;
}

/**
 * The page's main content when it marks one (`<main>`, `role="main"`, or a single `<article>`) with real text in it,
 * so sidebars, menus and footers outside it cost no tokens; else the whole page.
 */
export function mainContent(html: string): string {
  const candidates: Array<{ at: number; tag: string }> = [];
  const main = /<main\b/i.exec(html);
  if (main) candidates.push({ at: main.index, tag: 'main' });
  const role = /<([a-z][a-z0-9]*)\b[^>]*\brole=["']?main\b/i.exec(html);
  if (role) candidates.push({ at: role.index, tag: role[1]! });
  const articles = html.match(/<article\b/gi) ?? [];
  if (articles.length === 1) candidates.push({ at: html.search(/<article\b/i), tag: 'article' });
  for (const { at, tag } of candidates) {
    const part = elementAt(html, at, tag);
    if (part && htmlToText(part).length >= 200) return part;
  }
  return html;
}

/** Shortest run of consecutive link-only lines (a sidebar, a table of contents) that `collapseLinkRuns` folds. */
const LINK_RUN_MIN = 8;

/** Folds each run of LINK_RUN_MIN or more link-only lines into one note, so an over-long page shows content first. */
export function collapseLinkRuns(text: string): string {
  const linkOnly = (line: string) => /^(- )?\[[^\]]*\]\([^)]*\)$/.test(line.trim());
  const out: string[] = [];
  let run: string[] = [];
  const flush = () => {
    if (run.length >= LINK_RUN_MIN) out.push(`[${run.length} links omitted]`);
    else out.push(...run);
    run = [];
  };
  for (const line of text.split('\n')) {
    if (linkOnly(line)) run.push(line);
    else if (line.trim() === '' && run.length > 0) continue;
    else {
      flush();
      out.push(line);
    }
  }
  flush();
  return out.join('\n');
}

/**
 * An HTML page as the model reads it: title, source (with the page's language), a note when the static HTML may
 * differ from the rendered page, the page's metadata, then its main text.
 */
export function describeHtml(html: string, url: string): string {
  const title = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html)?.[1]?.trim();
  const lang = /<html\b[^>]*\blang=["']?([\w-]+)/i.exec(html)?.[1];
  const text = htmlToText(mainContent(html));
  const note = staticHtmlNote(html, text);
  const metadata = pageMetadata(html);
  const head = `${title ? `# ${decodeEntities(title)}\n` : ''}Source: ${url} · static HTML${lang ? ` · lang ${lang}` : ''}`;
  return [head, note, metadata, text].filter(Boolean).join('\n\n');
}
