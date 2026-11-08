// The shape of a provider answer: its field paths and each path's types. A path
// reads `vulns[*].severity[*].score`; a key that names data rather than a field
// (a version, `vendor/package`) reads `*`, as does any key under a declared map.
// Pure functions: the recorder, the shape check and the drift check share them.

const FIELD_KEY = /^[A-Za-z_@$][\w@$:-]*$/;
const KEEP_WHOLE = 4;
const LONG_TEXT = 1200;
/** A value this short says nothing when it appears in an output. */
const MIN_EVIDENCE = 3;

const typeOf = value => (value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value);
const join = (path, key) => (path ? `${path}.${key}` : key);

const pathPattern = glob => new RegExp(`^${glob.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '[^.]*')}(?:$|[.[])`);
/** Whether `path` is one of `globs` or below one; `*` stays within a segment. */
export const matcher = globs => {
  const patterns = globs.map(pathPattern);
  return path => patterns.some(pattern => pattern.test(path));
};

function walk(value, path, maps, visit) {
  visit(path, value);
  if (Array.isArray(value)) for (const item of value) walk(item, `${path}[*]`, maps, visit);
  else if (value && typeof value === 'object') {
    const named = maps(path);
    for (const [key, child] of Object.entries(value)) walk(child, join(path, !named && FIELD_KEY.test(key) ? key : '*'), maps, visit);
  }
}

/** path -> sorted types, for one parsed answer. */
export function shapeOf(answer, maps = []) {
  const types = new Map();
  walk(answer, '', matcher(maps), (path, value) => {
    if (!types.has(path)) types.set(path, new Set());
    types.get(path).add(typeOf(value));
  });
  return new Map([...types].map(([path, seen]) => [path, [...seen].toSorted((a, b) => a.localeCompare(b))]));
}

/** A small XML answer as nested objects, so POMs and metadata have field paths too. */
export function xmlFields(text) {
  const root = {};
  const stack = [root];
  for (const [, closing, name, attributes, selfClosed, content] of text.matchAll(/<(\/?)([A-Za-z_][\w.:-]*)((?:\s[^<>]*?)?)(\/?)>([^<]*)/g)) {
    if (attributes === undefined) continue;
    if (closing) { stack.pop(); continue; }
    const parent = stack.at(-1);
    const node = { text: content.trim() };
    parent[name] = parent[name] === undefined ? node : [parent[name], node].flat();
    if (!selfClosed) stack.push(node);
  }
  return root;
}

/** The fields of one recorded exchange; a plain-text answer has none. */
export function fieldsOf(exchange) {
  const { body } = exchange;
  if (typeof body !== 'string') return body ?? undefined;
  return /<[A-Za-z?]/.test(body) ? xmlFields(body) : undefined;
}

/** A source repository's location; transport spelling is not another repository. */
function repositoryIdentity(value) {
  if (typeof value !== 'string') return null;
  let address = value.trim().replace(/^scm:git:/, '').replace(/^git\+/, '');
  address = address.replace(/^git@([^/:]+):/, 'ssh://git@$1/');
  let uri;
  try { uri = new URL(address); } catch { return null; }
  if (!['http:', 'https:', 'ssh:', 'git:'].includes(uri.protocol)) return null;
  const defaultPort = { 'ssh:': '22', 'git:': '9418' }[uri.protocol];
  const port = uri.port && uri.port !== defaultPort ? `:${uri.port}` : '';
  const pathname = uri.pathname.replace(/\/$/, '').replace(/\.git$/, '');
  return `${uri.hostname.toLowerCase()}${port}${pathname}${uri.search}${uri.hash}`;
}

const repositoryMarker = value => {
  const identity = repositoryIdentity(value);
  return identity === null ? null : `\u001erepository:${identity}\u001e`;
};

const dateMarker = value => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}(?:T|$)/i.test(value)
  ? `\u001edate:${value.slice(0, 10)}\u001e` : null;

/** Output fields that state when the tool looked, not a date the provider gave. */
const TOOL_CLOCK = new Set(['observedAt']);

/** Every text and number an output holds, folded for comparison. */
export function outputText(output) {
  const parts = [];
  const visit = value => {
    if (typeof value === 'string' || typeof value === 'number') parts.push(String(value).toLowerCase().replace(/\s+/g, ' '));
    else if (value && typeof value === 'object') {
      // The authored artifact shape uses either repository or one owner/repo pair.
      const repository = typeof value.repository === 'string' ? value.repository
        : typeof value.owner === 'string' && typeof value.repo === 'string'
          ? `https://github.com/${value.owner}/${value.repo}` : null;
      const marker = repositoryMarker(repository);
      if (marker) parts.push(marker);
      // A date a tool shows is a day: any `...At` field of the output, whatever it is
      // named, except the tool's own clock: today's day proves no provider date.
      for (const [key, field] of Object.entries(value)) {
        const day = key.endsWith('At') && !TOOL_CLOCK.has(key) ? dateMarker(field) : null;
        if (day) parts.push(day);
      }
      if (value.verification === 'tag' && typeof value.ref === 'string') parts.push(`refs/tags/${value.ref}`);
      Object.values(value).forEach(visit);
    }
  };
  visit(output);
  return parts.join('\n');
}

