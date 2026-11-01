// Wiki health check (the LLM Wiki "lint" operation): notes citing a file that is gone or a line past its end,
// [[links]] to no page, communities in data.json without a page, orphaned pages, stale notes, and the pages holding
// most of the PageRank that still have no notes. Reads the cited files under the repository root.
import { isAbsolute, join, relative, resolve } from 'node:path';
import { ISOLATED, RESERVED, notesOf, pageFile, readMap, readText, staleMembers, wikiPaths } from './wiki-store.mjs';

// path:line, path:line-line or path:line:column, where path has an extension; URLs (://host:port) are left out.
const CITE = /(?<![\w/.:@-])((?:[\w@+.-]+\/)*[\w@+-][\w@+.-]*\.[A-Za-z0-9]+):(\d+)(?:[-–](\d+))?/g;
const LINK = /\[\[([^\]\n]+)\]\]/g;

const countLines = (text) => (text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0));

// Line count of a root-relative file, or null when it is not a readable file inside the root.
function lineCounter(root) {
  const cache = new Map();
  return (path) => {
    if (!cache.has(path)) {
      const abs = resolve(root, path), rel = relative(root, abs);
      const text = rel.startsWith('..') || isAbsolute(rel) ? null : readText(abs);
      cache.set(path, text === null ? null : countLines(text));
    }
    return cache.get(path);
  };
}

function citationIssues(slug, notes, lines) {
  const issues = [];
  for (const [text, path, from, to] of notes.matchAll(CITE)) {
    const n = lines(path);
    if (n === null) issues.push({ slug, kind: 'citation-file', detail: `${text}: ${path} is not a file under the root` });
    else if (Number(from) < 1 || Number(to ?? from) > n) issues.push({ slug, kind: 'citation-line', detail: `${text}: ${path} has ${n} lines` });
  }
  return issues;
}

function linkIssues(slug, markdown, known) {
  return [...markdown.matchAll(LINK)].map((m) => m[1].trim()).filter((s) => !known.has(s))
    .map((s) => ({ slug, kind: 'broken-link', detail: `[[${s}]] names no page` }));
}

function readData(out) {
  const text = readText(join(out, 'data.json'));
  return text ? JSON.parse(text) : null;
}

// Each community of more than one file in data.json needs a live page with the same id and members.
function missingPages(out, live) {
  const D = readData(out);
  if (!D?.metrics?.communities) return [];
  const byCommunity = new Map(live.map(([slug, e]) => [e.community, { slug, e }]));
  return D.metrics.communities.filter((c) => c.size > 1).flatMap((c) => {
    const p = byCommunity.get(c.id);
    const same = p && p.e.size === c.size && p.e.members.every((id, i) => id === c.members[i]);
    if (same) return [];
    const detail = p ? `community ${c.id} (${c.label}) changed since [[${p.slug}]] was built: run wiki.mjs build` : `community ${c.id} (${c.label}, ${c.size} files) has no page: run wiki.mjs build`;
    return [{ slug: p?.slug ?? '-', kind: 'missing-page', detail }];
  });
}

// The pages that together hold half the PageRank of all pages (or the first `top`), highest first.
function topPages(live, top) {
  const ranked = live.filter(([s]) => s !== ISOLATED).toSorted((a, b) => b[1].pagerank - a[1].pagerank);
  if (Number.isInteger(top)) return ranked.slice(0, top);
  const total = ranked.reduce((a, [, e]) => a + e.pagerank, 0);
  const picked = [];
  let sum = 0;
  for (const row of ranked) {
    if (picked.length && sum >= total / 2) break;
    picked.push(row);
    sum += row[1].pagerank;
  }
  return picked;
}

function pageIssues(slug, e, { out, root, lines, known }) {
  const markdown = readText(pageFile(out, slug));
  if (markdown === null) return [{ slug, kind: 'missing-file', detail: `${pageFile(out, slug)} is missing: run wiki.mjs build` }];
  const issues = [...citationIssues(slug, notesOf(markdown), lines), ...linkIssues(slug, markdown, known)];
  if (e.orphaned) issues.push({ slug, kind: 'orphaned', detail: `no community matches this page since ${e.orphaned.slice(0, 10)}: move its notes to the pages holding ${e.members.length} former members, then clear them` });
  const stale = e.notes ? staleMembers(root, e, new Set()) : [];
  if (stale.length) issues.push({ slug, kind: 'stale-notes', detail: `changed since the notes were written: ${stale.join(', ')}` });
  return issues;
}

// { issues: [{ slug, kind, detail }] }; `root` defaults to the repository the wiki was built from.
export function lintWiki({ out, root, top }) {
  const map = readMap(out);
  const base = resolve(root ?? map.root ?? '.');
  const entries = Object.entries(map.pages);
  const known = new Set([...entries.map(([s]) => s), ...RESERVED]);
  const ctx = { out, root: base, lines: lineCounter(base), known };
  const issues = entries.flatMap(([slug, e]) => pageIssues(slug, e, ctx));
  // index.md repeats each page's first notes sentence: a broken link there is reported once, on its page.
  const reported = new Set(issues.filter((i) => i.kind === 'broken-link').map((i) => i.detail));
  issues.push(...linkIssues('index', readText(wikiPaths(out).index) ?? '', known).filter((i) => !reported.has(i.detail)));
  const live = entries.filter(([, e]) => !e.orphaned);
  issues.push(...missingPages(out, live));
  for (const [slug, e] of topPages(live, top)) if (!e.notes) issues.push({ slug, kind: 'empty-notes', detail: `one of the pages holding most of the PageRank (${e.pagerank}) has no agent notes` });
  return { issues };
}
