// `octocode-graph events <result-dir|project-dir>`: a saved run's events.jsonl summarized. Every octocode call is
// `tool` events under one id: a start, then a done or an error (pipeline.ts reportTools). A build ends with its `done`
// event and each later refresh appends events ending with `refreshed`, so the log splits into runs at those two.
import type { LoggedRunEvent } from './pipeline.ts';
import { plural } from './query-ops.ts';

/** What the calls of one tool, or of all, did: each id counts once. */
export interface ToolTotals {
  /** Distinct tool ids. */
  calls: number;
  /** Calls that continued a cursor (`page: cursor …`), not a fresh query. */
  cursorPages: number;
  /** Calls that failed: an `error` terminal event. */
  errors: number;
  /** Calls answered with a failed row (its errorCode, such as scopeTooBroad). */
  errorRows: number;
  /** Calls with a start and no terminal event. */
  unfinished: number;
  rows: number;
  bytes: number;
  /** Summed call time of the terminal events (calls overlap, so it can exceed wall time). */
  ms: number;
}

/** One build, refresh, or the events after the last of them ended (`after`: viewer requests, an unfinished refresh). */
export interface RunSummary {
  kind: 'build' | 'refresh' | 'after';
  /** The first and last sequence numbers it holds. */
  seq: [number, number];
  /** False for a build or refresh that no `done` or `refreshed` event ends. */
  finished: boolean;
  /** From its first event to its last. */
  wallMs: number;
  tools: ToolTotals;
  /** Each phase in start order: `ms` from its start to its done or skip, null while it never ended. */
  phases: { id: string; state: string; ms: number | null }[];
  /** `error` events and chunks that ended in error, with their messages. */
  failures: string[];
}

export interface EventsSummary {
  events: number;
  tools: ToolTotals & { byTool: Record<string, ToolTotals> };
  /** Each id should start once: ids, start events, ids started more than once, ids with no start. */
  starts: {
    ids: number;
    startEvents: number;
    repeated: { id: string; starts: number }[];
    withoutStart: string[];
  };
  runs: RunSummary[];
}

const zero = (): ToolTotals => ({
  calls: 0,
  cursorPages: 0,
  errors: 0,
  errorRows: 0,
  unfinished: 0,
  rows: 0,
  bytes: 0,
  ms: 0,
});

const num = (value: unknown) =>
  typeof value === 'number' && Number.isFinite(value) ? value : 0;
// An event field that should hold text: the text, else what to show instead.
const text = (value: unknown, otherwise: string) =>
  typeof value === 'string' && value ? value : otherwise;

/** One call as its events state it: its tool, its start count and its terminal event. */
interface Call {
  tool: string;
  starts: number;
  end?: LoggedRunEvent;
}

function callsOf(events: readonly LoggedRunEvent[]) {
  const calls = new Map<string, Call>();
  for (const e of events) {
    if (e.type !== 'tool' || typeof e.id !== 'string') continue;
    const call = calls.get(e.id) ?? { tool: text(e.tool, '?'), starts: 0 };
    if (e.state === 'start') call.starts++;
    else call.end = e;
    calls.set(e.id, call);
  }
  return calls;
}

function add(totals: ToolTotals, { end }: Call) {
  totals.calls++;
  if (!end) {
    totals.unfinished++;
    return;
  }
  if (typeof end.page === 'string' && end.page.startsWith('cursor'))
    totals.cursorPages++;
  if (end.state === 'error') totals.errors++;
  else if (end.error) totals.errorRows++;
  totals.rows += num(end.rows);
  totals.bytes += num(end.bytes);
  totals.ms += num(end.ms);
}

function runOf(
  events: readonly LoggedRunEvent[],
  kind: RunSummary['kind'],
  finished: boolean,
  calls: Map<string, Call>
): RunSummary {
  const tools = zero();
  const ids = new Set<string>();
  const phases: RunSummary['phases'] = [];
  const started = new Map<
    string,
    { t: number; row: RunSummary['phases'][number] }
  >();
  const failures: string[] = [];
  for (const e of events) {
    if (e.type === 'tool' && typeof e.id === 'string' && !ids.has(e.id)) {
      ids.add(e.id);
      add(tools, calls.get(e.id)!);
    }
    if (e.type === 'phase' && typeof e.id === 'string') {
      const open = started.get(e.id);
      if (e.state === 'start') {
        const row = { id: e.id, state: 'started', ms: null };
        phases.push(row);
        started.set(e.id, { t: num(e.t), row });
      } else if (open) {
        open.row.state = text(e.state, '?');
        open.row.ms = num(e.t) - open.t;
        started.delete(e.id);
      } else phases.push({ id: e.id, state: text(e.state, '?'), ms: 0 });
    }
    if (e.type === 'error')
      failures.push(text(e.message, text(e.error, 'error')));
    if (e.type === 'chunk' && e.state === 'error')
      failures.push(`chunk ${text(e.id, '?')}: ${text(e.error, 'error')}`);
  }
  const first = events[0],
    last = events.at(-1)!;
  return {
    kind,
    seq: [first.seq, last.seq],
    finished,
    wallMs: num(last.t) - num(first.t),
    tools,
    phases,
    failures,
  };
}

