import { nextHints } from '../responses.mjs';

export const callKey = call => `${call.tool}:${JSON.stringify(call.input)}`;
export const continuations = value => nextHints(value).filter(call => call.kind === 'continue');

export function callIssue(entry) {
  if (entry.transport) return entry.transport;
  if (entry.isError) return entry.text || 'tool call failed';
  const errors = (entry.raw?.results ?? entry.sc?.results ?? []).filter(row => row.status === 'error');
  if (entry.rowErrors || errors.length) return `tool returned ${entry.rowErrors || errors.length} error rows`;
  return null;
}

function reaches(edges, start, target) {
  const pending = [start], seen = new Set();
  while (pending.length) {
    const key = pending.pop();
    if (key === target) return true;
    if (seen.has(key)) continue;
    seen.add(key);
    pending.push(...(edges.get(key) ?? []));
  }
  return false;
}

/** Execute issued inputs unchanged; response pages take priority over row pages. */
export async function walkContinuations(entries, from, invoke, max = 40) {
  const pending = [];
  const seen = new Set();
  const edges = new Map();
  const enqueue = (entry, source = callKey({ tool: entry.tool, input: entry.args })) => {
    for (const call of continuations(entry.raw ?? entry.sc)) {
      const key = callKey(call);
      if (reaches(edges, key, source)) throw new Error(`continuation cycle: ${call.tool} ${call.path}`);
      if (!edges.has(source)) edges.set(source, new Set());
      edges.get(source).add(key);
      if (seen.has(key)) continue;
      seen.add(key);
      pending.push(call);
    }
  };
  for (const entry of entries.slice(from)) enqueue(entry);
  let calls = 0;
  while (pending.length && calls < max) {
    const response = pending.findIndex(call => call.path === '.responsePagination.next');
    const call = pending.splice(response < 0 ? 0 : response, 1)[0];
    const entry = await invoke(call.tool, call.input);
    entries.push(entry);
    calls++;
    const error = callIssue(entry);
    if (error) return { exhausted: false, remaining: pending.length, error };
    enqueue(entry, callKey(call));
  }
  return { exhausted: pending.length === 0, remaining: pending.length };
}

/** Required continuations only; related calls are optional fresh work. */
export function offeredContinuations(entries) {
  const executed = new Set(entries.map(entry => callKey({ tool: entry.tool, input: entry.args })));
  const seen = new Set();
  return entries.flatMap(entry => continuations(entry.raw ?? entry.sc)).flatMap(call => {
    const key = callKey(call);
    if (seen.has(key)) return [];
    seen.add(key);
    return [{ name: call.path, call, byRecipe: executed.has(key) }];
  });
}
