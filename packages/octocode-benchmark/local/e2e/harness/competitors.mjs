// BENCHMARK, not a correctness test: run-all runs it only when named (LIVE: calls GitHub).
// Deterministic competitor harness (beat-rg-gh PLAN): frozen tasks run as
// an octocode recipe (MCP via the shared client; CLI for CLI-only tools) and
// as the shell recipe an expert agent would run (rg/sed/git/gh/ast-grep/npm).
// Both sides are judged by the same truth check; calls, model-visible bytes
// (structuredContent JSON / stdout+stderr) and wall ms are recorded per side.
// Exit is non-zero when an octocode answer is wrong or a fully walked recipe
// meets a never-trim violation; efficiency losses are reported, not failed.
//
// Fairness (normalized, the default): a local task runs with the MCP workspace
// and the shell cwd at its corpus repo root, so both sides print repo-relative
// paths; research tasks carry a short agent-sized `mainGoal` brief and
// single-call lookups none; a task's `unfiltered` shell
// recipe (a first try without answer knowledge: no jq/--json field picks, no
// pre-known line ranges) is reported next to the expert recipe.
// Sensors per octocode response: nextShare / hintsShare (bytes under `next` /
// `hints` over response bytes) and leadShare (lead continuations wherever they
// live), lead entries per menu (reported without a fixed cap), and
// never-trim (every truncation signal has an executable continuation or a
// terminal-limit disclosure). Shape-agnostic: `next` = pages (it also held
// leads in legacy streams), `hints` = leads + text; see harness/sensors.mjs.
// RFC tool-quality-efficiency sensors (harness/sensors.mjs):
//   schema errors   MCP/CLI input-validation errors per task (target 0)
//   verbose fields  default-output fields from harness/verbose-fields.json (report only)
//   verbatim replay a bounded sample of every task's continuations runs unchanged and must validate
//   byte gate       per task vs a pinned anchor and a rolling baseline in results/
//                   (competitors-anchor.json, competitors-rolling.json): bytes or calls
//                   > 1.5× a baseline while the shell ratio worsens fails; local tasks also
//                   fail when unique evidence shrinks or never-trim violations grow. Each
//                   record keeps a body hash and key byte shares to name the growing key.
//
// Usage: node harness/competitors.mjs [--only=L01,G03] [--kind=local|github|clasify] [--save-as=<results name>]
//        [--update-rolling]  accepted run (every check passed): merge this run into the rolling baseline
//        [--update-anchor]   explicit re-anchor: merge this run into the pinned anchor (and rolling);
//                            allowed when only byte-gate checks failed
//        [--self-test]       run the sensor self-tests (no server)
// Env:   OCTOCODE_COMPETITOR_CLASIFY=1   run the classifier tasks (localFetch clasifyContext; paid provider calls)
//        OCTOCODE_COMPETITOR_GITHUB=0    skip GitHub/network tasks
//        OCTOCODE_COMPETITOR_NORMALIZE=0 legacy run: octocode-root workspace, question as mainGoal on every call
//        OCTOCODE_COMPETITOR_BASH_TOOL_BYTES  Bash tool definition estimate (default 1200)
//        OCTOCODE_COMPETITOR_REPLAY_MAX=3   continuations replayed verbatim per task (0 = off)
//        RG_BIN                          ripgrep binary (else rg on PATH, else Claude Code's embedded rg)
import fs from 'node:fs';
import path from 'node:path';
import { TOOL_POLICIES } from '@octocodeai/config/schema';
import { RESULTS, ROOT } from './paths.mjs';
import { checks, startServer, writeResults } from './mcp-client.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { hash, median } from './util.mjs';
import { ANCHOR_FILE, GATE_FACTOR, ROLLING_FILE, baselineRecord, describeFlag, gateTask, selfTest } from './sensors.mjs';
import { REPLAY_MAX, clientFor, closeClients, octocodeSide, paceCodeSearch, replayHints } from './competitors/octocode.mjs';
import { prepareShell, removeShellScratch, shellSide } from './competitors/shell.mjs';
import { liveTruth } from './competitors/truth.mjs';
import { NORMALIZED, taskWorkspace } from './competitors/workspace.mjs';

