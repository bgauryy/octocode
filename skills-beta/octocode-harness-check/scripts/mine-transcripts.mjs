#!/usr/bin/env node
// Per-tool call/error/size stats and clustered error signatures for Octocode tool
// calls in Claude Code transcripts. Complements orangu, which redacts error text.
// Reads ~/.claude/projects/<slug>*/**/*.jsonl; prints JSON (or a table with --table).
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
if (args.includes('--help')) {
  console.log(`mine-transcripts — Octocode tool-call stats from Claude Code transcripts

  node scripts/mine-transcripts.mjs [--cwd <repo>] [--since YYYY-MM-DD] [--tool-prefix <s>] [--top <n>] [--table]

  --cwd          repository whose sessions to scan (default: current directory)
  --since        only calls at or after this date (ISO prefix compare)
  --tool-prefix  tool-name substring to keep (default: octocode)
  --top          error signatures to print (default: 30)
  --table        human table instead of JSON`);
  process.exit(0);
}

const cwd = opt('cwd', process.cwd());
const since = opt('since', '');
const prefix = opt('tool-prefix', 'octocode');
const top = Number(opt('top', '30'));
const slug = cwd.replace(/[^A-Za-z0-9]/g, '-');
const root = join(homedir(), '.claude', 'projects');

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) yield* walk(p);
    else if (name.endsWith('.jsonl')) yield p;
  }
}

const dirs = readdirSync(root).filter((d) => d === slug || d.startsWith(`${slug}-`));
const uses = new Map();
const rows = [];
for (const d of dirs) {
  for (const file of walk(join(root, d))) {
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      if (!line.includes('tool_')) continue;
      let m;
      try { m = JSON.parse(line); } catch { continue; }
      const content = m?.message?.content;
      if (!Array.isArray(content)) continue;
      for (const b of content) {
        if (b.type === 'tool_use' && String(b.name).includes(prefix)) {
          uses.set(b.id, { tool: String(b.name).split('__').pop(), input: JSON.stringify(b.input ?? {}), ts: m.timestamp ?? '' });
        } else if (b.type === 'tool_result' && uses.has(b.tool_use_id)) {
          const u = uses.get(b.tool_use_id);
          const c = b.content;
          const text = typeof c === 'string' ? c : (c ?? []).map((x) => x?.text ?? '').join('');
          if (!since || u.ts >= since) rows.push({ ...u, output: text, isError: Boolean(b.is_error) });
        }
      }
    }
  }
}

const ERROR_RE = /"status":"error"|status: error|outputContractViolation|invalidInput|Invalid arguments|"error":/;
const SIG_RE = /(Invalid arguments[^\]\n]{0,120}|invalidInput[^,}\n]{0,100}|"message":"[^"]{0,140}|"error":"[^"]{0,140}|error: [^\n]{0,140}|MCP error[^\n]{0,140})/;
const pct = (xs, p) => (xs.length ? xs[Math.min(xs.length - 1, Math.floor(xs.length * p))] : 0);

const byTool = {};
const sigs = new Map();
for (const r of rows) {
  const t = (byTool[r.tool] ??= { calls: 0, errors: 0, inChars: [], outChars: [] });
  t.calls++;
  t.inChars.push(r.input.length);
  t.outChars.push(r.output.length);
  const bad = r.isError || ERROR_RE.test(r.output.slice(0, 4000));
  if (!bad) continue;
  t.errors++;
  const raw = (r.output.match(SIG_RE)?.[1] ?? r.output.slice(0, 160)).replace(/\d+/g, 'N').replace(/\s+/g, ' ');
  const key = `${r.tool} | ${raw.slice(0, 150)}`;
  const s = sigs.get(key) ?? { count: 0, first: r.ts, last: r.ts };
  s.count++;
  if (r.ts < s.first) s.first = r.ts;
  if (r.ts > s.last) s.last = r.ts;
  sigs.set(key, s);
}

const tools = Object.entries(byTool)
  .map(([tool, t]) => {
    t.inChars.sort((a, b) => a - b);
    t.outChars.sort((a, b) => a - b);
    return { tool, calls: t.calls, errors: t.errors, errorRate: +(t.errors / t.calls).toFixed(3),
      inMedian: pct(t.inChars, 0.5), outMedian: pct(t.outChars, 0.5), outP90: pct(t.outChars, 0.9), outMax: t.outChars.at(-1) };
  })
  .sort((a, b) => b.calls - a.calls);
const signatures = [...sigs].sort((a, b) => b[1].count - a[1].count).slice(0, top)
  .map(([k, v]) => ({ signature: k, count: v.count, first: v.first.slice(0, 10), last: v.last.slice(0, 10) }));

if (args.includes('--table')) {
  console.log(`sessions dirs: ${dirs.length}  calls: ${rows.length}${since ? `  since ${since}` : ''}`);
  console.log('tool'.padEnd(20), 'calls', 'err', 'rate ', 'outMed', 'outP90', 'outMax');
  for (const t of tools) console.log(t.tool.padEnd(20), String(t.calls).padStart(5), String(t.errors).padStart(3), t.errorRate.toFixed(2).padStart(5), String(t.outMedian).padStart(6), String(t.outP90).padStart(6), String(t.outMax).padStart(6));
  console.log('\ntop error signatures (count first..last):');
  for (const s of signatures) console.log(String(s.count).padStart(4), `${s.first}..${s.last}`, s.signature);
} else {
  console.log(JSON.stringify({ cwd, since: since || null, sessionDirs: dirs.length, calls: rows.length, tools, signatures }, null, 1));
}
