#!/usr/bin/env node
// Union the callers one graphQuery reply and one lspSearch reply report for the same target, deduplicated by
// caller location, with the rows only one tool found and every coverage gap either reply discloses.
import { readFileSync } from 'node:fs';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const HELP = `Union the callers of one symbol from a graphQuery reply and an lspSearch reply. Reads only; writes nothing.

Usage:
  node scripts/callers-union.mjs <graph-reply.json> <lsp-reply.json>
  node scripts/callers-union.mjs --help

Inputs: the saved JSON replies (the CLI's --json output, or the MCP reply text) of one query each, same target:
  graph-reply.json  graphQuery {"operation":"callers","path":"<built root>","target":"<file#symbol id>"}
  lsp-reply.json    lspSearch {"operation":"callers","path":"<declaring file>","symbolName":"<name>","line":<n>}
  A file holds one reply, a JSON array of reply pages, or one reply per line (cat page1.json page2.json > lsp.json).
  A reply with a top-level next has more pages: add each page or the union stays a lower bound.

Matching: a caller is its declaration path plus its name line (graph rows: path and line; file rows: id; lsp
rows: matches[].path or files[].path, and line). Module-level callers (graph kind file, lsp kind module) match as
<path>:module. Rows left over pair when path and symbolName agree and their line ranges overlap
(matchedBy "overlap"), which absorbs a decorator or attribute line one tool counts and the other does not.

Output (JSON on stdout):
  ok            true
  target        {id, path, line, symbolName} the graph reply's target; an lspSearch reply does not restate its
                anchor, so pass one asked about that same declaration
  counts        {union, both, graphOnly, lspOnly}
  callers       [{key, path, line, endLine, symbolName, kind, sources, matchedBy?, lspKey?, graph?, lsp?, sitesDiffer?}]
                key is the graph location; lspKey the lsp location when matchedBy is overlap
                graph: {id, depth, sites (count), firstSiteLine, confidence, via}; lsp: {depth, sites [{path,line,column}]}
                sitesDiffer: both tools list the caller at depth 1 with different call-site counts
  graphOnly     [key] callers only graphQuery found; lspOnly: [key] callers only lspSearch found
  coverage      {graph, lsp}: the coverage fields each reply states (basis, callExtraction, lsp level,
                isPartial, partialReasons, warnings) and unfinishedPages
  gaps          [{tool, reason}] everything that keeps the union a lower bound: unfinishedPages,
                each partialReasons entry, isPartial, coverage partial or error, callExtraction other than
                supported, lsp level syntax or text (unverified)
  complete      true only when gaps is empty

Exit codes: 0 union written (read complete and gaps before claiming a count or an absence); 2 invalid input:
unreadable or non-JSON file, a reply without results, replies to several queries in one file, an error reply,
or the files in the wrong order ({ok:false, error:{code, message}}); 1 unexpected failure.
`;

class InputError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** Reply pages from file text: one JSON reply, a JSON array of pages, or one JSON reply per line. */
export function parsePages(text, label) {
  const trimmed = text.trim();
  if (!trimmed) throw new InputError('emptyReply', `${label} is empty`);
  try {
    const value = JSON.parse(trimmed);
    return Array.isArray(value) ? value : [value];
  } catch {
    const pages = [];
    text.split('\n').forEach((line, index) => {
      if (!line.trim()) return;
      try {
        pages.push(JSON.parse(line));
      } catch (error) {
        throw new InputError('invalidJson', `${label} line ${index + 1}: ${errorMessage(error)}`);
      }
    });
    return pages;
  }
}

/** The result rows of one query across its pages, and whether the last page still offers a next page. */
function queryResults(pages, label) {
  const results = [];
  pages.forEach((page, index) => {
    if (!page || typeof page !== 'object' || !Array.isArray(page.results)) {
      throw new InputError('notAReply', `${label} page ${index + 1} has no results[]: save the tool's JSON reply (--json)`);
    }
    results.push(...page.results);
  });
  const queries = new Set(results.map((result) => result?.index ?? 0));
  if (queries.size > 1) throw new InputError('severalQueries', `${label} answers ${queries.size} queries; pass one query's pages per file`);
  for (const result of results) {
    const data = result?.data ?? {};
    if (result?.status === 'error' || data.errorCode) {
      throw new InputError('errorReply', `${label} is an error reply (${data.errorCode ?? 'error'}): ${data.error ?? 'see the reply'}`);
    }
  }
  return { data: results.map((result) => result?.data ?? {}), unfinishedPages: Boolean(pages.at(-1)?.next) };
}