if (process.argv.includes('--self-test')) process.exit(selfTest() ? 1 : 0);

const { tasks: ALL } = JSON.parse(fs.readFileSync(new URL('competitor-tasks.json', import.meta.url), 'utf8'));
prepareShell();
const BASH_TOOL_DEF_BYTES = Number(process.env.OCTOCODE_COMPETITOR_BASH_TOOL_BYTES ?? 1200);
const sha16 = value => hash(value).slice(0, 16);
const arg = name => process.argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const only = arg('only')?.split(',');
const kinds = arg('kind')?.split(',');

const { check, summary } = checks('competitors');

// ---------- task runner ----------
async function runTask(task) {
  const disabled = (task.octocode ?? []).map(step => step.tool ?? step.cli).find(tool => TOOL_POLICIES[tool]?.disabled);
  if (disabled) return { status: 'skipped', skipReason: `${disabled} is disabled by the current tool contract; refusal coverage belongs to tool-health` };
  const truth = liveTruth(task);
  if (!truth) return { status: 'skipped', skipReason: 'live truth unavailable' };
  if (task.kind === 'clasify' && process.env.OCTOCODE_COMPETITOR_CLASIFY !== '1') return { status: 'skipped', skipReason: 'paid provider calls (set OCTOCODE_COMPETITOR_CLASIFY=1)' };
  if (task.kind === 'github' && process.env.OCTOCODE_COMPETITOR_GITHUB === '0') return { status: 'skipped', skipReason: 'GitHub disabled (OCTOCODE_COMPETITOR_GITHUB=0)' };
  const ws = taskWorkspace(task);
  // Local tasks run twice; the second (warm server, warm page cache) is recorded.
  const repeats = task.kind === 'local' ? 2 : 1;
  let octocode, competitor, unfiltered, coldOcto, coldComp;
  for (let i = 0; i < repeats; i += 1) {
    if (task.codeSearch) await paceCodeSearch();
    coldOcto ??= octocode; octocode = await octocodeSide(task, truth, ws, console.log);
    if (task.codeSearch) await paceCodeSearch();
    coldComp ??= competitor; competitor = shellSide(task.competitor, truth, ws);
    if (octocode.skipped || competitor.skipped) return { status: 'skipped', skipReason: `rate limited (${octocode.skipped ? 'octocode' : 'competitor'})` };
  }
  // The unfiltered first try, or the expert recipe when it already is one.
  if (task.unfiltered) {
    if (task.codeSearch) await paceCodeSearch();
    unfiltered = shellSide(task.unfiltered, truth, ws);
    if (unfiltered.skipped) return { status: 'skipped', skipReason: 'rate limited (unfiltered)' };
  } else unfiltered = { ...competitor, sameAsExpert: true };
  if (coldOcto) octocode.coldMs = coldOcto.ms;
  if (coldComp) competitor.coldMs = coldComp.ms;
  if (REPLAY_MAX > 0) octocode.replay = await replayHints(octocode.entries, ws);
  const ratio = (a, b) => (b > 0 ? +(a / b).toFixed(2) : null);
  return {
    status: 'ran', workspace: ws || '.', truth, octocode, competitor, unfiltered,
    recipeHash: { octocode: sha16(task.octocode), shell: sha16(task.competitor) },
    ratios: { calls: ratio(octocode.calls, competitor.calls), bytes: ratio(octocode.bytes, competitor.bytes), ms: ratio(octocode.ms, competitor.ms) },
    unfilteredRatios: { calls: ratio(octocode.calls, unfiltered.calls), bytes: ratio(octocode.bytes, unfiltered.bytes), ms: ratio(octocode.ms, unfiltered.ms) },
  };
}

