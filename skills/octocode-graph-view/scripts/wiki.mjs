#!/usr/bin/env node
// Code wiki over a result directory (Karpathy's LLM Wiki pattern applied to code): the repository and data.json are
// the raw sources, each Louvain community is a page, scripts keep each page's FACTS exact and current, and agents
// write the prose between the notes markers. Pages keep their slug and notes across runs (members matched by
// Jaccard overlap), notes go stale when a member file changes, and index.md is the map. Conventions: references/wiki.md.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ISOLATED, appendLog, firstSentence, isSlug, matchPages, memberHashes, notesOf, pageFile, readMap, readText, rootOf, slugify, staleMembers, today, wikiPaths, withNotes, writeIfChanged, writeMap } from './wiki-store.mjs';
import { communityFacts, factLine, isolatedFacts, pagerankOf, pathOfFile, wikiContext } from './wiki-facts.mjs';
import { HEADER, headerLine, notesState, orphanFacts, renderIndex, renderPage } from './wiki-render.mjs';
import { lintWiki } from './wiki-lint.mjs';

export { lintWiki };
const round = (x) => Number(x.toFixed(8));

// map.json row of one page: members (PageRank order), the files package members stand for, and the notes baseline.
function entryOf(ctx, c, prev) {
  const paths = {};
  for (const id of c.members) {
    const p = pathOfFile(ctx.byId.get(id));
    if (p && p !== id) paths[id] = p;
  }
  for (const id of Object.keys(prev?.notes?.hashes ?? {})) if (!(id in paths) && prev.paths?.[id]) paths[id] = prev.paths[id];
  return { community: c.id, label: c.label, size: c.size, pagerank: round(pagerankOf(ctx, c.members)), members: c.members, paths, notes: prev?.notes ?? null, summary: '', fact: '', stale: [], orphaned: null };
}

// The page's notes, kept verbatim from its file; notes found without a baseline (edited by hand) take one now.
function attachNotes(B, slug, entry) {
  const notes = notesOf(readText(pageFile(B.out, slug)));
  if (!notes) {
    entry.notes = null;
    return '';
  }
  entry.notes ??= { at: B.now, hashes: memberHashes(B.root, entry) };
  entry.stale = staleMembers(B.root, entry, B.changed);
  entry.summary = firstSentence(notes);
  return notes;
}

function reasonOf(was, entry) {
  const before = new Set(was.members), now = new Set(entry.members);
  const plus = entry.members.filter((id) => !before.has(id)).length, minus = was.members.filter((id) => !now.has(id)).length;
  const parts = [];
  if (was.orphaned) parts.push('its community is back');
  if (was.community !== entry.community) parts.push(`community ${was.community} is now ${entry.community}`);
  if (plus || minus) parts.push(`members +${plus} −${minus}`);
  if (notesState(was) !== notesState(entry)) parts.push(`notes ${notesState(entry)}`);
  return parts.length ? parts.join(', ') : 'facts changed';
}

