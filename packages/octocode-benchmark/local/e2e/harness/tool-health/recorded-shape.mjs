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

/** Every text and number an output holds, folded for comparison. */
export function outputText(output) {
  const parts = [];
  const visit = value => {
    if (typeof value === 'string' || typeof value === 'number') parts.push(String(value).toLowerCase().replace(/\s+/g, ' '));
    else if (value && typeof value === 'object') Object.values(value).forEach(visit);
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
export function uncarried(answer, text, { unread = [], maps = [] } = {}) {
  const waived = matcher(unread);
  const values = new Map();
  walk(answer, '', matcher(maps), (path, value) => {
    if (!provable(value) || waived(path)) return;
    if (!values.has(path)) values.set(path, []);
    values.get(path).push(value);
  });
  return [...values].filter(([, seen]) => !seen.some(value => text.includes(fingerprint(value)))).map(([path]) => path);
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

/** The items to keep: first, last, any holding a named version, and each that adds a path or type. */
function covering(entries, named, maps) {
  const seen = new Set();
  const adds = value => {
    let added = false;
    for (const [path, types] of shapeOf(value, maps)) for (const type of types) {
      const key = `${path}:${type}`;
      if (!seen.has(key)) { seen.add(key); added = true; }
    }
    return added;
  };
  return entries.filter(([key, value], index) => {
    const adding = adds(value);
    return index === 0 || index === entries.length - 1 || named.has(key) || holds(value, named) || adding;
  });
}

/**
 * `answer`, smaller, with every path and type of shapeOf kept: long lists and
 * maps keep the items that add a path, and long text its start. An `encoding`d
 * text stays whole, since cutting it would break its decoding, and so do the
 * lists in `whole`, where each row routes a request.
 */
export function trimmed(answer, { maps = [], pinned = [], whole = [] } = {}) {
  const named = new Set([...pinned, ...ownVersions(answer)]);
  const isMap = matcher(maps);
  const keepAll = matcher(whole);
  const cut = (value, path) => {
    if (typeof value === 'string') return value.length > LONG_TEXT ? value.slice(0, LONG_TEXT) : value;
    if (Array.isArray(value)) {
      const kept = value.length > KEEP_WHOLE && !keepAll(path) ? covering(value.map((item, index) => [index, item]), named, maps).map(([, item]) => item) : value;
      return kept.map(item => cut(item, `${path}[*]`));
    }
    if (!value || typeof value !== 'object') return value;
    const entries = Object.entries(value);
    // Only a key that names data can go: a field's key is itself a path.
    const dataKeyOf = ([key]) => isMap(path) || !FIELD_KEY.test(key);
    const dataEntries = entries.filter(dataKeyOf);
    const keptData = new Set(dataEntries.length > KEEP_WHOLE ? covering(dataEntries, named, maps).map(([key]) => key) : dataEntries.map(([key]) => key));
    const kept = entries.filter(entry => !dataKeyOf(entry) || keptData.has(entry[0]));
    const encoded = typeof value.encoding === 'string';
    return Object.fromEntries(kept.map(([key, child]) => [key, encoded && typeof child === 'string' ? child : cut(child, join(path, dataKeyOf([key]) ? '*' : key))]));
  };
  return cut(answer, '');
}
