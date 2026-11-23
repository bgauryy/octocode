// Links beside the code graph (links-docs.ts):
//   links [<file|symbol|doc>] [--unresolved] [--stale] [--rule <rule>]
//     every link (doc line → file, directory, citation or symbol) or, with --unresolved, every reference that named
//     no single target with its reason and candidates. A file narrows to links from or to it; --stale keeps the
//     citations whose cited code changed under an unchanged doc line. Complete rows, each with its doc `path:line`.
// `about` lists a file's or a symbol's `documentedBy` rows from the same data (documentation).
import type { Link, UnresolvedLink } from './types.ts';
import { QueryError, type QueryIndex } from './query-index.ts';
import {
  plural,
  sourcePath,
  type OpResult,
  type QueryArgs,
  type Row,
} from './query-ops.ts';

const RULES = [
  'md-link',
  'md-path',
  'md-cite',
  'md-symbol',
  'manifest',
  'tsconfig',
  'contract',
  'md-contract',
  'agent',
];

const label = (path: string) => path.split('/').pop() || path;

// Whether `arg` names this path: the path itself, a symbol in it, or a unique-looking suffix of it.
const names = (path: string, arg: string) =>
  path === arg ||
  path.startsWith(`${arg}#`) ||
  path.endsWith(`/${arg}`) ||
  path.split('#')[0].endsWith(`/${arg}`);

export function linkRow(l: Link): Row {
  return {
    id: l.source,
    label: label(l.source),
    chunk: null,
    loc: { path: l.source, line: l.line },
    target: l.target,
    relation: l.relation,
    rule: l.rule,
    check: l.check,
    ...(l.lines && { lines: l.lines }),
  };
}

function unresolvedRow(u: UnresolvedLink): Row {
  return {
    id: u.source,
    label: label(u.source),
    chunk: null,
    loc: { path: u.source, line: u.line },
    text: u.text,
    rule: u.rule,
    reason: u.reason,
    ...(u.candidates && { candidates: u.candidates }),
  };
}

/** The doc lines that link to a file or a symbol (and, for a file, to its symbols). */
export function documentedBy(I: QueryIndex, id: string): Row[] {
  return (I.D.links ?? [])
    .filter(
      l =>
        l.target === id || (!id.includes('#') && l.target.startsWith(`${id}#`))
    )
    .map(l => ({ ...linkRow(l), role: 'documentedBy' }));
}

export function links(I: QueryIndex, { args }: QueryArgs): OpResult {
  const flags = new Set(args.filter(a => a.startsWith('--')));
  const at = args.indexOf('--rule');
  const rule = at >= 0 ? args[at + 1] : undefined;
  if (at >= 0 && !RULES.includes(rule ?? ''))
    throw new QueryError(`--rule takes one of ${RULES.join(', ')}`);
  for (const f of flags)
    if (!['--unresolved', '--stale', '--rule'].includes(f))
      throw new QueryError(
        `links takes --unresolved, --stale or --rule <rule>, not ${f}`
      );
  const subject = args.find(
    (a, i) => !a.startsWith('--') && args[i - 1] !== '--rule'
  );
  const all = I.D.links ?? [];
  if (!all.length && !(I.D.unresolvedLinks ?? []).length)
    return {
      rows: [],
      edges: [],
      nodes: [],
      stats: { links: 0, unresolved: 0 },
      summary:
        'no links: this graph has no documentation links (build it again to read its docs)',
    };
  const byRule = (r: { rule: string }) => !rule || r.rule === rule;
  if (flags.has('--unresolved')) {
    const list = (I.D.unresolvedLinks ?? []).filter(
      u =>
        byRule(u) &&
        (!subject ||
          names(u.source, subject) ||
          u.candidates?.some(c => names(c, subject)))
    );
    const reasons: Record<string, number> = {};
    for (const u of list) reasons[u.reason] = (reasons[u.reason] ?? 0) + 1;
    const first = list[0];
    return {
      rows: list.map(unresolvedRow),
      edges: [],
      nodes: [],
      stats: { unresolved: list.length, reasons },
      ...(first && {
        next: {
          read: {
            tool: 'localFetch' as const,
            input: {
              queries: [
                {
                  path: sourcePath(I, first.source),
                  ranges: [{ startLine: first.line, endLine: first.line }],
                },
              ],
            },
          },
        },
      }),
      summary: `${plural(list.length, 'unresolved reference')}${subject ? ` for ${subject}` : ''} (${
        Object.entries(reasons)
          .map(([k, n]) => `${k} ${n}`)
          .join(', ') || 'none'
      })`,
    };
  }
  const list = all.filter(
    l =>
      byRule(l) &&
      (!flags.has('--stale') || l.check === 'stale') &&
      (!subject || names(l.source, subject) || names(l.target, subject))
  );
  const counts: Record<string, number> = {};
  for (const l of list) counts[l.rule] = (counts[l.rule] ?? 0) + 1;
  const stale = list.filter(l => l.check === 'stale').length;
  const graphFiles = (target: string) => target.split('#')[0];
  return {
    rows: list.map(linkRow),
    edges: list
      .filter(l => I.file.has(graphFiles(l.target)))
      .map(l => [l.source, graphFiles(l.target)] as [string, string]),
    nodes: [
      ...new Set(
        list.map(l => graphFiles(l.target)).filter(t => I.file.has(t))
      ),
    ],
    stats: { links: list.length, byRule: counts, stale },
    summary: `${plural(list.length, flags.has('--stale') ? 'stale citation' : 'link')}${subject ? ` from or to ${subject}` : ''} (${
      Object.entries(counts)
        .map(([k, n]) => `${k} ${n}`)
        .join(', ') || 'none'
    })${!flags.has('--stale') && stale ? `; ${stale} stale: links --stale` : ''}`,
  };
}
