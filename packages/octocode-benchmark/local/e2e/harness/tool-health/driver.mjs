// Execute schema-declared calls verbatim. Continuation kind determines required
// work; names are labels only. Case-provided row identities check completeness.
import { canon, graphInput, leadsIn, short } from './calls.mjs';
export { leadsIn } from './calls.mjs';

export const rowsOf = json => json?.results ?? [];
export const dataOf = row => row?.data;
export const errorCodes = json => [json?.errorCode, ...rowsOf(json).map(row => row.data?.errorCode)].filter(Boolean);
export function statusClass(json) {
  if (!json || typeof json !== 'object') return 'broken';
  if (errorCodes(json).includes('invalidInput')) return 'invalid';
  if (json.errorCode || json.error) return `error:${json.errorCode ?? 'unknown'}`;
  const rows = rowsOf(json);
  if (!rows.length) return 'broken';
  if (rows.some(row => row.status === 'error' || row.data?.errorCode)) return 'error';
  if (rows.every(row => row.status === 'empty')) return 'empty';
  if (rows.every(row => row.status === 'empty' || row.status === 'outOfRange')) return 'outOfRange';
  return 'ok';
}
export const writes = call => call.tool === 'ghCloneRepo' ? 'writes the clone cache (exercised by the ghCloneRepo case)' : null;
function skipReason(lead, { tools, cliOnly, surface, allow }) {
  if (!tools.has(lead.tool)) return 'target is not a contract tool';
  if (surface === 'mcp' && cliOnly.has(lead.tool)) return `${lead.tool} is CLI-only (not on MCP)`;
  return allow.includes(lead.key) ? null : writes(lead);
}

/** Inspect actual metadata without treating repeated valid guidance as a defect. */
export function metadataOf(json) {
  const metadata = { hints: 0, warnings: 0 };
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(walk); return; }
    for (const [key, child] of Object.entries(node)) {
      if (Object.hasOwn(metadata, key) && Array.isArray(child)) metadata[key] += child.length;
      else if (key !== 'content') walk(child);
    }
  };
  rowsOf(json).forEach(walk);
  return metadata;
}

export function createDriver({ validate, validateInput = () => null, validateTransport = () => null, tools, cliOnly, schemaOf }) {
  async function explore(tool, input, { surface, call, maxCalls = 8, maxPages = 12, rows = () => undefined, allow = [] }) {
    const report = { tool, surface, input, calls: [], failures: [], skipped: [], graph: new Set(), chains: [], budgetCut: 0, converged: 0, metadata: [] };
    const fail = (check, detail, at) => report.failures.push({ check, detail: short(detail, 400), at });
    const identity = (name, request) => `${name} ${canon(request)}`;
    const root = { tool, input, label: 'seed', depth: 0, ancestors: new Set(), children: [], executed: false };
    const seen = new Map([[identity(tool, input), root]]);
    const required = [], optional = [root];
    const transportGroups = [];
    let optionalCalls = 0;
    while (required.length || optional.length) {
      const item = required.length ? required.shift() : optional.shift();
      if (item.kind !== 'continue' && optionalCalls >= maxCalls) { report.budgetCut += 1; continue; }
      if (item.kind !== 'continue') optionalCalls += 1;
      const entry = await call(item.tool, item.input);
      if (item === root) report.root = entry;
      const json = entry.json;
      const group = item.responseGroup ?? { responses: [], from: item.label, ended: false };
      if (!item.responseGroup) transportGroups.push(group);
      group.responses.push(entry);
      const cls = statusClass(json);
      report.calls.push({ tool: item.tool, label: item.label, cls, ms: entry.ms, input: item.input });
      const error = validate(item.tool, entry.raw ?? json);
      if (error) fail('contract', `${item.tool} ${error}`, item.label);
      else if (cls === 'broken') fail('response', `${item.tool} returned no JSON rows: ${short(entry.text)}`, item.label);
      const rejected = leadFailure(item, cls, json);
      if (rejected) fail(rejected.check, rejected.detail, item.label);
      report.metadata.push({ tool: item.tool, ...metadataOf(json) });
      if (item.chain) recordPage(item, json, cls, fail);

      const calls = leadsIn(json, schemaOf(item.tool));
      group.ended = !calls.some(lead => lead.kind === 'continue' && lead.key === 'responsePagination.next');
      const here = new Set();
      for (const lead of calls) {
        const id = identity(lead.tool, lead.input);
        const label = lead.kind === 'related'
          ? `${item.label} > related.${lead.tool}:${canon(lead.input)}`
          : `${item.label} > ${lead.container}.${lead.key}`;
        const source = `${item.label}|${item.tool}|${canon(graphInput(item.input))}`;
        if (!cliOnly.has(lead.tool)) report.graph.add(`${source} --${lead.kind}:${lead.kind === 'related' ? 'related' : lead.path}--> ${lead.tool} ${canon(graphInput(lead.input))}`);
        if (repeatedLead({ lead, item, id, here, seen, report, fail })) continue;
        const child = { ...lead, label, depth: lead.kind === 'continue' ? item.depth + 1 : 0, ancestors: new Set([...item.ancestors, identity(item.tool, item.input)]), children: [], executed: false };
        if (lead.kind === 'continue' && lead.key === 'responsePagination.next') child.responseGroup = group;
        seen.set(id, child);
        const reason = skipReason(lead, { tools, cliOnly, surface, allow });
        if (reason) {
          report.skipped.push({ key: `${lead.container}.${lead.key}`, tool: lead.tool, reason });
          if (lead.kind === 'continue') fail('lead.unreachable', reason, label);
          continue;
        }
        const invalid = validateInput(lead.tool, lead.input);
        if (invalid) { fail('lead.input', `${lead.tool} ${String(invalid)}`, label); continue; }
        if (lead.kind === 'continue') {
          if (child.depth >= maxPages) { fail('page.cap', `${lead.key} still has required work after ${maxPages} pages`, label); if (item.chain) item.chain.capped = true; continue; }
          attachChain({ child, item, json, rows, report, fail });
          item.children.push(child);
          required.push(child);
        } else optional.push(child);
      }
      item.executed = true;
      if (item.chain && !calls.some(lead => lead.kind === 'continue' && lead.tool === item.tool && lead.key === item.chain.key)) item.chain.ended = true;
    }
    for (const chain of report.chains) if (!chain.ended && !chain.capped && !chain.problem) fail('page.ended', `${chain.key} stopped before its end`, chain.from);
    for (const group of transportGroups.filter(candidate => candidate.ended)) {
      const problem = validateTransport(surface, group.responses);
      if (problem) fail('transport', problem, group.from);
    }
    return report;
  }
  return { explore, graphParity };
}

