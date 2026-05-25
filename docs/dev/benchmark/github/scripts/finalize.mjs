#!/usr/bin/env node
// finalize.mjs — Aggregate q{N}/metrics.json into <run>/output.md + summary.json.
//
// Reads the canonical machine-readable metrics.json sidecars (not regex-parsed
// markdown) so the rollup matches per-question numbers byte for byte.
//
// Two timing axes are surfaced:
//   tool_elapsed_ms — Σ of individual tool-call wall times (what the agent
//                     waited on tools). Deterministic only as a sum of log rows.
//   q_elapsed_ms    — wall clock from set-q.sh to record.sh per Q. Captures
//                     total time the agent spent on the Q (incl. reasoning
//                     between calls). NOT comparable across hardware.
//
// MCP init cost (q=0, new):
//   mcp-meas.mjs logs `initialize` and `tools/list` responses at q=0 with
//   cmd="_initialize" / "_tools/list". These represent the one-time per-session
//   cost of loading tool schemas + server instructions into the agent's context.
//   gh has no equivalent. finalize.mjs reads them from log.jsonl and surfaces
//   them in output.md + summary.json so the judge agent can include them in reports.
//
// Usage: node finalize.mjs <run_dir>
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const run = process.argv[2];
if (!run || !existsSync(run)) {
  console.error('Usage: finalize.mjs <run_dir>');
  process.exit(1);
}

// Question count: from .q-count (written by init-run.sh) or derived from q*/ dirs.
let N_QS;
const qCountFile = join(run, '.q-count');
if (existsSync(qCountFile)) {
  N_QS = parseInt(readFileSync(qCountFile, 'utf8').trim(), 10);
}
if (!Number.isFinite(N_QS) || N_QS < 1) {
  const qDirs = readdirSync(run).filter(d => /^q\d+$/.test(d)).map(d => +d.slice(1));
  N_QS = qDirs.length ? Math.max(...qDirs) : 0;
}

// MCP init context: read raw log.jsonl, sum rows whose cmd starts with `_`
// (only mcp-meas.mjs emits these: _initialize, _tools/list). Represents the
// one-time per-session cost of loading server instructions + tool schemas
// into the agent's context — attributed to octocode; gh has no equivalent.
const logPath = join(run, 'log.jsonl');
let mcpInit = { calls: 0, in_chars: 0, out_chars: 0, elapsed_ms: 0, rows: [] };
let preQOrphans = 0;        // rows with q=0 that are NOT init (operator error)
if (existsSync(logPath)) {
  const lines = readFileSync(logPath, 'utf8').split('\n').filter(Boolean);
  for (const l of lines) {
    let r; try { r = JSON.parse(l); } catch { continue; }
    if (typeof r.cmd === 'string' && r.cmd.startsWith('_')) {
      mcpInit.calls++;
      mcpInit.in_chars += r.in_chars || 0;
      mcpInit.out_chars += r.out_chars || 0;
      mcpInit.elapsed_ms += r.elapsed_ms || 0;
      mcpInit.rows.push({ cmd: r.cmd, in_chars: r.in_chars, out_chars: r.out_chars });
    } else if (r.q === 0) {
      preQOrphans++;
    }
  }
}

const fmt = n => Number(n).toLocaleString('en');
const oneLine = (p) => {
  if (!existsSync(p)) return '';
  const t = readFileSync(p, 'utf8');
  return (t.split(/## Answer\s*\n+/)[1] || '').split('\n').find(l => l.trim()) || '';
};
const trunc = (s, n = 60) => s.length > n ? s.slice(0, n - 1) + '\u2026' : s;

// Flat layout: q1.json + q1.md live directly in $RUN (no per-Q subdirs).
const rows = readdirSync(run)
  .filter(f => /^q\d+\.json$/.test(f))
  .map(f => +f.replace(/\D/g, ''))
  .sort((a, b) => a - b)
  .map(q => {
    const metricsPath = join(run, `q${q}.json`);
    const outPath    = join(run, `q${q}.md`);
    if (!existsSync(metricsPath) || !existsSync(outPath)) return { q, missing: true };
    const m = JSON.parse(readFileSync(metricsPath, 'utf8'));
    const toolMs = m.tool_elapsed_ms ?? m.elapsed_ms ?? 0;
    const qMs = m.q_elapsed_ms ?? 0;
    return {
      q,
      calls: m.calls,
      in: m.in_chars,
      out: m.out_chars,
      tool_ms: toolMs,
      q_ms: qMs,
      reason_ms: Math.max(0, qMs - toolMs),
      one: oneLine(outPath),
    };
  });

const ok = rows.filter(r => !r.missing);
const sum = (k) => ok.reduce((s, r) => s + (r[k] || 0), 0);
const tot = {
  calls: sum('calls'),
  in: sum('in'),
  out: sum('out'),
  tool_ms: sum('tool_ms'),
  q_ms: sum('q_ms'),
  reason_ms: sum('reason_ms'),
};

const slug = basename(run);
// Run dir is <session>/<agent> — basename is just the agent slug (e.g. "octocode", "gh").
const agent = slug || basename(run);

const initSection = mcpInit.calls > 0
  ? `\n## MCP init context (one-time per-session cost)\n\n` +
    `| Calls | In chars | Out chars (schemas + instructions loaded into agent context) | ms |\n` +
    `|------:|---------:|-------------------------------------------------------------:|---:|\n` +
    `| ${mcpInit.calls} | ${fmt(mcpInit.in_chars)} | ${fmt(mcpInit.out_chars)} | ${fmt(mcpInit.elapsed_ms)} |\n\n` +
    `Breakdown: ${mcpInit.rows.map(r => `\`${r.cmd}\`=${fmt(r.out_chars)} chars`).join(', ')}.\n\n` +
    `> This cost is attributed to octocode (loaded once at session start). gh has no equivalent context-loading step — surfacing it is what makes the comparison honest.\n`
  : '';

const orphanWarn = preQOrphans > 0
  ? `\n> ⚠️  ${preQOrphans} log row(s) tagged q=0 without an MCP init marker — likely a tool call before \`set-q.sh 1\`. Excluded from both per-Q and init totals.\n`
  : '';

