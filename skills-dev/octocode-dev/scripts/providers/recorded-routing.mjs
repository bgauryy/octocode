// Which values of a recorded answer the other requests of the same call carry, so
// trimming keeps them (recorded-shape.mjs `trimmed`): case-folded tokens, values
// inside request parts, and request path tails a value holds.
import { LONG_TEXT, MIN_EVIDENCE, leaves } from './recorded-values.mjs';

/** Where a value splits into the parts a request may carry one of: URL, query and text punctuation. */
const SEPARATORS = /[\s/?#&=:+@,;|"'`<>()[\]{}\\]+/;
const tokensOf = text => text.split(SEPARATORS).filter(Boolean);
/** One URL component decoded, `+` as a space; a malformed escape leaves only that component as sent. */
const decodedPart = (part, query) => { const text = query ? part.replaceAll('+', ' ') : part; try { return decodeURIComponent(text); } catch { return text; } };

/**
 * A request ({ url, body }) as its case-folded text, the whole tokens it carries,
 * and its path tails (`repos/o/r`, `o/r`): the joined segments a value may hold
 * inside a longer one, as a repository URL holds the owner/repo a request names.
 */
function requestParts({ url, body }) {
  const parts = [];
  let parsed;
  try { parsed = new URL(url); } catch { parsed = null; }
  const segments = parsed ? parsed.pathname.split('/').map(segment => decodedPart(segment, false).toLowerCase()).filter(Boolean) : [];
  if (parsed) {
    parts.push(decodedPart(`${parsed.origin}${parsed.pathname}`, false), parsed.host, ...segments);
    for (const pair of parsed.search.slice(1).split('&')) parts.push(...pair.split('=').map(part => decodedPart(part, true)));
  } else parts.push(String(url));
  if (body !== undefined) parts.push(...leaves(typeof body === 'string' ? { body } : body));
  const folded = parts.filter(Boolean).map(part => part.toLowerCase());
  const tails = segments.slice(0, -1).map((_, at) => segments.slice(at).join('/')).filter(tail => tail.length >= MIN_EVIDENCE);
  return { text: folded.join('\n'), tokens: new Set(folded.flatMap(part => [part, ...tokensOf(part)])), tails };
}

/**
 * What a value can route by, case-folded: each whole token of it (`t:`), which a
 * request must carry as a whole token, and the value itself (`v:`), which a
 * request may also carry inside a longer part, as a service URL is the start
 * of the URL called.
 */
const unitsOf = value => {
  const folded = String(value).toLowerCase();
  return [`v:${folded}`, ...[folded, ...tokensOf(folded)].map(token => `t:${token}`)];
};

const NAME_CHAR = /[a-z0-9_-]/;
/** Whether `text` holds `tail` as whole segments: no name character runs on at either end. */
function holdsTail(text, tail) {
  for (let at = text.indexOf(tail); at >= 0; at = text.indexOf(tail, at + 1)) {
    if (!NAME_CHAR.test(text[at - 1] ?? '') && !NAME_CHAR.test(text[at + tail.length] ?? '')) return true;
  }
  return false;
}

/** Whether request `part` carries `unit`: a whole token, a value inside its text, or one of its path tails. */
function carries(part, unit) {
  const value = unit.slice(2);
  if (unit.startsWith('t:')) return part.tokens.has(value);
  if (unit.startsWith('p:')) return part.tails.includes(value);
  return value.length >= MIN_EVIDENCE && part.text.includes(value);
}

/**
 * What the other requests of a call (`requests`, { url, body }) carry of an
 * answer, given `own`, the request it answers: the tool sent that before the
 * answer came, so nothing in it was taken from the answer.
 * - `text(unit)`: a unit of a long text routes when another request carries it
 *   and `own` does not; prose repeats the words the tool asked with.
 * - `unitsOf(value, key)`: the units an item holds, without the values that only
 *   echo `own` whole. A value the answer extends (`lodash/lodash` for an asked
 *   `lodash`) keeps every unit, and each path tail it holds.
 * - `each`: per other request, whether it carries a unit.
 * A false match keeps more, never less.
 */
export function requested(requests, own) {
  const parts = requests.map(requestParts);
  const sent = own ? requestParts(own) : { text: '', tokens: new Set(), tails: [] };
  const tails = [...new Set(parts.flatMap(part => part.tails))];
  const echoes = folded => sent.tokens.has(folded) || (folded.length >= MIN_EVIDENCE && sent.text.includes(folded));
  const itemUnits = (value, key) => [...leaves(value, typeof key === 'string' ? new Set([key]) : new Set())].flatMap(leaf => {
    const folded = leaf.toLowerCase();
    return echoes(folded) ? [] : [...unitsOf(folded), ...tails.filter(tail => holdsTail(folded, tail)).map(tail => `p:${tail}`)];
  });
  return {
    text: unit => parts.some(part => carries(part, unit)) && !carries(sent, unit),
    unitsOf: itemUnits,
    each: parts.map(part => unit => carries(part, unit)),
  };
}

/**
 * The indices to keep beside `kept`. Per request, the item holding the most of
 * what it carries, unless a kept item holds as much: a request built from two
 * values of one item needs that item, not one holder of each. Then each unit a
 * request carries, rarest first, gets one holder unless a kept item holds it,
 * the holder that routes the most. A unit every item holds routes to none.
 */
export function routedIndices(held, each, kept) {
  const keep = new Set(kept);
  const counts = new Map();
  for (const units of held) for (const unit of units) counts.set(unit, (counts.get(unit) ?? 0) + 1);
  const distinct = unit => counts.get(unit) < held.length;
  const routing = held.map(units => new Set([...units].filter(unit => distinct(unit) && each.some(carried => carried(unit)))));
  const most = (indices, score) => indices.reduce((best, index) => (score(index) > score(best) ? index : best));
  for (const carried of each) {
    const covered = routing.map(units => [...units].filter(carried).length);
    const best = Math.max(0, ...covered);
    if (best === 0 || [...keep].some(index => covered[index] === best)) continue;
    keep.add(most(covered.flatMap((count, index) => (count === best ? [index] : [])), index => routing[index].size));
  }
  const holders = new Map();
  routing.forEach((units, index) => { for (const unit of units) holders.set(unit, [...(holders.get(unit) ?? []), index]); });
  for (const [, at] of [...holders].toSorted((a, b) => a[1].length - b[1].length)) {
    if (!at.some(index => keep.has(index))) keep.add(most(at, index => routing[index].size));
  }
  return keep;
}

/**
 * Where a long text may be cut: just past the last place a routing unit of it
 * occurs, never before LONG_TEXT. All of it when the text itself routes.
 */
export function cutAt(text, routes) {
  const folded = text.toLowerCase();
  if (routes(`v:${folded}`)) return text.length;
  let end = LONG_TEXT;
  for (const token of new Set([folded, ...tokensOf(folded)])) {
    if (routes(`t:${token}`)) end = Math.max(end, folded.lastIndexOf(token) + token.length);
  }
  // Case folding that changes a text's length moves its offsets: keep what the folded offset may reach.
  return folded.length === text.length || end === LONG_TEXT ? end : text.length;
}