/** The summary of a run's events, in log order (events.jsonl as readEvents returns it). */
export function summarizeEvents(
  events: readonly LoggedRunEvent[]
): EventsSummary {
  const calls = callsOf(events);
  const total = zero();
  const byTool: Record<string, ToolTotals> = {};
  for (const call of calls.values()) {
    add(total, call);
    add((byTool[call.tool] ??= zero()), call);
  }
  const runs: RunSummary[] = [];
  let open: LoggedRunEvent[] = [];
  for (const e of events) {
    open.push(e);
    if (e.type !== 'done' && e.type !== 'refreshed') continue;
    runs.push(
      runOf(open, e.type === 'done' ? 'build' : 'refresh', true, calls)
    );
    open = [];
  }
  if (open.length) {
    const kind = !runs.length
      ? 'build'
      : open.some(e => e.type === 'phase')
        ? 'refresh'
        : 'after';
    runs.push(runOf(open, kind, kind === 'after', calls));
  }
  const repeated = [...calls]
    .filter(([, c]) => c.starts > 1)
    .map(([id, c]) => ({ id, starts: c.starts }));
  return {
    events: events.length,
    tools: { ...total, byTool },
    starts: {
      ids: calls.size,
      startEvents: [...calls.values()].reduce((n, c) => n + c.starts, 0),
      repeated,
      withoutStart: [...calls].filter(([, c]) => !c.starts).map(([id]) => id),
    },
    runs,
  };
}

const seconds = (ms: number) =>
  ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`;

/** The summary as text: one line per run with its phases, then a table of calls per tool. */
export function formatEvents(s: EventsSummary, dir: string) {
  const lines = [
    `${dir}/events.jsonl: ${plural(s.events, 'event')}, ${plural(s.runs.length, 'run')}`,
  ];
  for (const r of s.runs) {
    const t = r.tools;
    lines.push(
      `${r.kind}${r.finished ? '' : ' (unfinished)'} seq ${r.seq[0]}–${r.seq[1]}: wall ${seconds(r.wallMs)}, ${plural(t.calls, 'tool call')} (${plural(t.cursorPages, 'cursor page')}, ${t.errors} failed, ${plural(t.errorRows, 'error row')}, ${t.unfinished} unfinished), summed call time ${seconds(t.ms)}`
    );
    if (r.phases.length)
      lines.push(
        `  phases: ${r.phases.map(p => `${p.id} ${p.ms === null ? 'not ended' : `${seconds(p.ms)}${p.state === 'done' ? '' : ` (${p.state})`}`}`).join(' · ')}`
      );
    for (const f of r.failures) lines.push(`  failure: ${f}`);
  }
  const header = [
    'tool',
    'calls',
    'cursor',
    'failed',
    'errorRows',
    'unfinished',
    'rows',
    'bytes',
    'ms',
  ];
  const row = (name: string, t: ToolTotals) => [
    name,
    ...[
      t.calls,
      t.cursorPages,
      t.errors,
      t.errorRows,
      t.unfinished,
      t.rows,
      t.bytes,
      Math.round(t.ms),
    ].map(String),
  ];
  const table = [
    header,
    ...Object.entries(s.tools.byTool)
      .toSorted(([a, x], [b, y]) => y.calls - x.calls || (a < b ? -1 : 1))
      .map(([name, t]) => row(name, t)),
    row('total', s.tools),
  ];
  const widths = header.map((_, i) => Math.max(...table.map(r => r[i].length)));
  for (const r of table)
    lines.push(
      r
        .map((cell, i) =>
          i ? cell.padStart(widths[i]) : cell.padEnd(widths[i])
        )
        .join('  ')
    );
  const st = s.starts;
  lines.push(
    `starts: ${plural(st.ids, 'id')}, ${plural(st.startEvents, 'start event')}${st.repeated.length ? `; started more than once: ${st.repeated.map(x => `${x.id} (${x.starts})`).join(', ')}` : ''}${st.withoutStart.length ? `; no start event: ${st.withoutStart.join(', ')}` : ''}`
  );
  return lines.join('\n');
}
