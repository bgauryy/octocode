// Links an agent proposes (F6 of the graph end-to-end RFC): what static rules cannot settle (an ambiguous or
// unconfirmed doc mention, a config a file reads by a computed path, a concept spread over files) is recorded by
// an agent with quoted evidence, and the tool decides. A link is stored only when every quote is at its cited
// lines and occurs exactly once in its file; the server keeps its own hash of each span, never the agent's copy.
// Every build and refresh checks the quotes again: a quote found once elsewhere moves the anchor (`ok`), a changed
// one makes the link `stale`, a missing file `missing`. Stored in <out>/links.json, which rebuilds keep, and
// mirrored into data.json `links` (rule `agent`) so every query sees them; the code metrics never count them.
//
// Confidence (the model.json values): `candidate` when the quotes verify; `likely` when an anchor's quote also
// names the other endpoint (its file name or symbol name); `confirmed` when a reviewer other than the proposer
// accepted it at the current hashes. A changed quote drops a link back to what the machine checks show.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { writeGraph } from './lib/write-graph.ts';
import type { GraphData, Link } from './types.ts';

export type ProposedRelation = 'documents' | 'configures' | 'relates';
export type Confidence = 'candidate' | 'likely' | 'confirmed';

/** One cited span: a file, the lines, and text that must occur there and nowhere else in the file. */
export interface Anchor {
  path: string;
  line: number;
  endLine?: number;
  quote: string;
}

/** What an agent sends: the link, the claim it makes, and its evidence. */
export interface LinkProposal {
  relation: ProposedRelation;
  source: string;
  target: string;
  claim: string;
  anchors: Anchor[];
  proposedBy: string;
}

export interface ProposedLink extends LinkProposal {
  id: string;
  anchors: (Anchor & { hash: string })[];
  confidence: Confidence;
  reviewedBy?: string;
  check: 'ok' | 'stale' | 'missing';
}

/** Why a proposal was refused (the wiki-lint citation kinds, and the quote checks). */
export type Refusal =
  | 'citation-file'
  | 'citation-line'
  | 'quote-mismatch'
  | 'quote-not-unique'
  | 'unknown-endpoint'
  | 'missing-anchor'
  | 'invalid';

export class LinkRefused extends Error {
  readonly reason: Refusal;
  constructor(reason: Refusal, message: string) {
    super(`${reason}: ${message}`);
    this.reason = reason;
  }
}

const RELATIONS: ProposedRelation[] = ['documents', 'configures', 'relates'];
const STORE = 'links.json';
const normalize = (text: string) => text.replace(/\s+/g, ' ').trim();
const sha = (text: string) =>
  createHash('sha256').update(text).digest('hex').slice(0, 16);
const fileOf = (id: string) => id.split('#')[0];

export function readProposed(out: string): ProposedLink[] {
  const file = join(out, STORE);
  if (!existsSync(file)) return [];
  // links.json is this module's own output.
  return (JSON.parse(readFileSync(file, 'utf8')) as { links: ProposedLink[] })
    .links;
}

function writeProposed(out: string, links: ProposedLink[]) {
  writeGraph(join(out, STORE), { version: 1, links });
}

// The start line of every occurrence of a quote in a file, whitespace-normalized on both sides: the file's text is
// collapsed the same way while each character keeps the line it came from, so a quote may span lines.
function occurrences(lines: string[], quote: string) {
  const want = normalize(quote);
  let flat = '';
  const lineAt: number[] = [];
  let space = true;
  lines.forEach((line, i) => {
    for (const c of `${line}\n`) {
      if (/\s/.test(c)) {
        if (!space) {
          flat += ' ';
          lineAt.push(i + 1);
        }
        space = true;
      } else {
        flat += c;
        lineAt.push(i + 1);
        space = false;
      }
    }
  });
  const found = new Set<number>();
  for (let at = flat.indexOf(want); at >= 0; at = flat.indexOf(want, at + 1))
    found.add(lineAt[at]);
  return [...found];
}

interface Checked {
  anchor: Anchor & { hash: string };
  state: 'ok' | 'moved' | 'stale' | 'missing';
}

