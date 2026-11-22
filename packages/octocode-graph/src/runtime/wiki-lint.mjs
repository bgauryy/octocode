// Check graph pages, links, source citations and the freshness of any authored notes.
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
  if (!D?.metrics?.communities) return [{ slug: 'data.json', kind: 'missing-graph', detail: 'data.json with community metrics is required to check wiki coverage' }];
  const byCommunity = new Map(live.map(([slug, e]) => [e.community, { slug, e }]));
  return D.metrics.communities.filter((c) => c.size > 1).flatMap((c) => {
    const p = byCommunity.get(c.id);
    const same = p && p.e.size === c.size && p.e.members.every((id, i) => id === c.members[i]);
    if (same) return [];
    const detail = p ? `community ${c.id} (${c.label}) changed since [[${p.slug}]] was built: run wiki.mjs build` : `community ${c.id} (${c.label}, ${c.size} files) has no page: run wiki.mjs build`;
    return [{ slug: p?.slug ?? '-', kind: 'missing-page', detail }];
  });
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
export function lintWiki({ out, root, top = 0 }) {
  if (!Number.isSafeInteger(top) || top < 0) throw new Error('--top needs a nonnegative integer');
  const map = readMap(out, null);
  if (!map) return { issues: [{ slug: 'map.json', kind: 'missing-file', detail: 'wiki/map.json is unavailable: run wiki.mjs build' }] };
  const base = resolve(root ?? map.root ?? '.');
  const entries = Object.entries(map.pages);
  const known = new Set([...entries.map(([s]) => s), ...RESERVED]);
  const ctx = { out, root: base, lines: lineCounter(base), known };
  const issues = entries.flatMap(([slug, e]) => pageIssues(slug, e, ctx));
  // index.md repeats each page's first notes sentence: a broken link there is reported once, on its page.
  const reported = new Set(issues.filter((i) => i.kind === 'broken-link').map((i) => i.detail));
  const index = readText(wikiPaths(out).index);
  if (index === null) issues.push({ slug: 'index', kind: 'missing-file', detail: 'wiki/index.md is unavailable: run wiki.mjs build' });
  else issues.push(...linkIssues('index', index, known).filter((i) => !reported.has(i.detail)));
  const live = entries.filter(([, e]) => !e.orphaned);
  issues.push(...missingPages(out, live));
  if (top > 0) {
    const ranked = live.filter(([slug]) => slug !== ISOLATED).toSorted((a, b) => b[1].pagerank - a[1].pagerank);
    for (const [slug, page] of ranked.slice(0, top)) {
      if (!page.notes) issues.push({ slug, kind: 'empty-notes', detail: `top-ranked page (PageRank ${page.pagerank}) has no notes` });
    }
  }
  return { issues };
}