// ---------- fixed overhead ----------
async function fixedOverhead() {
  const plain = await startServer({ env: { OCTOCODE_BETA: 'false' } });
  const beta = await clientFor('');
  const out = {
    toolsListBytes: JSON.stringify(plain.tools).length, tools: plain.tools.length,
    instructionsBytes: (plain.init?.instructions ?? '').length,
    betaToolsListBytes: JSON.stringify(beta.tools).length, betaTools: beta.tools.length,
    bashToolDefBytes: BASH_TOOL_DEF_BYTES,
    assumption: 'Shell side = one Bash tool definition (name, ~1 paragraph description, {command, timeout, description} input schema) ≈ BASH_TOOL_DEF_BYTES; the host re-sends tool definitions every turn on both sides, so only the difference matters. Override with OCTOCODE_COMPETITOR_BASH_TOOL_BYTES.',
  };
  plain.close();
  out.octocodeTotal = out.toolsListBytes + out.instructionsBytes;
  out.deltaVsBash = out.octocodeTotal - BASH_TOOL_DEF_BYTES;
  return out;
}

// ---------- main ----------
const selected = ALL.filter(t => (!only || only.includes(t.id)) && (!kinds || kinds.includes(t.kind)));
const overhead = await fixedOverhead();
console.log(`fixed overhead: tools/list ${overhead.toolsListBytes} B (${overhead.tools} tools; beta ${overhead.betaToolsListBytes} B) + instructions ${overhead.instructionsBytes} B vs Bash tool ≈${BASH_TOOL_DEF_BYTES} B → +${overhead.deltaVsBash} B per request`);
console.log(`mode: ${NORMALIZED ? 'normalized (corpus-root workspaces, short briefs)' : 'legacy (octocode-root workspace, question briefs)'}`);
const results = [];
for (const task of selected) {
  const started = Date.now();
  let r;
  try { r = await runTask(task); } catch (error) { r = { status: 'error', error: error.message }; }
  results.push({ id: task.id, tool: task.tool, kind: task.kind, source: task.source, question: task.question, truthSource: task.truthSource, seconds: +((Date.now() - started) / 1000).toFixed(1), ...r });
  if (r.status === 'ran') {
    check(`${task.id} ${task.tool}: octocode answer is correct`, r.octocode.ok, r.octocode.ok ? `${r.octocode.calls} calls ${r.octocode.bytes} B ${r.octocode.ms} ms` : r.octocode.fails.join('; '));
    if (!r.competitor.ok) console.log(`INFO [competitors] ${task.id}: competitor answer wrong/incomplete — ${r.competitor.fails.join('; ').slice(0, 200)}`);
    if (!r.unfiltered.sameAsExpert && !r.unfiltered.ok) console.log(`INFO [competitors] ${task.id}: unfiltered first try wrong/incomplete — ${r.unfiltered.fails.join('; ').slice(0, 200)}`);
    const s = r.octocode.sensors;
    if (s?.violations.length) {
      const detail = s.violations.slice(0, 3).map(v => `${v.tool} ${v.at} (${v.signal})`).join('; ');
      // Only a recipe that walked every continuation to the end proves the
      // remainder is unreachable; otherwise the violation is reported.
      if (s.followedAll) check(`${task.id} ${task.tool}: never-trim (every truncation has a continuation or terminal-limit disclosure)`, false, `${s.violations.length} violations: ${detail}`);
      else console.log(`INFO [competitors] ${task.id}: never-trim violations ${s.violations.length} — ${detail}`);
    }
    if (s?.schemaErrors) check(`${task.id} ${task.tool}: no schema (input-validation) errors`, false, `${s.schemaErrors}: ${s.schemaErrorDetails.join(' | ')}`);
    const rp = r.octocode.replay;
    if (rp?.failed) check(`${task.id} ${task.tool}: continuations replay verbatim (C-3)`, false, rp.replays.filter(x => !x.ok).map(x => `${x.tool} ${x.name}: ${x.detail}`).join(' | '));
    if (s?.verboseBytes) console.log(`INFO [competitors] ${task.id}: verbose default fields ${s.verboseBytes} B — ${Object.entries(s.verbose).filter(([k]) => k !== 'debugRows').map(([k, v]) => `${k}×${v.count} ${v.bytes} B`).join(', ')}`);
  } else if (r.status === 'error') {
    check(`${task.id} ${task.tool}: harness ran the task`, false, r.error);
  } else console.log(`SKIP [competitors] ${task.id} ${task.tool}: ${r.skipReason}`);
}
closeClients();
removeShellScratch();