const provable = value => (typeof value === 'number' ? Math.abs(value) >= 1000 : typeof value === 'string' && value.trim().length >= MIN_EVIDENCE);
// Reading a long text may reflow it, so its start stands for the whole; a time
// may be restated in another precision or zone spelling, so its seconds do.
const ISO_TIME = /^\d{4}-\d\d-\d\dt\d\d:\d\d:\d\d/;
const fingerprint = value => {
  const text = String(value).toLowerCase().replace(/\s+/g, ' ').trim();
  return ISO_TIME.test(text) ? text.slice(0, 19) : text.slice(0, 80);
};

/**
 * The field paths of `answer` none of whose values reached `text` and which are
 * not declared `unread`: what the provider said and the tool never showed. A
 * path passes on any one value: a tool may select rows, not every row.
 */
export function uncarried(answer, text, { unread = [], maps = [], repositories = [], dates = [] } = {}) {
  const waived = matcher(unread);
  const repository = matcher(repositories);
  const date = matcher(dates);
  const values = new Map();
  walk(answer, '', matcher(maps), (path, value) => {
    if (!provable(value) || waived(path)) return;
    if (!values.has(path)) values.set(path, []);
    values.get(path).push(value);
  });
  return [...values].filter(([path, seen]) => !seen.some(value => {
    const marker = repository(path) ? repositoryMarker(value) : date(path) && dateMarker(value);
    return text.includes(marker || fingerprint(value));
  })).map(([path]) => path);
}

/** Exchange-local omissions describe enrichment, without waiving the queried record. */
export function uncarriedExchanges(exchanges, text, options = {}) {
  return [...new Set(exchanges.filter(exchange => !exchange.alternate && exchange.status < 400).flatMap(exchange => {
    const fields = fieldsOf(exchange);
    return fields === undefined ? [] : uncarried(fields, text, {
      ...options, unread: [...(options.unread ?? []), ...(exchange.unread ?? [])],
    });
  }))];
}

const parentOf = path => {
  const cut = Math.max(path.lastIndexOf('.'), path.lastIndexOf('['));
  return cut > 0 ? path.slice(0, cut) : null;
};

/**
 * `gaps` as the shortest declaration: a subtree whose every provable field is a
 * gap reads as its root, so a list stays reviewable.
 */
export function declaredUnread(gaps, answers, maps = []) {
  const provablePaths = new Set();
  for (const answer of answers) walk(answer, '', matcher(maps), (path, value) => { if (provable(value)) provablePaths.add(path); });
  const gapSet = new Set(gaps);
  const widen = path => {
    const parent = parentOf(path);
    if (!parent) return path;
    const within = matcher([parent]);
    return [...provablePaths].every(known => !within(known) || gapSet.has(known)) ? widen(parent) : path;
  };
  return [...new Set(gaps.map(widen))].toSorted((a, b) => a.localeCompare(b));
}

/**
 * How `fresh` differs from `recorded`, both from shapeOf. A new path or type is
 * a note; a path or type that went away is a failure, except a value that was
 * or became null, which a field may do on any answer.
 */
export function shapeDrift(recorded, fresh) {
  const notes = [], failures = [];
  for (const [path, types] of fresh) {
    const before = recorded.get(path);
    if (!before) notes.push(`added ${path || '(root)'}: ${types.join('|')}`);
    else {
      const gained = types.filter(type => !before.includes(type) && type !== 'null');
      if (gained.length) notes.push(`widened ${path || '(root)'}: ${before.join('|')} + ${gained.join('|')}`);
    }
  }
  for (const [path, types] of recorded) {
    const now = fresh.get(path);
    if (!now) failures.push(`removed ${path || '(root)'}: was ${types.join('|')}`);
    else {
      const gone = now.every(type => type === 'null') ? [] : types.filter(type => !now.includes(type) && type !== 'null');
      if (gone.length) failures.push(`type changed ${path || '(root)'}: ${types.join('|')} -> ${now.join('|')}`);
    }
  }
  return { notes, failures };
}

const VERSION_LIKE = /^v?\d+\.\d/;

