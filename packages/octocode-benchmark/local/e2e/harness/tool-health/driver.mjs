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

export function createDriver({ validate, validateInput = () => null, tools, cliOnly, schemaOf }) {
  async function explore(tool, input, { surface, call, maxCalls = 8, maxPages = 12, rows = () => undefined, allow = [] }) {
    const report = { tool, surface, input, calls: [], failures: [], skipped: [], graph: new Set(), chains: [], budgetCut: 0, converged: 0, metadata: [] };
    const fail = (check, detail, at) => report.failures.push({ check, detail: short(detail, 400), at });
    const identity = (name, request) => `${name} ${canon(request)}`;
    const root = { tool, input, label: 'seed', depth: 0, ancestors: new Set(), children: [], executed: false };
    const seen = new Map([[identity(tool, input), root]]);
    const required = [], optional = [root];
    let optionalCalls = 0;
    while (required.length || optional.length) {
      const item = required.length ? required.shift() : optional.shift();
      if (item.kind !== 'continue' && optionalCalls >= maxCalls) { report.budgetCut += 1; continue; }
      if (item.kind !== 'continue') optionalCalls += 1;
      const entry = await call(item.tool, item.input);
      if (item === root) report.root = entry;
      const json = entry.json;
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
      const here = new Set();
      for (const lead of calls) {
        const id = identity(lead.tool, lead.input);
        const label = lead.kind === 'related'
          ? `${item.label} > related.${lead.tool}:${canon(lead.input)}`
          : `${item.label} > ${lead.container}.${lead.key}`;
        const source = `${item.label}|${item.tool}|${canon(graphInput(item.input))}`;
        if (!cliOnly.has(lead.tool)) report.graph.add(`${source} --${lead.kind}:${lead.kind === 'related' ? 'related' : lead.path}--> ${lead.tool} ${canon(graphInput(lead.input))}`);
        if (here.has(id)) { fail('lead.repeat', `${lead.path} repeats an input in this response`, item.label); continue; }
        here.add(id);
        if (seen.has(id)) {
          const prior = seen.get(id);
          const reason = repeatReason(lead, prior, id, item);
          if (reason) fail('lead.repeat', `${lead.path} repeats ${reason} at ${prior.label}`, item.label);
          else report.converged += 1;
          continue;
        }
        const child = { ...lead, label, depth: lead.kind === 'continue' ? item.depth + 1 : 0, ancestors: new Set([...item.ancestors, identity(item.tool, item.input)]), children: [], executed: false };
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
          const rowsFn = rows(lead.tool, lead.key);
          if (lead.tool === item.tool && rowsFn) {
            child.chain = item.chain?.key === lead.key ? item.chain : newChain({ lead, item, json, rowsFn }, fail);
            if (child.chain !== item.chain) report.chains.push(child.chain);
          }
          item.children.push(child);
          required.push(child);
        } else optional.push(child);
      }
      item.executed = true;
      if (item.chain && !calls.some(lead => lead.kind === 'continue' && lead.tool === item.tool && lead.key === item.chain.key)) item.chain.ended = true;
    }
    for (const chain of report.chains) if (!chain.ended && !chain.capped && !chain.problem) fail('page.ended', `${chain.key} stopped before its end`, chain.from);
    return report;
  }
  return { explore, graphParity };
}

const completed = node => node.executed && node.children.every(completed);
function leadFailure(item, cls, json) {
  if (!item.kind || cls === 'broken') return null;
  if (cls === 'invalid') return { check: 'lead.accepted', detail: `${item.tool} rejected issued ${item.kind} input: ${errorCodes(json).join(',')}` };
  return item.kind === 'continue' && !['ok', 'empty', 'outOfRange'].includes(cls)
    ? { check: 'lead.execution', detail: `${item.tool} failed required continuation: ${errorCodes(json).join(',') || cls}` } : null;
}
function repeatReason(lead, prior, identity, item) {
  if (lead.kind !== 'continue') return null;
  if (prior === item || item.ancestors.has(identity)) return 'an ancestor';
  return completed(prior) ? null : 'unfinished work';
}

function newChain({ lead, item, json, rowsFn }, fail) {
  const first = rowsFn(dataOf(rowsOf(json)[lead.rowIndex ?? 0]), json);
  if (new Set(first).size !== first.length) fail('page.exactlyOnce', `${lead.key} first page lists duplicate rows`, item.label);
  return { key: lead.key, tool: lead.tool, from: item.label, pages: 1, rows: [...first], seenRows: new Set(first), rowsFn, ended: false };
}
function recordPage(item, json, cls, fail) {
  const chain = item.chain;
  chain.pages += 1;
  const ids = chain.rowsFn(dataOf(rowsOf(json)[item.rowIndex ?? 0]), json);
  const repeated = ids.filter(id => chain.seenRows.has(id));
  if (repeated.length) fail('page.exactlyOnce', `${chain.key} page ${chain.pages} repeats ${repeated.slice(0, 3).join(', ')}`, item.label);
  ids.forEach(id => chain.seenRows.add(id));
  chain.rows.push(...ids);
  if (!['ok', 'empty', 'outOfRange'].includes(cls)) {
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