const ran = results.filter(r => r.status === 'ran');

// ---------- byte-regression gate (pinned anchor + rolling baseline) ----------
const readBaseline = name => { try { return JSON.parse(fs.readFileSync(path.join(RESULTS, name), 'utf8')); } catch { return null; } };
const baselines = { anchor: readBaseline(ANCHOR_FILE), rolling: readBaseline(ROLLING_FILE) };
const gated = [];
for (const r of ran) {
  r.gate = gateTask({ id: r.id, ...baselineRecord(r) }, baselines);
  if (r.gate.flags.length) { gated.push(r.id); check(`byte gate ${r.id} ${r.tool}: within ${GATE_FACTOR}× of anchor and rolling (bytes, calls, evidence)`, false, r.gate.flags.map(describeFlag).join('; ')); }
  else if (r.gate.notes.some(n => /recipe changed/.test(n))) console.log(`INFO [competitors] ${r.id}: ${r.gate.notes.join('; ')}`);
}
if (baselines.anchor || baselines.rolling) check(`byte gate: no task regressed vs ${['anchor', 'rolling'].filter(k => baselines[k]).join(' + ')}`, gated.length === 0, gated.length ? gated.join(',') : `${ran.length} tasks`);
else console.log(`INFO [competitors] byte gate: no baseline yet (seed with --update-anchor)`);
const schemaTotal = ran.reduce((a, r) => a + (r.octocode.sensors?.schemaErrors ?? 0), 0);
check('schema errors: 0 input-validation errors across recipes', schemaTotal === 0, `${schemaTotal}`);
const replayRows = ran.map(r => r.octocode.replay).filter(Boolean);
const replayFailed = replayRows.reduce((a, x) => a + x.failed, 0);
if (REPLAY_MAX > 0) check('verbatim replay: every replayed continuation validates', replayFailed === 0, `${replayRows.reduce((a, x) => a + x.replayed, 0)} replayed + ${replayRows.reduce((a, x) => a + x.byRecipe, 0)} run by recipes of ${replayRows.reduce((a, x) => a + x.offered, 0)} offered; ${replayFailed} failed`);