function writePage(B, slug, entry, { facts, notes }) {
  const was = B.previous.get(slug);
  const written = writeIfChanged(pageFile(B.out, slug), renderPage({ slug, entry, facts, notes }));
  B.map.pages[slug] = entry;
  const newly = entry.stale.filter((id) => !(was?.stale ?? []).includes(id));
  if (newly.length) B.log.push(`stale [[${slug}]]: the notes predate changes to ${newly.map((id) => `\`${id}\``).join(', ')}`);
  if (!was) {
    B.created.push(slug);
    B.log.push(`created [[${slug}]]: ${entry.community === null ? 'files in no community' : `community ${entry.community}`}, ${entry.size} files`);
  } else if (written) {
    B.updated.push(slug);
    B.log.push(`updated [[${slug}]]: ${reasonOf(was, entry)}`);
  }
}

// Previous pages no community took: kept as orphaned while they hold notes, removed when they hold none.
function retire(B, used) {
  for (const [slug, was] of B.previous) {
    if (used.has(slug) || slug === ISOLATED) continue;
    const notes = notesOf(readText(pageFile(B.out, slug)));
    if (!notes) {
      rmSync(pageFile(B.out, slug), { force: true });
      B.removed.push(slug);
      B.log.push(`removed [[${slug}]]: no community matches its ${was.members.length} members and it had no notes`);
      continue;
    }
    if (was.orphaned) {
      B.map.pages[slug] = was;
      continue;
    }
    const entry = { ...was, community: null, orphaned: B.now };
    entry.stale = staleMembers(B.root, entry, B.changed);
    writeIfChanged(pageFile(B.out, slug), renderPage({ slug, entry, facts: orphanFacts(entry), notes }));
    B.map.pages[slug] = entry;
    B.updated.push(slug);
    B.log.push(`orphaned [[${slug}]]: no community matches its ${was.members.length} members; notes kept`);
  }
}

function isolatedPage(B, ctx) {
  const members = ctx.D.metrics.communities.filter((c) => c.size === 1).flatMap((c) => c.members)
    .toSorted((a, b) => (ctx.byId.get(b)?.metrics?.pagerank ?? 0) - (ctx.byId.get(a)?.metrics?.pagerank ?? 0) || a.localeCompare(b));
  const entry = entryOf(ctx, { id: null, label: 'Isolated files', size: members.length, members }, B.previous.get(ISOLATED));
  entry.fact = `${members.length} files that no production edge joins to a community`;
  const notes = attachNotes(B, ISOLATED, entry);
  writePage(B, ISOLATED, entry, { facts: isolatedFacts(ctx, members), notes });
}

const pageRows = (map) => Object.entries(map.pages).filter(([, e]) => !e.orphaned)
  .map(([slug, e]) => ({ slug, community: e.community, label: e.label, size: e.size, notesStale: notesState(e) === 'stale', notes: notesState(e), pagerank: e.pagerank }))
  .toSorted((a, b) => (a.slug === ISOLATED) - (b.slug === ISOLATED) || b.pagerank - a.pagerank);

// Writes <out>/wiki from data.json `D` (after computeMetrics). `changed`: files a refresh reports changed, added or
// removed (undefined on a full run); a page's file is rewritten only when its content differs.
export function buildWiki({ D, out, changed }) {
  if (!D?.metrics?.communities) throw new Error('buildWiki needs data.json with graph metrics (computeMetrics)');
  const prev = readMap(out);
  mkdirSync(wikiPaths(out).pages, { recursive: true });
  const now = new Date().toISOString();
  const B = { out, root: rootOf(D) || prev.root, now, changed: new Set(changed ?? []), previous: new Map(Object.entries(prev.pages)), created: [], updated: [], removed: [], log: [] };
  B.map = { version: 1, root: B.root, builtAt: now, pages: {} };
  const ctx = wikiContext(D);
  const communities = D.metrics.communities.filter((c) => c.size > 1);
  const matched = matchPages(communities, new Map([...B.previous].filter(([s]) => s !== ISOLATED)));
  const taken = new Set(B.previous.keys());
  const slugs = new Map(communities.map((c) => [c.id, matched.get(c.id) ?? slugify(c.label, taken)]));
  const labels = new Map(D.metrics.communities.map((c) => [c.id, c.label]));
  const names = { slugOf: (id) => slugs.get(id) ?? ISOLATED, labelOf: (id) => (slugs.has(id) ? labels.get(id) : 'Isolated files') };
  for (const c of communities) {
    const slug = slugs.get(c.id);
    const entry = entryOf(ctx, c, B.previous.get(slug));
    entry.fact = factLine(ctx, c, names.slugOf);
    const notes = attachNotes(B, slug, entry);
    writePage(B, slug, entry, { facts: communityFacts(ctx, c, names), notes });
  }
  isolatedPage(B, ctx);
  retire(B, new Set(slugs.values()));
  writeMap(out, B.map);
  writeFileSync(wikiPaths(out).index, renderIndex(B.map));
  appendLog(out, changed ? `refresh: ${changed.length} files changed, added or removed${changed.length ? `: ${changed.map((id) => `\`${id}\``).join(', ')}` : ''}` : 'build (full run)', B.log);
  return { pages: pageRows(B.map), updated: B.updated, created: B.created, removed: B.removed };
}

export function readPage({ out, slug }) {
  const P = wikiPaths(out);
  const file = slug === 'index' ? P.index : slug === 'log' ? P.log : isSlug(slug) ? pageFile(out, slug) : null;
  const markdown = file ? readText(file) : null;
  return markdown === null ? null : { slug, markdown };
}

// The live page holding a file (a symbol id resolves to its file; a package manifest to its package node).
export function pageOf({ out, file }) {
  const id = String(file ?? '').split('#')[0];
  for (const [slug, e] of Object.entries(readMap(out).pages)) {
    if (!e.orphaned && (e.members.includes(id) || Object.values(e.paths ?? {}).includes(id))) return slug;
  }
  return null;
}

function pageEntry(out, slug) {
  const map = readMap(out);
  const entry = isSlug(slug) ? map.pages[slug] : null;
  const markdown = entry ? readText(pageFile(out, slug)) : null;
  if (markdown === null) throw new Error(`no wiki page "${slug}" in ${wikiPaths(out).dir}: run wiki.mjs build first, or read index.md for the slugs`);
  return { map, entry, markdown };
}