function repeatedLead({ lead, item, id, here, seen, report, fail }) {
  if (here.has(id)) {
    if (lead.kind === 'continue') fail('lead.repeat', `${lead.path} repeats required work in this response`, item.label);
    else report.converged += 1;
    return true;
  }
  here.add(id);
  const prior = seen.get(id);
  if (!prior) return false;
  const reason = repeatReason(lead, prior, id, item);
  if (reason) fail('lead.repeat', `${lead.path} repeats ${reason} at ${prior.label}`, item.label);
  else report.converged += 1;
  return true;
}

const completed = node => node.executed && node.children.every(completed);
function leadFailure(item, cls, json) {
  if (!item.kind || cls === 'broken') return null;
  if (capturedRows(item, json)) return null;
  if (cls === 'invalid') return { check: 'lead.accepted', detail: `${item.tool} rejected issued ${item.kind} input: ${errorCodes(json).join(',')}` };
  return item.kind === 'continue' && !['ok', 'empty', 'outOfRange'].includes(cls)
    ? { check: 'lead.execution', detail: `${item.tool} failed required continuation: ${errorCodes(json).join(',') || cls}` } : null;
}
const capturedRows = (item, json) => item.key === 'responsePagination.next' && !json.errorCode && rowsOf(json).length > 0;
const queryIndexOf = (lead, json) => lead.rowIndex === null ? null : rowsOf(json)[lead.rowIndex]?.index;
const mappedRows = (rows, json, mapper, context) => rows.flatMap(row => mapper(dataOf(row), json, context));
function attachChain({ child, item, json, rows, report, fail }) {
  const rowsFn = rows(child.tool, child.key);
  if (child.tool !== item.tool || !rowsFn) return;
  child.chain = item.chain?.key === child.key && item.chain.queryIndex === queryIndexOf(child, json)
    ? item.chain : newChain({ lead: child, item, json, rowsFn }, fail);
  if (child.chain !== item.chain) report.chains.push(child.chain);
}
function repeatReason(lead, prior, identity, item) {
  if (lead.kind !== 'continue') return null;
  if (prior === item || item.ancestors.has(identity)) return 'an ancestor';
  return completed(prior) ? null : 'unfinished work';
}

function newChain({ lead, item, json, rowsFn }, fail) {
  const queryIndex = queryIndexOf(lead, json);
  const context = new Map();
  const first = mappedRows(queryIndex === null ? rowsOf(json) : [rowsOf(json)[lead.rowIndex]], json, rowsFn, context);
  if (new Set(first).size !== first.length) fail('page.exactlyOnce', `${lead.key} first page lists duplicate rows`, item.label);
  return { key: lead.key, queryIndex, tool: lead.tool, from: item.label, pages: 1, rows: [...first], seenRows: new Set(first), rowsFn, context, ended: false };
}
function recordPage(item, json, cls, fail) {
  const chain = item.chain;
  chain.pages += 1;
  const selected = rowsOf(json).filter(row => chain.queryIndex === null || row.index === chain.queryIndex);
  const ids = mappedRows(selected, json, chain.rowsFn, chain.context);
  const repeated = ids.filter(id => {
    const seen = chain.seenRows.has(id);
    chain.seenRows.add(id);
    return seen;
  });
  if (repeated.length) fail('page.exactlyOnce', `${chain.key} page ${chain.pages} repeats ${repeated.slice(0, 3).join(', ')}`, item.label);
  chain.rows.push(...ids);
  if (!capturedRows(item, json) && !['ok', 'empty', 'outOfRange'].includes(cls)) {
    chain.problem = `page ${chain.pages} class ${cls}`;
    fail('page.class', `${chain.key} ${chain.problem}`, item.label);
  }
}

/** Compare routes observed on both surfaces, with opaque cursor bytes removed. */
function graphParity(cli, mcp) {
  const sources = report => new Set(report.calls.map(call => `${call.label}|${call.tool}|${canon(graphInput(call.input))}`));
  const aSources = sources(cli), bSources = sources(mcp);
  const both = new Set([...aSources].filter(source => bSources.has(source)));
  const pick = report => new Set([...report.graph].filter(edge => both.has(edge.split(' --')[0])));
  const a = pick(cli), b = pick(mcp);
  const onlyCli = [...a].filter(edge => !b.has(edge)), onlyMcp = [...b].filter(edge => !a.has(edge));
  return onlyCli.length || onlyMcp.length ? `call graph differs: CLI-only ${onlyCli.length} (${short(onlyCli[0] ?? '-')}), MCP-only ${onlyMcp.length} (${short(onlyMcp[0] ?? '-')})` : null;
}
