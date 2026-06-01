#!/usr/bin/env node
// finalize.mjs — Aggregate q<N>.json into <run>/output.md + summary.json.
//
// Reads the canonical machine-readable q<N>.json sidecars (not regex-parsed
// markdown) so the rollup matches per-question numbers byte for byte.
//
// Two timing axes are surfaced:
//   tool_elapsed_ms — Σ of individual tool-call wall times (what the agent
//                     waited on tools). Deterministic only as a sum of log rows.
//   q_elapsed_ms    — wall clock from set-q.sh to record.sh per Q. Captures
//                     total time the agent spent on the Q (incl. reasoning
//                     between calls). NOT comparable across hardware.
//
// MCP init cost (q=0):
//   mcp-meas.mjs logs `initialize` and `tools/list` responses at q=0 with
//   cmd="_initialize" / "_tools/list". These represent the one-time character
//   cost of loading tool schemas + server instructions into the agent's context.
//   gh has no equivalent. finalize.mjs reads them from log.jsonl and surfaces
//   them in output.md + summary.json so the judge agent can include them.
//
// Usage: node finalize.mjs <run_dir>
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const run = process.argv[2];
if (!run || !existsSync(run)) {
  console.error('Usage: finalize.mjs <run_dir>');
  process.exit(1);
}

const qJsonNumbers = () => readdirSync(run)
  .filter(f => /^q\d+\.json$/.test(f))
  .map(f => +f.replace(/\D/g, ''))
  .filter(Number.isFinite)
  .sort((a, b) => a - b);

let N_QS;
const qCountFile = join(run, '.q-count');
if (existsSync(qCountFile)) {
  N_QS = Number.parseInt(readFileSync(qCountFile, 'utf8').trim(), 10);
}
const discoveredQs = qJsonNumbers();
if (!Number.isFinite(N_QS) || N_QS < 1) {
  N_QS = discoveredQs.length ? Math.max(...discoveredQs) : 0;
}
const questionNumbers = N_QS > 0
  ? Array.from({ length: N_QS }, (_, i) => i + 1)
  : discoveredQs;