function saveNotes(out, { slug, map, markdown, notes }, logLine) {
  const entry = map.pages[slug];
  entry.summary = firstSentence(notes);
  writeFileSync(pageFile(out, slug), withNotes(markdown, notes).replace(HEADER, headerLine(entry, slug)));
  writeMap(out, map);
  writeFileSync(wikiPaths(out).index, renderIndex(map));
  appendLog(out, `notes · ${slug}`, [logLine]);
}

// Replaces a page's agent notes; the members' current content becomes the baseline they are fresh against.
export function writeNotes({ out, slug, text }) {
  const { map, entry, markdown } = pageEntry(out, slug);
  const notes = String(text ?? '').trim();
  entry.notes = notes ? { at: new Date().toISOString(), hashes: memberHashes(map.root, entry) } : null;
  entry.stale = [];
  saveNotes(out, { slug, map, markdown, notes }, notes ? `notes written [[${slug}]]: ${notes.length} characters, fresh against ${entry.members.length} members` : `notes cleared [[${slug}]]`);
  return { slug, updated: true };
}

// Appends an answer worth keeping (a chat reply, a verified finding) to a page's notes with its source and date.
// The notes baseline stays: promoting one answer does not make stale notes fresh.
export function promote({ out, slug, text, source }) {
  const body = String(text ?? '').trim();
  const from = String(source ?? '').replace(/\s+/g, ' ').trim();
  if (!body || !from) throw new Error('promote needs text and a source (where the answer came from, e.g. "chat 12" or "lspSearch callers")');
  const { map, entry, markdown } = pageEntry(out, slug);
  const old = notesOf(markdown);
  const notes = `${old ? `${old}\n\n` : ''}**Promoted ${today()} from ${from}:** ${body}`;
  entry.notes ??= { at: new Date().toISOString(), hashes: memberHashes(map.root, entry) };
  saveNotes(out, { slug, map, markdown, notes }, `promoted into [[${slug}]] from ${from}: ${body.length} characters`);
  return { slug, appended: true };
}

const USAGE = `usage: wiki.mjs <command> <out> ...
  build <out> [--changed a,b]       write <out>/wiki from <out>/data.json (--changed: refresh file ids)
  read <out> <slug|index|log>       print a page; read <out> --file <path> prints the page holding that file
  note <out> <slug> <text|->        replace the page's agent notes (- reads stdin; --from <file> reads a file)
  promote <out> <slug> <text|-> --source <where it came from>   append to the notes with source and date
  lint <out> [--root <dir>] [--top N] [--json]   citations, links, missing/orphaned pages, stale or empty notes
Conventions: references/wiki.md.`;

const say = (text) => process.stdout.write(`${text}\n`);
const argText = (arg, from) => (from ? readFileSync(resolve(from), 'utf8') : arg === '-' ? readFileSync(0, 'utf8') : arg);
const option = (args, name) => {
  const i = args.indexOf(`--${name}`);
  return i < 0 ? undefined : args.splice(i, 2)[1];
};

function lintCommand(out, args) {
  const json = args.includes('--json');
  const top = option(args, 'top');
  const r = lintWiki({ out, root: option(args, 'root'), top: top ? Number(top) : undefined });
  if (json) say(JSON.stringify(r));
  else for (const i of r.issues) say(`${i.kind} ${i.slug}: ${i.detail}`);
  if (!json) say(`${r.issues.length} issue${r.issues.length === 1 ? '' : 's'}`);
  return r.issues.length ? 1 : 0;
}

const COMMANDS = {
  build: (out, args) => {
    const changed = option(args, 'changed');
    say(JSON.stringify(buildWiki({ D: JSON.parse(readFileSync(join(out, 'data.json'), 'utf8')), out, changed: changed?.split(',').filter(Boolean) })));
  },
  read: (out, args) => {
    const file = option(args, 'file');
    const page = readPage({ out, slug: file ? pageOf({ out, file }) : args[0] });
    if (!page) throw new Error(`no wiki page for ${file ?? args[0]}: index.md lists the slugs`);
    say(page.markdown);
  },
  note: (out, args) => {
    const from = option(args, 'from');
    say(JSON.stringify(writeNotes({ out, slug: args[0], text: argText(args[1], from) })));
  },
  promote: (out, args) => {
    const source = option(args, 'source');
    say(JSON.stringify(promote({ out, slug: args[0], text: argText(args[1]), source })));
  },
  lint: lintCommand,
};

function run(args) {
  const [command, dir] = args.splice(0, 2);
  if (!COMMANDS[command] || !dir || args.includes('--help')) {
    say(USAGE);
    return command === '--help' || args.includes('--help') ? 0 : 2;
  }
  return COMMANDS[command](resolve(dir), args) ?? 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = run(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`wiki.mjs: ${error.message}\n`);
    process.exitCode = 1;
  }
}
