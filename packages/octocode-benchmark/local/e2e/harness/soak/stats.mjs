// Pure soak verdicts over recorded calls and RSS samples (self-checked by `soak.mjs --self-check`).

/** Nearest-rank percentile `p` of `values` (null when empty). */
export function pct(values, p) {
  if (!values.length) return null;
  const sorted = values.toSorted((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))];
}

/** Plain code-unit order (what a comparator-less `sort()` gives strings). */
export const byCodeUnit = (a, b) => {
  if (a === b) return 0;
  return a < b ? -1 : 1;
};

/** Per server window: first RSS sample at least `warmupMs` after start vs the last sample. */
export function rssVerdicts(windows, { warmupMs, limit = 0.25 }) {
  return windows.map(w => {
    const samples = w.rss.filter(s => Number.isFinite(s.rssKb));
    const base = samples.find(s => s.t - w.startedAt >= warmupMs);
    const last = samples.at(-1);
    if (!base || !last || last === base) return { window: w.window, pid: w.pid, ok: null, note: 'too short for a warm baseline' };
    const growth = last.rssKb / base.rssKb - 1;
    return { window: w.window, pid: w.pid, baseKb: base.rssKb, lastKb: last.rssKb, growth: +growth.toFixed(3), ok: growth < limit };
  });
}

/** Per tool: p95 of successful calls in the first vs the last `spanMs` of the run (disjoint spans only). */
export function driftVerdicts(calls, { spanMs, runMs, floorMs, limit = 2, minCalls = 3 }) {
  const tools = [...new Set(calls.map(c => c.tool))];
  return tools.map(tool => {
    const mine = calls.filter(c => c.tool === tool && !c.isError && !c.breach && !c.injected);
    if (runMs < 2 * spanMs) return { tool, ok: null, note: 'run shorter than two spans' };
    const first = mine.filter(c => c.t < spanMs).map(c => c.ms);
    const last = mine.filter(c => c.t >= runMs - spanMs).map(c => c.ms);
    if (first.length < minCalls || last.length < minCalls) return { tool, ok: null, note: `fewer than ${minCalls} calls per span` };
    const firstP95 = pct(first, 95), lastP95 = pct(last, 95);
    const ratio = +(lastP95 / Math.max(1, firstP95)).toFixed(2);
    return { tool, firstP95, lastP95, ratio, ok: lastP95 < floorMs || ratio < limit };
  });
}

/** Failure reasons for the tools whose p95 drifted. */
export const driftReasons = drift => drift.filter(d => d.ok === false).map(d => `p95 drift ${d.ratio}x for ${d.tool} (${d.firstP95}→${d.lastP95} ms)`);
/** The judged drift ratios as one detail line ('' when none was judged). */
export const driftDetail = drift => drift.filter(d => d.ok !== null).map(d => `${d.tool} ${d.ratio}x`).join(' ');

export function toolStats(calls) {
  const tools = [...new Set(calls.map(c => c.tool))].toSorted(byCodeUnit);
  return tools.map(tool => {
    const mine = calls.filter(c => c.tool === tool);
    const ms = mine.map(c => c.ms);
    return {
      tool, calls: mine.length, errors: mine.filter(c => c.isError).length, breaches: mine.filter(c => c.breach).length,
      p50: pct(ms, 50), p95: pct(ms, 95), max: Math.max(...ms), avgBytes: Math.round(mine.reduce((a, c) => a + (c.bytes ?? 0), 0) / mine.length),
    };
  });
}