const table = ran.map(r => ({
  id: r.id, tool: r.tool, oOk: r.octocode.ok ? 'Y' : 'N', cOk: r.competitor.ok ? 'Y' : 'N', uOk: r.unfiltered.sameAsExpert ? '=' : r.unfiltered.ok ? 'Y' : 'N',
  oCalls: r.octocode.calls, cCalls: r.competitor.calls, uCalls: r.unfiltered.calls,
  oBytes: r.octocode.bytes, cBytes: r.competitor.bytes, uBytes: r.unfiltered.bytes, 'bytes×': r.ratios.bytes, 'u bytes×': r.unfilteredRatios.bytes,
  oMs: r.octocode.ms, cMs: r.competitor.ms, 'ms×': r.ratios.ms,
  nextShare: r.octocode.sensors?.nextShare, hintsShare: r.octocode.sensors?.hintsShare, leadMax: r.octocode.sensors?.leadEntriesMax, trimViol: r.octocode.sensors?.violations.length,
  schemaErr: r.octocode.sensors?.schemaErrors, verboseB: r.octocode.sensors?.verboseBytes, replay: r.octocode.replay ? `${r.octocode.replay.replayed - r.octocode.replay.failed}/${r.octocode.replay.replayed}+${r.octocode.replay.byRecipe}` : '', gate: r.gate?.flags.length ? 'FAIL' : r.gate?.notes.length === 2 && r.gate.notes.every(n => /no baseline/.test(n)) ? '—' : 'ok',
}));
console.table(table);
const tally = (metric, arm = 'competitor') => {
  const out = { win: 0, tie: 0, loss: 0 };
  for (const r of ran) {
    const a = r.octocode[metric], b = r[arm][metric];
    if (metric === 'ok') { if (a && !b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1; continue; }
    if (a < b) out.win += 1; else if (a === b) out.tie += 1; else out.loss += 1;
  }
  return out;
};
const perTool = {};
for (const r of ran) (perTool[r.tool] ??= []).push(r);
const toolSummary = Object.fromEntries(Object.entries(perTool).map(([tool, rows]) => [tool, {
  tasks: rows.length, octocodeCorrect: rows.filter(r => r.octocode.ok).length, competitorCorrect: rows.filter(r => r.competitor.ok).length, unfilteredCorrect: rows.filter(r => r.unfiltered.ok).length,
  medianCallsRatio: median(rows.map(r => r.ratios.calls), 2), medianBytesRatio: median(rows.map(r => r.ratios.bytes), 2), medianMsRatio: median(rows.map(r => r.ratios.ms), 2),
  medianBytesRatioUnfiltered: median(rows.map(r => r.unfilteredRatios.bytes), 2),
  medianNextShare: median(rows.map(r => r.octocode.sensors?.nextShare), 2), medianHintsShare: median(rows.map(r => r.octocode.sensors?.hintsShare), 2), medianLeadShare: median(rows.map(r => r.octocode.sensors?.leadShare), 2),
  maxNextEntriesPerRow: Math.max(0, ...rows.map(r => r.octocode.sensors?.nextEntriesMax ?? 0)), maxLeadEntriesPerMenu: Math.max(0, ...rows.map(r => r.octocode.sensors?.leadEntriesMax ?? 0)),
  neverTrimViolations: rows.reduce((a, r) => a + (r.octocode.sensors?.violations.length ?? 0), 0),
  schemaErrors: rows.reduce((a, r) => a + (r.octocode.sensors?.schemaErrors ?? 0), 0),
  verboseBytes: rows.reduce((a, r) => a + (r.octocode.sensors?.verboseBytes ?? 0), 0),
}]));
console.table(toolSummary);
const overall = {
  mode: NORMALIZED ? 'normalized' : 'legacy',
  tasks: selected.length, ran: ran.length, skipped: results.filter(r => r.status === 'skipped').length, errors: results.filter(r => r.status === 'error').length,
  octocodeCorrect: ran.filter(r => r.octocode.ok).length, competitorCorrect: ran.filter(r => r.competitor.ok).length, unfilteredCorrect: ran.filter(r => r.unfiltered.ok).length,
  medianCallsRatio: median(ran.map(r => r.ratios.calls), 2), medianBytesRatio: median(ran.map(r => r.ratios.bytes), 2), medianMsRatio: median(ran.map(r => r.ratios.ms), 2),
  medianBytesRatioUnfiltered: median(ran.map(r => r.unfilteredRatios.bytes), 2),
  callsLeCompetitor: ran.filter(r => r.octocode.calls <= r.competitor.calls).length,
  bytesLeCompetitor: ran.filter(r => r.octocode.bytes <= r.competitor.bytes).length,
  bytesLeUnfiltered: ran.filter(r => r.octocode.bytes <= r.unfiltered.bytes).length,
  wins: { correctness: tally('ok'), calls: tally('calls'), bytes: tally('bytes'), ms: tally('ms'), bytesVsUnfiltered: tally('bytes', 'unfiltered') },
  medianNextShare: median(ran.map(r => r.octocode.sensors?.nextShare), 2),
  medianHintsShare: median(ran.map(r => r.octocode.sensors?.hintsShare), 2),
  medianLeadShare: median(ran.map(r => r.octocode.sensors?.leadShare), 2),
  neverTrimViolations: ran.flatMap(r => (r.octocode.sensors?.violations ?? []).map(v => ({ id: r.id, followedAll: r.octocode.sensors.followedAll, ...v }))),
  schemaErrors: { total: schemaTotal, byTask: Object.fromEntries(ran.filter(r => r.octocode.sensors?.schemaErrors).map(r => [r.id, r.octocode.sensors.schemaErrors])) },
  verboseFields: (() => {
    const byRule = {};
    for (const r of ran) for (const [k, v] of Object.entries(r.octocode.sensors?.verbose ?? {})) {
      if (k === 'debugRows') continue;
      const x = (byRule[k] ??= { tasks: [], count: 0, bytes: 0 });
      x.tasks.push(r.id); x.count += v.count; x.bytes += v.bytes;
    }
    return { totalBytes: Object.values(byRule).reduce((a, x) => a + x.bytes, 0), byRule };
  })(),
  replay: REPLAY_MAX > 0 ? {
    perTaskMax: REPLAY_MAX,
    offered: replayRows.reduce((a, x) => a + x.offered, 0), byRecipe: replayRows.reduce((a, x) => a + x.byRecipe, 0), cliOnly: replayRows.reduce((a, x) => a + x.cliOnly, 0),
    replayed: replayRows.reduce((a, x) => a + x.replayed, 0), failed: replayFailed, rateLimited: replayRows.reduce((a, x) => a + x.replays.filter(y => y.rateLimited).length, 0),
    runtimeErrors: ran.flatMap(r => (r.octocode.replay?.replays ?? []).filter(y => y.ok && y.isError && !y.rateLimited).map(y => `${r.id} ${y.tool} ${y.name}`)),
  } : null,
  byteGate: { factor: GATE_FACTOR, anchor: baselines.anchor ? { at: baselines.anchor.updatedAt, tasks: Object.keys(baselines.anchor.tasks).length } : null, rolling: baselines.rolling ? { at: baselines.rolling.updatedAt, tasks: Object.keys(baselines.rolling.tasks).length } : null, flagged: gated },
};
console.log('overall', JSON.stringify({ ...overall, neverTrimViolations: overall.neverTrimViolations.length }));
console.table(overall.verboseFields.byRule);
const { results: checkResults, failed } = summary();

// Baselines: rolling only on an accepted run; the anchor only on an explicit flag.
const build = () => {
  const dist = path.join(ROOT, 'packages/octocode-mcp/dist/index.js');
  return { mcpDist: fs.existsSync(dist) ? sha16(fs.readFileSync(dist, 'utf8')) : null, nativeBinary: nativeBinaryStamp() };
};
const writeBaseline = (name, role) => {
  const prev = readBaseline(name);
  const tasks = { ...(prev?.tasks ?? {}) };
  for (const r of ran) tasks[r.id] = baselineRecord(r);
  const env = { clasify: process.env.OCTOCODE_COMPETITOR_CLASIFY === '1', github: process.env.OCTOCODE_COMPETITOR_GITHUB !== '0', normalized: NORMALIZED };
  fs.writeFileSync(path.join(RESULTS, name), JSON.stringify({ version: 1, role, factor: GATE_FACTOR, updatedAt: new Date().toISOString(), seededAt: prev?.seededAt ?? new Date().toISOString(), build: build(), env, tasks }, null, 2));
  console.log(`baseline: wrote ${role} ${name} (${ran.length} tasks updated, ${Object.keys(tasks).length} total)`);
};
const onlyGateFailed = failed.every(f => f.name.startsWith('byte gate'));
if (process.argv.includes('--update-anchor')) {
  if (onlyGateFailed) { writeBaseline(ANCHOR_FILE, 'anchor'); writeBaseline(ROLLING_FILE, 'rolling'); }
  else console.log('baseline: anchor NOT updated — checks other than the byte gate failed');
} else if (process.argv.includes('--update-rolling')) {
  if (!failed.length) writeBaseline(ROLLING_FILE, 'rolling');
  else console.log('baseline: rolling NOT updated — run not accepted');
}
const payload = { at: new Date().toISOString(), node: process.version, overhead, overall, toolSummary, table, tasks: results, checks: checkResults };
const file = writeResults('competitors', payload);
const saveAs = arg('save-as');
if (saveAs) writeResults(saveAs, payload);
console.log(`results: ${file}${saveAs ? ` (+ ${saveAs}.json)` : ''}`);
process.exitCode = failed.length ? 1 : 0;