const body = `# Run ${slug}

| Agent | Questions | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|--------:|----------:|-------------:|
| ${agent} | ${ok.length} / ${N_QS} | ${tot.calls} | ${fmt(tot.in)} | ${fmt(tot.out)} | ${fmt(tot.tool_ms)} | ${fmt(tot.q_ms)} | ${fmt(tot.reason_ms)} |

> **Tool ms** = Σ wall time on tool calls. **Q wall ms** = Σ wall time per question from \`set-q.sh\` to \`record.sh\`. **Reasoning ms** = Q wall − Tool (approx time the LLM spent thinking between calls).
${initSection}${orphanWarn}

| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|--------:|----------:|-------------:|-------------------|
${rows.map(r => r.missing
  ? `| Q${r.q} | — | — | — | — | — | — | ⚠️ missing |`
  : `| Q${r.q} | ${r.calls} | ${fmt(r.in)} | ${fmt(r.out)} | ${fmt(r.tool_ms)} | ${fmt(r.q_ms)} | ${fmt(r.reason_ms)} | ${trunc(r.one)} |`
).join('\n')}
| **Σ** | **${tot.calls}** | **${fmt(tot.in)}** | **${fmt(tot.out)}** | **${fmt(tot.tool_ms)}** | **${fmt(tot.q_ms)}** | **${fmt(tot.reason_ms)}** | |
`;

writeFileSync(join(run, 'output.md'), body);

// summary.json — used by cross-run.mjs, report-variance.mjs, validate-pipeline.mjs,
// and the judge agent (reads mcp_init for the two-run comparison report).
const summary = {
  run: slug,
  agent,
  questions: ok.length,
  totals: {
    calls: tot.calls,
    in_chars: tot.in,
    out_chars: tot.out,
    tool_elapsed_ms: tot.tool_ms,
    q_elapsed_ms: tot.q_ms,
    reasoning_ms: tot.reason_ms,
  },
  // mcp_init: one-time session cost (null for gh — no schema loading).
  // the judge agent reads this to include init context in the comparison report.
  mcp_init: mcpInit.calls > 0
    ? {
        calls: mcpInit.calls,
        in_chars: mcpInit.in_chars,
        out_chars: mcpInit.out_chars,
        elapsed_ms: mcpInit.elapsed_ms,
        rows: mcpInit.rows,
      }
    : null,
  pre_q_orphans: preQOrphans,
  per_q: rows.map(r => r.missing
    ? { q: r.q, missing: true }
    : {
        q: r.q,
        calls: r.calls,
        in_chars: r.in,
        out_chars: r.out,
        tool_elapsed_ms: r.tool_ms,
        q_elapsed_ms: r.q_ms,
        reasoning_ms: r.reason_ms,
      }),
};
writeFileSync(join(run, 'summary.json'), JSON.stringify(summary, null, 2));

console.log(`wrote ${join(run, 'output.md')}`);
console.log(`wrote ${join(run, 'summary.json')}`);
console.log(`questions=${ok.length}/${N_QS}  calls=${tot.calls}  in=${fmt(tot.in)}  out=${fmt(tot.out)}  tool_ms=${fmt(tot.tool_ms)}  q_ms=${fmt(tot.q_ms)}  reason_ms=${fmt(tot.reason_ms)}`);
if (mcpInit.calls > 0) {
  console.log(`mcp_init: calls=${mcpInit.calls}  in=${fmt(mcpInit.in_chars)}  out=${fmt(mcpInit.out_chars)} chars  ms=${fmt(mcpInit.elapsed_ms)}  (one-time session cost)`);
}
if (preQOrphans > 0) {
  console.warn(`⚠️  ${preQOrphans} non-init log row(s) tagged q=0 — likely a tool call before set-q.sh 1`);
}
const missing = rows.filter(r => r.missing).map(r => `Q${r.q}`);
if (missing.length) {
  console.warn(`missing: ${missing.join(', ')}`);
  process.exit(1);
}