const unique = (values) => [...new Set(values.filter((value) => value !== undefined && value !== null))];

function graphCallers(data) {
  const rows = [];
  for (const page of data) {
    for (const row of page.rows ?? []) {
      const hash = typeof row.id === 'string' ? row.id.indexOf('#') : -1;
      const path = row.path ?? (hash < 0 ? row.id : row.id.slice(0, hash));
      const module = row.kind === 'file' || (hash < 0 && row.line === undefined);
      rows.push({
        path, module, line: module ? null : row.line, endLine: module ? null : row.endLine ?? row.line,
        symbolName: row.symbolName ?? null, kind: row.kind ?? null, depth: row.depth ?? 1,
        detail: { id: row.id, depth: row.depth ?? 1, sites: row.sites ?? null, firstSiteLine: row.edgeLine ?? null, confidence: row.confidence ?? null, via: row.via ?? null },
      });
    }
  }
  return rows;
}

function lspCallers(data) {
  const rows = [];
  for (const page of data) {
    for (const file of page.files ?? []) {
      for (const match of file.matches ?? []) {
        const module = match.kind === 'module';
        rows.push({
          path: match.path ?? file.path, module, line: module ? null : match.line, endLine: module ? null : match.endLine ?? match.line,
          symbolName: match.symbolName ?? null, kind: match.kind ?? null, depth: match.level ?? 1,
          detail: { depth: match.level ?? 1, sites: (match.sites ?? []).map((site) => ({ path: file.path, line: site.line, column: site.column })) },
        });
      }
    }
  }
  return rows;
}

const keyOf = (row) => `${row.path}:${row.module ? 'module' : row.line}`;

/** One row per caller location; a location listed twice (two depths, two pages) keeps its shallowest listing. */
function byKey(rows) {
  const out = new Map();
  for (const row of rows) {
    const key = keyOf(row);
    const seen = out.get(key);
    if (!seen || row.depth < seen.depth) out.set(key, row);
  }
  return out;
}

const overlaps = (a, b) => !a.module && !b.module && a.line <= b.endLine && b.line <= a.endLine;

/** Union of graph and lsp caller rows: exact location first, then same path and name with overlapping ranges. */
export function unionCallers(graphRows, lspRows) {
  const graph = byKey(graphRows);
  const lsp = byKey(lspRows);
  const pairs = [];
  for (const [key, row] of graph) {
    if (lsp.has(key)) {
      pairs.push({ key, graph: row, lsp: lsp.get(key), matchedBy: 'line' });
      graph.delete(key);
      lsp.delete(key);
    }
  }
  for (const [key, row] of graph) {
    const match = [...lsp].find(([, other]) => other.path === row.path && row.symbolName && other.symbolName === row.symbolName && overlaps(row, other));
    if (!match) continue;
    pairs.push({ key, graph: row, lsp: match[1], matchedBy: 'overlap' });
    graph.delete(key);
    lsp.delete(match[0]);
  }
  const callers = [
    ...pairs.map(({ key, graph: g, lsp: l, matchedBy }) => {
      const caller = describe(key, g, ['graph', 'lsp']);
      caller.matchedBy = matchedBy;
      if (matchedBy === 'overlap') caller.lspKey = keyOf(l);
      caller.graph = g.detail;
      caller.lsp = l.detail;
      if (g.depth === 1 && l.depth === 1 && g.detail.sites !== null && g.detail.sites !== l.detail.sites.length) caller.sitesDiffer = true;
      return caller;
    }),
    ...[...graph].map(([key, row]) => ({ ...describe(key, row, ['graph']), graph: row.detail })),
    ...[...lsp].map(([key, row]) => ({ ...describe(key, row, ['lsp']), lsp: row.detail })),
  ].sort((a, b) => a.path.localeCompare(b.path) || (a.line ?? 0) - (b.line ?? 0) || a.key.localeCompare(b.key));
  return {
    callers,
    graphOnly: callers.filter((caller) => caller.sources.length === 1 && caller.sources[0] === 'graph').map((caller) => caller.key),
    lspOnly: callers.filter((caller) => caller.sources.length === 1 && caller.sources[0] === 'lsp').map((caller) => caller.key),
  };
}

function describe(key, row, sources) {
  return { key, path: row.path, line: row.line, endLine: row.endLine, symbolName: row.symbolName, kind: row.kind, sources };
}