function checkAnchor(root: string, a: Anchor): Checked {
  const file = join(root, a.path);
  if (!existsSync(file))
    return {
      anchor: { ...a, hash: sha(normalize(a.quote)) },
      state: 'missing',
    };
  const lines = readFileSync(file, 'utf8').split('\n');
  const at = occurrences(lines, a.quote);
  const end = a.endLine ?? a.line;
  const hash = sha(normalize(a.quote));
  const here = at.filter(n => n >= a.line && n <= end);
  if (here.length && at.length === 1)
    return { anchor: { ...a, hash }, state: 'ok' };
  // Swimm's re-sync: the same text, found once elsewhere, moves the anchor.
  if (at.length === 1) {
    const length = end - a.line;
    return {
      anchor: {
        ...a,
        line: at[0],
        ...(a.endLine && { endLine: at[0] + length }),
        hash,
      },
      state: 'moved',
    };
  }
  return { anchor: { ...a, hash }, state: 'stale' };
}

// The endpoints a link may name: graph files and symbols, and any file under the root (a doc or a config).
function knows(
  D: Pick<GraphData, 'files' | 'symbols'>,
  root: string,
  id: string
) {
  if (id.includes('#')) return (D.symbols ?? []).some(s => s.id === id);
  return (D.files ?? []).some(f => f.id === id) || existsSync(join(root, id));
}

// `likely`: an anchor quoting one endpoint also names the other (its file name or its symbol name).
function machineConfidence(p: LinkProposal): Confidence {
  const names = (id: string) => [
    fileOf(id).split('/').pop()!,
    ...(id.includes('#') ? [id.split('#')[1].split('.').pop()!] : []),
  ];
  const mentions = (a: Anchor, id: string) =>
    names(id).some(n => a.quote.includes(n));
  return p.anchors.some(
    a =>
      (a.path === fileOf(p.source) && mentions(a, p.target)) ||
      (a.path === fileOf(p.target) && mentions(a, p.source))
  )
    ? 'likely'
    : 'candidate';
}

export const linkId = (
  p: Pick<LinkProposal, 'relation' | 'source' | 'target' | 'anchors'>
) =>
  sha(
    JSON.stringify([
      p.relation,
      p.source,
      p.target,
      p.anchors.map(a => [a.path, normalize(a.quote)]).toSorted(),
    ])
  );

function validate(p: unknown): LinkProposal {
  const o = p as Partial<LinkProposal> | null;
  if (!o || typeof o !== 'object')
    throw new LinkRefused('invalid', 'a link is a JSON object');
  if (!RELATIONS.includes(o.relation as ProposedRelation))
    throw new LinkRefused(
      'invalid',
      `relation must be one of ${RELATIONS.join(', ')}`
    );
  for (const k of ['source', 'target', 'claim', 'proposedBy'] as const)
    if (typeof o[k] !== 'string' || !o[k].trim())
      throw new LinkRefused('invalid', `${k} is required`);
  if (!Array.isArray(o.anchors) || !o.anchors.length)
    throw new LinkRefused(
      'missing-anchor',
      'anchors: one {path, line, quote} per endpoint file at least'
    );
  for (const a of o.anchors)
    if (
      !a ||
      typeof a.path !== 'string' ||
      !Number.isInteger(a.line) ||
      typeof a.quote !== 'string' ||
      !normalize(a.quote)
    )
      throw new LinkRefused(
        'invalid',
        'each anchor needs path, line and a non-empty quote'
      );
  return o as LinkProposal;
}

/**
 * Verify and store a proposed link in the run at `out` (its graph root is data.json `section`). Returns the stored
 * link; throws LinkRefused with the reason when a quote, a line or an endpoint does not hold. Proposing the same
 * link again (same relation, endpoints and quotes) updates it in place.
 */
export function proposeLink(out: string, input: unknown): ProposedLink {
  const p = validate(input);
  const D = readGraph(out);
  const root = D.section;
  for (const id of [p.source, p.target])
    if (!knows(D, root, id))
      throw new LinkRefused(
        'unknown-endpoint',
        `${id} is no graph file, symbol or file under ${root}`
      );
  for (const id of [p.source, p.target])
    if (!p.anchors.some(a => a.path === fileOf(id)))
      throw new LinkRefused(
        'missing-anchor',
        `no anchor in ${fileOf(id)}, the file of endpoint ${id}`
      );
  const anchors = p.anchors.map(a => {
    const file = join(root, a.path);
    if (!existsSync(file))
      throw new LinkRefused(
        'citation-file',
        `${a.path} is not a file under ${root}`
      );
    const lines = readFileSync(file, 'utf8').split('\n');
    const end = a.endLine ?? a.line;
    if (a.line < 1 || end > lines.length || end < a.line)
      throw new LinkRefused(
        'citation-line',
        `${a.path} has ${lines.length} lines, not ${a.line}-${end}`
      );
    const at = occurrences(lines, a.quote);
    if (!at.some(n => n >= a.line && n <= end))
      throw new LinkRefused(
        'quote-mismatch',
        `${a.path}:${a.line}${a.endLine ? `-${end}` : ''} does not hold the quote`
      );
    if (at.length > 1)
      throw new LinkRefused(
        'quote-not-unique',
        `the quote occurs ${at.length} times in ${a.path} (lines ${at.join(', ')}): quote more`
      );
    return { ...a, hash: sha(normalize(a.quote)) };
  });
  const id = linkId(p);
  const list = readProposed(out);
  const link: ProposedLink = {
    ...p,
    id,
    anchors,
    confidence: machineConfidence(p),
    check: 'ok',
  };
  writeProposed(out, [...list.filter(l => l.id !== id), link]);
  mirror(out, D);
  return link;
}

