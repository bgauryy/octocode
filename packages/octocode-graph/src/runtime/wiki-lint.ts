// Check graph pages, links, source citations and the freshness of any authored notes.
import { graphCli } from './invocation.ts';
import { isAbsolute, join, relative, resolve } from 'node:path';
import {
  ISOLATED,
  RESERVED,
  notesOf,
  pageFile,
  readMap,
  readText,
  staleMembers,
  wikiPaths,
  type WikiEntry,
} from './wiki-store.ts';
import type { GraphData } from './types.ts';

/** One problem lintWiki found, on a page (or on index, map.json or data.json). */
export interface LintIssue {
  slug: string;
  kind:
    | 'citation-file'
    | 'citation-line'
    | 'broken-link'
    | 'missing-graph'
    | 'missing-page'
    | 'missing-file'
    | 'orphaned'
    | 'stale-notes'
    | 'empty-notes';
  detail: string;
}

interface LintContext {
  out: string;
  root: string;
  lines: (path: string) => number | null;
  known: ReadonlySet<string>;
}

// path:line, path:line-line or path:line:column, where path has an extension; URLs (://host:port) are left out.
const CITE =
  /(?<![\w/.:@-])((?:[\w@+.-]+\/)*[\w@+-][\w@+.-]*\.[A-Za-z0-9]+):(\d+)(?:[-–](\d+))?/g;
const LINK = /\[\[([^\]\n]+)\]\]/g;

const countLines = (text: string) =>
  text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0);

// Line count of a root-relative file, or null when it is not a readable file inside the root.
function lineCounter(root: string) {
  const cache = new Map<string, number | null>();
  return (path: string) => {
    if (!cache.has(path)) {
      const abs = resolve(root, path),
        rel = relative(root, abs);
      const text =
        rel.startsWith('..') || isAbsolute(rel) ? null : readText(abs);
      cache.set(path, text === null ? null : countLines(text));
    }
    return cache.get(path) ?? null;
  };
}

function citationIssues(
  slug: string,
  notes: string,
  lines: LintContext['lines']
) {
  const issues: LintIssue[] = [];
  for (const [text, path, from, to] of notes.matchAll(CITE)) {
    const n = lines(path);
    if (n === null)
      issues.push({
        slug,
        kind: 'citation-file',
        detail: `${text}: ${path} is not a file under the root`,
      });
    else if (Number(from) < 1 || Number(to ?? from) > n)
      issues.push({
        slug,
        kind: 'citation-line',
        detail: `${text}: ${path} has ${n} lines`,
      });
  }
  return issues;
}

function linkIssues(
  slug: string,
  markdown: string,
  known: ReadonlySet<string>
): LintIssue[] {
  return [...markdown.matchAll(LINK)]
    .map(m => m[1].trim())
    .filter(s => !known.has(s))
    .map(s => ({
      slug,
      kind: 'broken-link',
      detail: `[[${s}]] names no page`,
    }));
}

function readData(out: string) {
  const text = readText(join(out, 'data.json'));
  // data.json is this package's own output; missingPages checks it carries community metrics.
  return text ? (JSON.parse(text) as GraphData | null) : null;
}

// Each community of more than one file in data.json needs a live page with the same id and members.
function missingPages(
  out: string,
  live: readonly [string, WikiEntry][]
): LintIssue[] {
  const D = readData(out);
  if (!D?.metrics?.communities)
    return [
      {
        slug: 'data.json',
        kind: 'missing-graph',
        detail:
          'data.json with community metrics is required to check wiki coverage',
      },
    ];
  const byCommunity = new Map(
    live.map(([slug, e]) => [e.community, { slug, e }])
  );
  return D.metrics.communities
    .filter(c => c.size > 1)
    .flatMap((c): LintIssue[] => {
      const p = byCommunity.get(c.id);
      const same =
        p &&
        p.e.size === c.size &&
        p.e.members.every((id, i) => id === c.members[i]);
      if (same) return [];
      const detail = p
        ? `community ${c.id} (${c.label}) changed since [[${p.slug}]] was built: run ${graphCli('wiki build')}`
        : `community ${c.id} (${c.label}, ${c.size} files) has no page: run ${graphCli('wiki build')}`;
      return [{ slug: p?.slug ?? '-', kind: 'missing-page', detail }];
    });
}

function pageIssues(
  slug: string,
  e: WikiEntry,
  { out, root, lines, known }: LintContext
): LintIssue[] {
  const markdown = readText(pageFile(out, slug));
  if (markdown === null)
    return [
      {
        slug,
        kind: 'missing-file',
        detail: `${pageFile(out, slug)} is missing: run ${graphCli('wiki build')}`,
      },
    ];
  const issues = [
    ...citationIssues(slug, notesOf(markdown), lines),
    ...linkIssues(slug, markdown, known),
  ];
  if (e.orphaned)
    issues.push({
      slug,
      kind: 'orphaned',
      detail: `no community matches this page since ${e.orphaned.slice(0, 10)}: move its notes to the pages holding ${e.members.length} former members, then clear them`,
    });
  const stale = e.notes ? staleMembers(root, e, new Set()) : [];
  if (stale.length)
    issues.push({
      slug,
      kind: 'stale-notes',
      detail: `changed since the notes were written: ${stale.join(', ')}`,
    });
  return issues;
}

// { issues: [{ slug, kind, detail }] }; `root` defaults to the repository the wiki was built from.
export function lintWiki({
  out,
  root,
  top = 0,
}: {
  out: string;
  root?: string | null;
  top?: number;
}): { issues: LintIssue[] } {
  if (!Number.isSafeInteger(top) || top < 0)
    throw new Error('--top needs a nonnegative integer');
  const map = readMap(out, null);
  if (!map)
    return {
      issues: [
        {
          slug: 'map.json',
          kind: 'missing-file',
          detail: `wiki/map.json is unavailable: run ${graphCli('wiki build')}`,
        },
      ],
    };
  const base = resolve(root ?? map.root ?? '.');
  const entries = Object.entries(map.pages);
  const known = new Set([...entries.map(([s]) => s), ...RESERVED]);
  const ctx = { out, root: base, lines: lineCounter(base), known };
  const issues = entries.flatMap(([slug, e]) => pageIssues(slug, e, ctx));
  // index.md repeats each page's first notes sentence: a broken link there is reported once, on its page.
  const reported = new Set(
    issues.filter(i => i.kind === 'broken-link').map(i => i.detail)
  );
  const index = readText(wikiPaths(out).index);
  if (index === null)
    issues.push({
      slug: 'index',
      kind: 'missing-file',
      detail: `wiki/index.md is unavailable: run ${graphCli('wiki build')}`,
    });
  else
    issues.push(
      ...linkIssues('index', index, known).filter(i => !reported.has(i.detail))
    );
  const live = entries.filter(([, e]) => !e.orphaned);
  issues.push(...missingPages(out, live));
  if (top > 0) {
    const ranked = live
      .filter(([slug]) => slug !== ISOLATED)
      .toSorted((a, b) => b[1].pagerank - a[1].pagerank);
    for (const [slug, page] of ranked.slice(0, top)) {
      if (!page.notes)
        issues.push({
          slug,
          kind: 'empty-notes',
          detail: `top-ranked page (PageRank ${page.pagerank}) has no notes`,
        });
    }
  }
  return { issues };
}