const logPath = join(run, 'log.jsonl');
let mcpInit = { calls: 0, in_chars: 0, out_chars: 0, elapsed_ms: 0, rows: [] };
let preQOrphans = 0;
if (existsSync(logPath)) {
  const lines = readFileSync(logPath, 'utf8').split('\n').filter(Boolean);
  for (const l of lines) {
    let r; try { r = JSON.parse(l); } catch { continue; }
    if (typeof r.cmd === 'string' && r.cmd.startsWith('_')) {
      if (!Number.isFinite(r.in_chars) || !Number.isFinite(r.out_chars)) {
        console.error(`finalize: init row lacks character fields in ${logPath} — rerun with character-based metering`);
        process.exit(2);
      }
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
const approxTokens = n => Math.ceil(Number(n) / 4);
const oneLine = (p) => {
  if (!existsSync(p)) return '';
  const t = readFileSync(p, 'utf8');
  return (t.split(/## Answer\s*\n+/)[1] || '').split('\n').find(l => l.trim()) || '';
};
const trunc = (s, n = 60) => s.length > n ? s.slice(0, n - 1) + '\u2026' : s;

const rows = questionNumbers.map(q => {
    const metricsPath = join(run, `q${q}.json`);
    const outPath    = join(run, `q${q}.md`);
    if (!existsSync(metricsPath) || !existsSync(outPath)) return { q, missing: true };
    const m = JSON.parse(readFileSync(metricsPath, 'utf8'));
    if (!Number.isFinite(m.in_chars) || !Number.isFinite(m.out_chars)) {
      console.error(`finalize: ${metricsPath} lacks character fields — rerun with character-based metering`);
      process.exit(2);
    }
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
const perQChars = tot.in + tot.out;
const mcpInitChars = mcpInit.in_chars + mcpInit.out_chars;
const totalCharsWithInit = perQChars + mcpInitChars;

if (preQOrphans > 0) {
  console.error(`finalize: ${preQOrphans} non-init log row(s) tagged q=0 — likely a metered call before set-q.sh. Re-run the affected question so every call is attributed.`);
  process.exit(3);
}

const slug = basename(run);
const agent = slug || basename(run);

const initSection = mcpInit.calls > 0
  ? `\n## MCP init context (one-time per-session cost)\n\n` +
    `| Calls | In chars | Out chars (schemas + instructions loaded into agent context) | ms |\n` +
    `|------:|---------:|-------------------------------------------------------------:|---:|\n` +
    `| ${mcpInit.calls} | ${fmt(mcpInit.in_chars)} | ${fmt(mcpInit.out_chars)} | ${fmt(mcpInit.elapsed_ms)} |\n\n` +
    `Breakdown: ${mcpInit.rows.map(r => `\`${r.cmd}\`=${fmt(r.out_chars)} chars`).join(', ')}.\n\n` +
    `> This cost is attributed to octocode (loaded once at session start). gh has no equivalent context-loading step — surfacing it is what makes the comparison honest.\n`
  : '';

const body = `# Run ${slug}

| Agent | Questions | Calls | In Chars | Out Chars | Per-Q Chars | MCP Init Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms |
|-------|----------:|------:|---------:|----------:|------------:|---------------:|------------:|--------------:|--------:|----------:|-------------:|
| ${agent} | ${ok.length} / ${N_QS} | ${tot.calls} | ${fmt(tot.in)} | ${fmt(tot.out)} | ${fmt(perQChars)} | ${fmt(mcpInitChars)} | ${fmt(totalCharsWithInit)} | ${fmt(approxTokens(totalCharsWithInit))} | ${fmt(tot.tool_ms)} | ${fmt(tot.q_ms)} | ${fmt(tot.reason_ms)} |

> **Total Chars** = per-question \`in_chars + out_chars\` plus MCP init/context chars when present. **Approx Tokens** = \`ceil(Total Chars / 4)\` and is a rough display-only token proxy; characters remain the canonical measurement. **Tool/Q/Reasoning ms** are context only for token-usage judging.
${initSection}

| Q | Calls | In Chars | Out Chars | Total Chars | Approx Tokens | Tool ms | Q wall ms | Reasoning ms | Answer (one line) |
|---|------:|---------:|----------:|------------:|--------------:|--------:|----------:|-------------:|-------------------|
${rows.map(r => r.missing
  ? `| Q${r.q} | — | — | — | — | — | — | — | — | ⚠️ missing |`
  : `| Q${r.q} | ${r.calls} | ${fmt(r.in)} | ${fmt(r.out)} | ${fmt(r.in + r.out)} | ${fmt(approxTokens(r.in + r.out))} | ${fmt(r.tool_ms)} | ${fmt(r.q_ms)} | ${fmt(r.reason_ms)} | ${trunc(r.one)} |`
).join('\n')}
| **Σ** | **${tot.calls}** | **${fmt(tot.in)}** | **${fmt(tot.out)}** | **${fmt(perQChars)}** | **${fmt(approxTokens(perQChars))}** | **${fmt(tot.tool_ms)}** | **${fmt(tot.q_ms)}** | **${fmt(tot.reason_ms)}** | |
`;

writeFileSync(join(run, 'output.md'), body);

const summary = {
  run: slug,
  agent,
  questions: ok.length,
  totals: {
    calls: tot.calls,
    in_chars: tot.in,
    out_chars: tot.out,
    per_q_chars: perQChars,
    total_chars_with_init: totalCharsWithInit,
    approx_tokens_with_init: approxTokens(totalCharsWithInit),
    tool_elapsed_ms: tot.tool_ms,
    q_elapsed_ms: tot.q_ms,
    reasoning_ms: tot.reason_ms,
  },
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
        total_chars: r.in + r.out,
        approx_tokens: approxTokens(r.in + r.out),
        tool_elapsed_ms: r.tool_ms,
        q_elapsed_ms: r.q_ms,
        reasoning_ms: r.reason_ms,
      }),
};
writeFileSync(join(run, 'summary.json'), JSON.stringify(summary, null, 2));

console.log(`wrote ${join(run, 'output.md')}`);
console.log(`wrote ${join(run, 'summary.json')}`);
console.log(`questions=${ok.length}/${N_QS}  calls=${tot.calls}  in_chars=${fmt(tot.in)}  out_chars=${fmt(tot.out)}  total_chars=${fmt(totalCharsWithInit)}  approx_tokens=${fmt(approxTokens(totalCharsWithInit))}  tool_ms=${fmt(tot.tool_ms)}  q_ms=${fmt(tot.q_ms)}  reason_ms=${fmt(tot.reason_ms)}`);
if (mcpInit.calls > 0) {
  console.log(`mcp_init: calls=${mcpInit.calls}  in_chars=${fmt(mcpInit.in_chars)}  out_chars=${fmt(mcpInit.out_chars)} chars  ms=${fmt(mcpInit.elapsed_ms)}  (one-time session cost)`);
}
const missing = rows.filter(r => r.missing).map(r => `Q${r.q}`);
if (missing.length) {
  console.warn(`missing: ${missing.join(', ')}`);
  process.exit(1);
}