export function removeLink(out: string, id: string) {
  const list = readProposed(out);
  if (!list.some(l => l.id === id))
    throw new LinkRefused('invalid', `no proposed link ${id}`);
  writeProposed(
    out,
    list.filter(l => l.id !== id)
  );
  mirror(out, readGraph(out));
}

/** A reviewer other than the proposer confirms a link whose quotes still hold. */
export function reviewLink(
  out: string,
  id: string,
  reviewer: string
): ProposedLink {
  const list = readProposed(out);
  const link = list.find(l => l.id === id);
  if (!link) throw new LinkRefused('invalid', `no proposed link ${id}`);
  if (!reviewer.trim() || reviewer === link.proposedBy)
    throw new LinkRefused(
      'invalid',
      'a link is confirmed by a reviewer other than its proposer'
    );
  if (link.check !== 'ok')
    throw new LinkRefused(
      'quote-mismatch',
      `the link is ${link.check}: propose it again with current quotes`
    );
  const reviewed = {
    ...link,
    confidence: 'confirmed' as const,
    reviewedBy: reviewer,
  };
  writeProposed(
    out,
    list.map(l => (l.id === id ? reviewed : l))
  );
  mirror(out, readGraph(out));
  return reviewed;
}

/** Check every stored link's quotes against the files now: re-anchor, or mark stale or missing. */
export function recheckProposed(out: string, root: string): ProposedLink[] {
  const list = readProposed(out);
  if (!list.length) return list;
  const next = list.map(l => {
    const checked = l.anchors.map(a => checkAnchor(root, a));
    const missing = checked.some(c => c.state === 'missing');
    const stale = checked.some(c => c.state === 'stale');
    const check: ProposedLink['check'] = missing
      ? 'missing'
      : stale
        ? 'stale'
        : 'ok';
    return {
      ...l,
      anchors: checked.map(c => c.anchor),
      check,
      // A link whose text no longer holds keeps only what the machine checks show.
      ...(check !== 'ok' &&
        l.confidence === 'confirmed' && {
          confidence: machineConfidence(l),
          reviewedBy: undefined,
        }),
    };
  });
  writeProposed(out, next);
  return next;
}

/** Proposed links as data.json links (rule `agent`), witnessed by their first anchor in the source file. */
export function asLinks(list: readonly ProposedLink[]): Link[] {
  return list.map(l => {
    const at = l.anchors.find(a => a.path === fileOf(l.source)) ?? l.anchors[0];
    return {
      source: l.source,
      line: at.line,
      target: l.target,
      relation: l.relation,
      rule: 'agent',
      check: l.check === 'ok' ? 'ok' : 'stale',
      id: l.id,
      confidence: l.confidence,
      proposedBy: l.proposedBy,
      claim: l.claim,
      ...(l.check === 'missing' && { missing: true }),
    };
  });
}

type SavedGraph = GraphData & { section: string };

function readGraph(out: string): SavedGraph {
  const file = join(out, 'data.json');
  if (!existsSync(file))
    throw new LinkRefused(
      'invalid',
      `no data.json in ${out}: build the graph first`
    );
  // data.json is this package's own output.
  return JSON.parse(readFileSync(file, 'utf8')) as SavedGraph;
}

// data.json shows the current proposed links beside the ones its docs gave.
function mirror(out: string, D: SavedGraph) {
  D.links = [
    ...(D.links ?? []).filter(l => l.rule !== 'agent'),
    ...asLinks(readProposed(out)),
  ];
  writeGraph(join(out, 'data.json'), D);
}
