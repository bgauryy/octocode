// What every response in a run emitted: lead channels, error codes, partial reasons.
// Each distinct related call is queued once per surface and replayed by the worker
// whose world issued it; required continuations are walked by the row that got them.
import { canon, leadsIn } from '../tool-health/calls.mjs';

const walkKeys = (node, visit) => {
  if (!node || typeof node !== 'object') return;
  if (Array.isArray(node)) { for (const child of node) walkKeys(child, visit); return; }
  for (const [key, child] of Object.entries(node)) {
    visit(key, child);
    if (key !== 'content') walkKeys(child, visit);
  }
};

const note = (map, key, surface) => { (map.get(key) ?? map.set(key, new Set()).get(key)).add(surface); };
const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

export function createLedger(contract) {
  const channels = new Map();
  const codes = new Map();
  const reasons = new Map();
  const replayed = new Set();
  const schema = new Map(contract.tools.map(tool => [tool.name, tool.outputSchema]));
  return {
    observe(worker, { surface, tool, result }) {
      const json = result.json;
      if (!json) return;
      walkKeys(json, (key, value) => {
        if (key === 'errorCode' && typeof value === 'string' && Object.hasOwn(contract.errorCodes, value)) note(codes, value, surface);
        if (key === 'partialReasons' && Array.isArray(value)) for (const reason of value) if (Object.hasOwn(contract.partialReasons, reason)) note(reasons, reason, surface);
      });
      for (const lead of leadsIn(json, schema.get(tool))) {
        // A named lead outside related[] (calls.repair, next.retry) counts under its own channel, whatever its kind.
        const channel = lead.container === 'related' ? 'related' : lead.key === 'responsePagination.next' ? 'responsePagination.next' : lead.key;
        note(channels, `${tool}.${channel}`, surface);
        if (lead.kind !== 'related') continue;
        const identity = `${surface} ${lead.tool} ${worker.normalize(canon(lead.input))}`;
        if (replayed.has(identity)) continue;
        replayed.add(identity);
        worker.pending.push({ surface, tool: lead.tool, input: lead.input, from: tool });
      }
    },
    report(surfaces) {
      const list = map => Object.fromEntries(surfaces.map(surface => [surface, [...map].filter(([, seen]) => seen.has(surface)).map(([key]) => key).toSorted(byText)]));
      return { channels: list(channels), codes: list(codes), reasons: list(reasons), relatedReplayed: replayed.size };
    },
  };
}