/** The versions an answer states about itself near its root: a latest, a default. */
function ownVersions(value, depth = 3, found = new Set()) {
  if (typeof value === 'string' && VERSION_LIKE.test(value)) found.add(value);
  else if (depth && value && typeof value === 'object' && !Array.isArray(value)) for (const child of Object.values(value)) ownVersions(child, depth - 1, found);
  return found;
}

/** Whether `value`, within a few levels, is or holds one of `named`. */
function holds(value, named, depth = 3) {
  if (typeof value === 'string') return named.has(value);
  return depth > 0 && !!value && typeof value === 'object' && Object.values(value).some(child => holds(child, named, depth - 1));
}

/** Every text and number in `value`, and every key that names data, at any depth. */
function leaves(value, found = new Set()) {
  if (typeof value === 'string' || typeof value === 'number') found.add(String(value));
  else if (value && typeof value === 'object') for (const [key, child] of Object.entries(value)) {
    if (!Array.isArray(value) && !FIELD_KEY.test(key)) found.add(key);
    leaves(child, found);
  }
  return found;
}

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
function requested(requests, own) {
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
function routedIndices(held, each, kept) {
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
 * The items to keep: first, last, any holding a named version, each that adds
 * a path or type, and what the call's other requests were built from.
 */
function covering(entries, named, maps, routes) {
  const held = entries.map(([key, value]) => new Set(routes.unitsOf(value, key)));
  const seen = new Set();
  const adds = value => {
    let added = false;
    for (const [path, types] of shapeOf(value, maps)) for (const type of types) {
      const key = `${path}:${type}`;
      if (!seen.has(key)) { seen.add(key); added = true; }
    }
    return added;
  };
  const base = entries.flatMap(([key, value], index) => {
    const adding = adds(value);
    return index === 0 || index === entries.length - 1 || named.has(key) || holds(value, named) || adding ? [index] : [];
  });
  const keep = routedIndices(held, routes.each, base);
  return entries.filter((_, index) => keep.has(index));
}

/**
 * Where a long text may be cut: just past the last place a routing unit of it
 * occurs, never before LONG_TEXT. All of it when the text itself routes.
 */
function cutAt(text, routes) {
  const folded = text.toLowerCase();
  if (routes(`v:${folded}`)) return text.length;
  let end = LONG_TEXT;
  for (const token of new Set([folded, ...tokensOf(folded)])) {
    if (routes(`t:${token}`)) end = Math.max(end, folded.lastIndexOf(token) + token.length);
  }
  // Case folding that changes a text's length moves its offsets: keep what the folded offset may reach.
  return folded.length === text.length || end === LONG_TEXT ? end : text.length;
}

/**
 * `answer`, smaller, with every path and type of shapeOf kept: long lists and
 * maps keep the items that add a path, and long text its start. An `encoding`d
 * text stays whole, since cutting it would break its decoding. Nothing another
 * request of the same call carries goes: `requests` ({ url, body }) are those
 * requests and `own` the one this answer answers. A value routes when one of
 * them carries it, case-folded, as a whole token of a URL component or body
 * text, inside one when the value is at least MIN_EVIDENCE long, or as a path
 * tail the value holds. A list or map keeps, per request, an item holding the
 * most of what it carries, then a holder of each routing value; values that
 * only echo `own` route nothing. A long text is cut just past its last routing
 * value that `own` did not already carry.
 */
export function trimmed(answer, { maps = [], pinned = [], requests = [], own } = {}) {
  const named = new Set([...pinned, ...ownVersions(answer)]);
  const isMap = matcher(maps);
  const routes = requested(requests, own);
  const cut = (value, path) => {
    if (typeof value === 'string') return value.length > LONG_TEXT ? value.slice(0, cutAt(value, routes.text)) : value;
    if (Array.isArray(value)) {
      const kept = value.length > KEEP_WHOLE ? covering(value.map((item, index) => [index, item]), named, maps, routes).map(([, item]) => item) : value;
      return kept.map(item => cut(item, `${path}[*]`));
    }
    if (!value || typeof value !== 'object') return value;
    const entries = Object.entries(value);
    // Only a key that names data can go: a field's key is itself a path.
    const dataKeyOf = ([key]) => isMap(path) || !FIELD_KEY.test(key);
    const dataEntries = entries.filter(dataKeyOf);
    const keptData = new Set(dataEntries.length > KEEP_WHOLE ? covering(dataEntries, named, maps, routes).map(([key]) => key) : dataEntries.map(([key]) => key));
    const kept = entries.filter(entry => !dataKeyOf(entry) || keptData.has(entry[0]));
    const encoded = typeof value.encoding === 'string';
    return Object.fromEntries(kept.map(([key, child]) => [key, encoded && typeof child === 'string' ? child : cut(child, join(path, dataKeyOf([key]) ? '*' : key))]));
  };
  return cut(answer, '');
}
