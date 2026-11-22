// Wiki storage: where the files live under <out>/wiki, map.json (each page's members and the member content hashes
// its agent notes were written against), stable slugs, the agent-notes markers and the append-only log.
// Conventions for the agents that maintain the wiki: references/wiki.md.
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const NOTES_START = '<!-- notes:start -->';
export const NOTES_END = '<!-- notes:end -->';
export const ISOLATED = 'isolated';
// Slugs a community page can never take: the map, the history and the isolated-files listing.
export const RESERVED = new Set(['index', 'log', ISOLATED]);
// A previous page keeps its slug and notes for the new community it overlaps most, when the overlap is at least
// this Jaccard share: a community split three ways still hands its page to its largest part.
export const MATCH_MIN = 0.25;
const SLUG = /^[a-z0-9][a-z0-9-]*$/;
const SLUG_MAX = 72;

export const wikiPaths = (out) => {
  const dir = join(out, 'wiki');
  return { dir, pages: join(dir, 'pages'), index: join(dir, 'index.md'), log: join(dir, 'log.md'), map: join(dir, 'map.json') };
};

export const isSlug = (slug) => typeof slug === 'string' && SLUG.test(slug);
export const pageFile = (out, slug) => join(wikiPaths(out).pages, `${slug}.md`);
export const today = (at = new Date()) => at.toISOString().slice(0, 10);

export function readText(file) {
  try { return readFileSync(file, 'utf8'); } catch { return null; }
}

export function readMap(out) {
  const text = readText(wikiPaths(out).map);
  return text ? JSON.parse(text) : { version: 1, root: '', pages: {}, isolated: [] };
}

export function writeMap(out, map) {
  mkdirSync(wikiPaths(out).pages, { recursive: true });
  writeFileSync(wikiPaths(out).map, `${JSON.stringify(map, null, 1)}\n`);
}

// Writes a file only when its content differs; true when it was written.
export function writeIfChanged(file, text) {
  if (readText(file) === text) return false;
  writeFileSync(file, text);
  return true;
}

// Content hash of one file on disk; null when it cannot be read (missing).
export function hashFile(abs) {
  try { return createHash('sha256').update(readFileSync(abs)).digest('hex').slice(0, 16); } catch { return null; }
}

// The repository root data.json was built from: any located file's absolute path minus its relative path.
export function rootOf(D) {
  const f = D.files.find((x) => x.loc?.abs && x.loc?.path && x.loc.abs.endsWith(x.loc.path));
  return f ? f.loc.abs.slice(0, f.loc.abs.length - f.loc.path.length).replace(/\/$/, '') : '';
}

// The file a member id stands for: a package node is its manifest, every other node its own path.
export const pathOf = (entry, id) => entry.paths?.[id] ?? id;

export function memberHashes(root, entry) {
  return Object.fromEntries(entry.members.map((id) => [id, hashFile(join(root, pathOf(entry, id)))]));
}

// Members whose content differs from the hashes the notes were written against (or that appeared or went away),
// plus members a refresh reports changed that could not be read either time.
export function staleMembers(root, entry, changed) {
  const base = entry.notes?.hashes;
  if (!base) return [];
  return Object.keys(base).filter((id) => {
    const now = hashFile(join(root, pathOf(entry, id)));
    return now !== base[id] || (now === null && changed.has(id));
  });
}

// Agent notes are the text between the markers, kept verbatim.
export function notesOf(markdown) {
  if (!markdown) return '';
  const a = markdown.indexOf(NOTES_START), b = markdown.indexOf(NOTES_END);
  return a < 0 || b < a ? '' : markdown.slice(a + NOTES_START.length, b).replace(/^\n/, '').replace(/\n$/, '');
}

export function withNotes(markdown, notes) {
  const a = markdown.indexOf(NOTES_START), b = markdown.indexOf(NOTES_END);
  return `${markdown.slice(0, a + NOTES_START.length)}\n${notes}${notes ? '\n' : ''}${markdown.slice(b)}`;
}

// The first sentence of the notes, for index.md: headings, markers and emphasis left out.
export function firstSentence(notes) {
  const line = notes.split('\n').map((l) => l.trim()).find((l) => l && !l.startsWith('#') && !l.startsWith('<!--') && !l.startsWith('|'));
  if (!line) return '';
  const text = line.replace(/^[-*>]\s+/, '').replace(/\*\*/g, '');
  const end = text.search(/[.!?](\s|$)/);
  return end < 0 ? text : text.slice(0, end + 1);
}

export function slugify(label, taken) {
  let base = String(label).replace(/\.[A-Za-z0-9]+$/, '').toLowerCase().replace(/^\(root\)/, 'root').replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'root';
  if (base.length > SLUG_MAX) base = base.slice(-SLUG_MAX).replace(/^-+/, '');
  if (RESERVED.has(base)) base = `${base}-page`;
  let slug = base;
  for (let n = 2; taken.has(slug); n++) slug = `${base}-${n}`;
  taken.add(slug);
  return slug;
}

// Every overlapping (community, previous page) pair with its Jaccard share.
function overlaps(communities, previous) {
  const owners = new Map();
  for (const [slug, entry] of previous) for (const id of entry.members) owners.set(id, [...(owners.get(id) ?? []), slug]);
  const pairs = [];
  for (const c of communities) {
    const shared = new Map();
    for (const id of c.members) for (const slug of owners.get(id) ?? []) shared.set(slug, (shared.get(slug) ?? 0) + 1);
    for (const [slug, n] of shared) {
      const size = previous.get(slug).members.length;
      pairs.push({ c, slug, jaccard: n / (c.members.length + size - n), size });
    }
  }
  return pairs;
}

// Louvain renumbers communities on every run: each new community takes the previous page it overlaps most, best
// share first (ties: the larger community, then the larger page, then the slug), so slugs and notes stay put.
export function matchPages(communities, previous) {
  const pairs = overlaps(communities, previous).filter((p) => p.jaccard >= MATCH_MIN)
    .toSorted((a, b) => b.jaccard - a.jaccard || b.c.size - a.c.size || b.size - a.size || a.slug.localeCompare(b.slug));
  const slugOf = new Map(), used = new Set();
  for (const p of pairs) {
    if (slugOf.has(p.c.id) || used.has(p.slug)) continue;
    slugOf.set(p.c.id, p.slug);
    used.add(p.slug);
  }
  return slugOf;
}

export function appendLog(out, title, lines) {
  const file = wikiPaths(out).log;
  if (!existsSync(file)) writeFileSync(file, '# Wiki log\n\nAppend-only history of builds, refreshes, notes and promotions. Newest last.\n');
  appendFileSync(file, `\n## ${new Date().toISOString()} · ${title}\n\n${(lines.length ? lines : ['no page changed']).map((l) => `- ${l}`).join('\n')}\n`);
}