function graphCoverage(data, unfinishedPages) {
  const coverage = data.find((page) => page.coverage)?.coverage ?? {};
  const partialReasons = unique(data.flatMap((page) => page.partialReasons ?? []));
  const out = { ...coverage, partialReasons, warnings: unique(data.flatMap((page) => page.warnings ?? [])), unfinishedPages };
  const gaps = partialReasons.map((reason) => ({ tool: 'graph', reason }));
  if (coverage.callExtraction && coverage.callExtraction !== 'supported') gaps.push({ tool: 'graph', reason: `callExtraction ${coverage.callExtraction}` });
  if (unfinishedPages) gaps.push({ tool: 'graph', reason: 'unfinishedPages' });
  return { out, gaps };
}

function lspCoverage(data, unfinishedPages) {
  const level = data.find((page) => page.lsp?.level)?.lsp.level ?? null;
  const states = unique(data.map((page) => page.coverage?.state));
  const partialReasons = unique(data.flatMap((page) => page.partialReasons ?? []));
  const isPartial = data.some((page) => page.isPartial === true);
  const out = {
    level, coverage: states, isPartial, partialReasons, warnings: unique(data.flatMap((page) => page.warnings ?? [])),
    root: data.find((page) => page.root)?.root ?? null, unfinishedPages,
  };
  const gaps = partialReasons.map((reason) => ({ tool: 'lsp', reason }));
  if (isPartial && !partialReasons.length) gaps.push({ tool: 'lsp', reason: 'isPartial' });
  for (const state of states) if (state === 'partial' || state === 'error') gaps.push({ tool: 'lsp', reason: `coverage ${state}` });
  if (level === 'syntax' || level === 'text') gaps.push({ tool: 'lsp', reason: `level ${level}` });
  if (unfinishedPages) gaps.push({ tool: 'lsp', reason: 'unfinishedPages' });
  return { out, gaps };
}

function targetOf(data) {
  const target = data.find((page) => page.target)?.target;
  return target ? { id: target.id ?? null, path: target.path ?? null, line: target.line ?? null, symbolName: target.symbolName ?? null } : null;
}

/** The full report for one graph reply and one lsp reply, each given as parsed pages. */
export function callersUnion(graphPages, lspPages) {
  const graph = queryResults(graphPages, 'graph reply');
  const lsp = queryResults(lspPages, 'lsp reply');
  const graphShaped = (data) => data.some((page) => Array.isArray(page.rows) || page.target);
  const lspShaped = (data) => data.some((page) => Array.isArray(page.files));
  if (!graphShaped(graph.data) && graphShaped(lsp.data)) throw new InputError('wrongOrder', 'Pass the graphQuery reply first and the lspSearch reply second');
  if (!graphShaped(graph.data)) throw new InputError('notGraphCallers', 'The first file has no graphQuery rows or target: save a graphQuery callers reply');
  if (!lspShaped(lsp.data)) throw new InputError('notLspCallers', 'The second file has no lspSearch files[]: save an lspSearch callers reply');
  const union = unionCallers(graphCallers(graph.data), lspCallers(lsp.data));
  const graphCov = graphCoverage(graph.data, graph.unfinishedPages);
  const lspCov = lspCoverage(lsp.data, lsp.unfinishedPages);
  const gaps = [...graphCov.gaps, ...lspCov.gaps];
  const both = union.callers.filter((caller) => caller.sources.length === 2).length;
  return {
    ok: true,
    target: targetOf(graph.data),
    counts: { union: union.callers.length, both, graphOnly: union.graphOnly.length, lspOnly: union.lspOnly.length },
    callers: union.callers,
    graphOnly: union.graphOnly,
    lspOnly: union.lspOnly,
    coverage: { graph: graphCov.out, lsp: lspCov.out },
    gaps,
    complete: gaps.length === 0,
  };
}

function read(file, label) {
  try {
    return parsePages(readFileSync(file, 'utf8'), label);
  } catch (error) {
    if (error instanceof InputError) throw error;
    throw new InputError('unreadable', `${label} ${file}: ${errorMessage(error)}`);
  }
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    return;
  }
  let report;
  try {
    const unknown = argv.find((arg) => arg.startsWith('-'));
    if (unknown) throw new InputError('invalidArguments', `Unknown option: ${unknown}`);
    if (argv.length !== 2) throw new InputError('invalidArguments', 'Pass two files: <graph-reply.json> <lsp-reply.json>; see --help.');
    report = callersUnion(read(argv[0], 'graph reply'), read(argv[1], 'lsp reply'));
  } catch (error) {
    if (error instanceof InputError) fail(error.code, error.message, 2);
    fail('unionFailed', errorMessage(error), 1);
  }
  console.log(JSON.stringify(report, null, 2));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
