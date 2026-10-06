/**
 * Text rules for memory search, pure so they are easy to test: the words of a prompt become a safe FTS5 query (every
 * term quoted, so `"`, `*`, `:`, `^`, `NEAR`, `AND`, `OR`, `NOT` and column filters are plain text), and the same terms
 * drive the LIKE fallback and near-duplicate checks.
 */

/** Words too common to tell memories apart. */
const STOPWORDS = new Set(
  (
    'a an and are as at be been but by can could did do does doing done for from had has have how i if in into is it its ' +
    'just let me my no not of on or our please should so some than that the their them then there these they this those ' +
    'to too up us use used using was we were what when where which who why will with would you your yes also any all ' +
    'about after again before being below between both each few more most other over same such only own very'
  ).split(' '),
);

/** Most terms one query carries: a long prompt keeps its first distinct words. */
export const MAX_QUERY_TERMS = 32;

/**
 * Lower-case letter/number words of `text` (NFKC, so full-width `ＳＱＬ` is `sql`), stopwords and one-letter words
 * dropped, first occurrence order, unique.
 */
export function terms(text: string, max = Number.POSITIVE_INFINITY): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const match of text.normalize('NFKC').toLowerCase().matchAll(/[\p{L}\p{N}_]+/gu)) {
    // Snake case splits too (`busy_timeout` → busy, timeout) so it matches the unicode61 tokenizer's view of `_`.
    for (const word of match[0].split('_')) {
      if (word.length < 2 || STOPWORDS.has(word) || seen.has(word)) continue;
      seen.add(word);
      out.push(word);
      if (out.length >= max) return out;
    }
  }
  return out;
}

/**
 * An FTS5 MATCH expression that ORs the quoted terms of `text`, or undefined when nothing searchable is left. Terms hold
 * only letters and digits, so quoting leaves no operator, prefix, column filter or quote for the parser to act on.
 */
export function ftsQuery(text: string): string | undefined {
  const words = terms(text, MAX_QUERY_TERMS);
  return words.length > 0 ? words.map((word) => `"${word}"`).join(' OR ') : undefined;
}

/**
 * A rough English stem, the same for a word's common inflections (`release`, `releases`, `released`, `releasing` →
 * `relea`), so matches made outside FTS5 roughly agree with its porter tokenizer. Only ever compared with another stem.
 */
export function stem(word: string): string {
  const stripped = word.replace(/(?:ing|ed|es|e|s)+$/, '');
  return stripped.length >= 3 ? stripped : word;
}

/** A title compared for duplicates: case, punctuation and spacing ignored (every word kept, so `Step 1` ≠ `Step 2`). */
export function normalizeTitle(title: string): string {
  return (title.normalize('NFKC').toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []).join(' ');
}

/** |A ∩ B| / |A ∪ B| of two term lists (0 when both are empty). */
export function jaccard(a: readonly string[], b: readonly string[]): number {
  const left = new Set(a);
  const right = new Set(b);
  const union = new Set([...left, ...right]).size;
  if (union === 0) return 0;
  let shared = 0;
  for (const word of left) if (right.has(word)) shared++;
  return shared / union;
}

/** `M7`, `m7` or `7` → 7; anything else → undefined. */
export function parseMemoryId(raw: string | undefined): number | undefined {
  const match = /^\s*m?(\d{1,15})\s*$/i.exec(raw ?? '');
  const id = match ? Number(match[1]) : Number.NaN;
  return Number.isSafeInteger(id) && id > 0 ? id : undefined;
}

export const memoryLabel = (id: number): string => `M${id}`;
