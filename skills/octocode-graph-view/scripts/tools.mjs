// Octocode tool calls followed to their end: responsePagination.next first, then every `next.*` continuation
// of kind `continue` (one chain per result row). Each page reaches onPage as it arrives, so callers stream.
import { octocode } from './lib.mjs';

// Continuations a response hands back: the serialized-response page first (TOOL_DATA_CONTRACT), then each row's.
const rowContinuations = (row, tool) => Object.values(row?.data?.next ?? {}).filter((c) => c?.kind === 'continue' && c.input).map((c) => ({ tool: c.tool ?? tool, input: c.input }));
export function continuations(page, tool) {
  const rp = page?.responsePagination?.next;
  if (rp?.input) return [{ tool: rp.tool ?? tool, input: rp.input }];
  return (page?.results ?? []).flatMap((row) => rowContinuations(row, tool));
}

// Runs `input` and every continuation it emits; returns the pages in order. A repeated cursor is a defect.
export async function pagedTool(tool, input, { onPage = () => {}, run = (t, i) => octocode([t, '--json', JSON.stringify(i)], { partial: true }) } = {}) {
  const pages = [];
  const seen = new Set();
  const queue = [{ tool, input }];
  while (queue.length) {
    const call = queue.shift();
    const key = JSON.stringify(call);
    if (seen.has(key)) throw new Error(`Repeated ${call.tool} continuation; evidence is incomplete`);
    seen.add(key);
    const page = await run(call.tool, call.input);
    pages.push(page);
    onPage(page, pages.length, call);
    queue.unshift(...continuations(page, call.tool));
  }
  return pages;
}

// Rows of every page for one result field (e.g. `symbols`, `files`), in page order.
export const rowsOf = (pages, field) => pages.flatMap((p) => (p.results ?? []).flatMap((r) => r?.data?.[field] ?? []));

// One line summarizing a tool call for the viewer's Tools tab: operation, target, first page or cursor.
export function describeCall(args) {
  const [tool, ...rest] = args;
  const json = rest.find((a) => a.startsWith('{'));
  let input = {};
  try { input = json ? JSON.parse(json) : {}; } catch { /* the call is still logged with its raw args */ }
  if (input.cursor) return { tool, detail: 'cursor', page: `cursor ${String(input.cursor).slice(-10)}` };
  const q = input.queries ?? [];
  if (tool === 'graphBuild' || tool === 'graphQuery') {
    const x = q[0] ?? {};
    const targets = [x.target].flat().filter(Boolean);
    const target = targets.length > 1 ? `${targets.length} targets` : targets[0];
    const what = tool === 'graphBuild' ? x.path?.split('/').slice(-2).join('/') : target;
    return { tool, detail: [x.operation, what, x.kind, x.edges?.join(',')].filter(Boolean).join(' · '), page: 'first' };
  }
  const ops = [...new Set(q.map((x) => x.operation ?? x.view ?? ''))].filter(Boolean).join(',');
  const what = q.map((x) => [x.symbolName, x.path?.split('/').pop()].filter(Boolean).join('@')).slice(0, 2).join('; ');
  return { tool, detail: [ops, what, q.length > 1 ? `${q.length} queries` : ''].filter(Boolean).join(' · '), page: 'first' };
}

// Grouped rows (structure `{dir, files}`, LSP `{path, matches}`) count their members.
const groupSize = (x) => (Array.isArray(x?.files) ? x.files.length : Array.isArray(x?.matches) ? x.matches.length : 1);
const ROW_FIELDS = ['files', 'symbols', 'matches', 'rows'];
const rowCount = (d) => ROW_FIELDS.reduce((sum, k) => sum + (Array.isArray(d[k]) ? d[k].reduce((a, x) => a + groupSize(x), 0) : 0), 0);

// Rows a response carries (the row fields of tool results, graph `rows` among them) and whether it continues.
export function responseShape(value) {
  if (!value || typeof value !== 'object') return { rows: 0, more: false };
  const datas = (value.results ?? []).map((r) => r?.data ?? {});
  const rows = datas.reduce((sum, d) => sum + rowCount(d), 0);
  const total = datas.length === 1 ? datas[0].pagination?.totalItems : undefined;
  return { rows, ...(Number.isInteger(total) ? { total } : {}), more: continuations(value).length > 0, error: datas.find((d) => d.errorCode)?.errorCode };
}
