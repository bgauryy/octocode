// Reduces tool reply rows to a set of evidence items, so two builds can be compared on what they returned and
// not on how they wrapped it (README.md#replay).
import { isRecord } from './lib.mjs';

/** Reply fields that carry no evidence: continuations, advice, paging bookkeeping and values that differ per run. */
export const ENVELOPE_KEYS = ['next', 'hints', 'related', 'rowPart', 'pagination', 'index', 'reused', 'clonedAt', 'expiresAt', 'replayed'];

const NUMBERED = /^(\d+)\t(.*)$/s;

/** Shared facts fill only the fields a row lacks. */
function withShared(shared, own) {
  if (own === undefined) return shared;
  if (!isRecord(shared) || !isRecord(own)) return own;
  const out = { ...own };
  for (const [key, value] of Object.entries(shared)) out[key] = withShared(value, Object.hasOwn(own, key) ? own[key] : undefined);
  return out;
}

/** The rows of one reply with the reply's shared facts filled in. */
export function rowsOf(reply) {
  if (!Array.isArray(reply?.results)) return [];
  return reply.results.map(row => (isRecord(reply.shared?.data) && isRecord(row?.data) ? { ...row, data: withShared(reply.shared.data, row.data) } : row));
}

/**
 * The evidence in `rows` as Map(item → texts[]). An item is one of
 *   q<i> line <path>:<n>\t<text>     a numbered line of file content
 *   q<i> match <path>:<n>            a search hit (its texts are the hit's text in each form it was given)
 *   q<i> patch <path> <text>#<k>     the k-th occurrence of a patch line
 *   q<i> text <path> <text>#<k>      a content line that carries no number
 *   q<i> field <pattern> = <json>    any other value, by its path with array steps as `*`
 * `queries` is the request's queries (a row without a path of its own takes the query's); strings equal to a key
 * of `replace` are rewritten first (a session home differs between runs).
 */
export function evidenceOf(rows, { queries = [], replace = {}, ignore = ENVELOPE_KEYS } = {}) {
  const items = new Map();
  const ignored = new Set(ignore);
  const swaps = Object.entries(replace).filter(([from]) => from);
  const clean = text => swaps.reduce((out, [from, to]) => out.replaceAll(from, to), text);
  const add = (item, text) => {
    const texts = items.get(item) ?? [];
    if (text !== undefined) texts.push(text);
    items.set(item, texts);
  };
  const counted = new Map();
  const nth = key => {
    const count = (counted.get(key) ?? 0) + 1;
    counted.set(key, count);
    return `${key}#${count}`;
  };
  const visit = (value, q, file, at) => {
    if (Array.isArray(value)) {
      for (const item of value) visit(item, q, file, `${at}/*`);
      return;
    }
    if (!isRecord(value)) {
      add(`${q} field ${at} = ${typeof value === 'string' ? JSON.stringify(clean(value)) : JSON.stringify(value)}`);
      return;
    }
    const here = typeof value.path === 'string' ? clean(value.path) : file;
    for (const [key, item] of Object.entries(value)) {
      if (ignored.has(key)) continue;
      const pointer = `${at}/${key}`;
      if (key === 'content' && typeof item === 'string') {
        for (const line of item.split('\n')) {
          const numbered = NUMBERED.exec(line);
          if (numbered) add(`${q} line ${here}:${numbered[1]}\t${clean(numbered[2])}`);
          else if (line.trim()) add(nth(`${q} text ${here} ${clean(line)}`));
        }
      } else if (key === 'patch' && typeof item === 'string') {
        for (const line of item.split('\n')) add(nth(`${q} patch ${here} ${clean(line)}`));
      } else if (key === 'matches' && Array.isArray(item)) {
        for (const match of item) {
          const numbered = typeof match === 'string' ? NUMBERED.exec(match) : undefined;
          if (numbered) add(`${q} match ${here}:${numbered[1]}`, clean(numbered[2]));
          else if (isRecord(match) && Number.isFinite(match.line)) {
            add(`${q} match ${here}:${match.line}`, typeof match.value === 'string' ? clean(match.value) : '');
            // What a hit states beside its line and text stays evidence, tied to the hit.
            for (const [name, extra] of Object.entries(match)) if (name !== 'line' && name !== 'value' && !ignored.has(name)) visit(extra, `${q} match ${here}:${match.line}`, here, `/${name}`);
          } else visit(match, q, here, `${pointer}/*`);
        }
      } else visit(item, q, here, pointer);
    }
  };
  for (const row of rows) {
    if (!isRecord(row)) continue;
    const index = Number.isSafeInteger(row.index) ? row.index : 0;
    const data = row.data;
    if (isRecord(data) && (data.errorCode !== undefined || data.error !== undefined)) continue; // an error row states no evidence
    const query = queries[index];
    visit(data, `q${index}`, isRecord(query) && typeof query.path === 'string' ? clean(query.path) : '', '');
  }
  return items;
}

/**
 * The items of `base` that `candidate` does not hold. A search hit is the same hit when the candidate has it at
 * the same path and line and its text still contains the base's text (a hit may gain what it states, e.g. where a
 * declaration ends, but may not lose text).
 */
export function missingFrom(base, candidate) {
  const missing = [];
  for (const [item, texts] of base) {
    const other = candidate.get(item);
    if (!other) missing.push(item);
    else if (texts.some(text => !other.some(candidateText => candidateText.includes(text.trim())))) missing.push(`${item} (text changed: ${JSON.stringify(texts.find(text => !other.some(candidateText => candidateText.includes(text.trim()))))})`);
  }
  return missing;
}
