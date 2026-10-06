#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';
import { propagateOctocodeEnv } from './octocode-config.mjs';

const args = process.argv.slice(2);
const cmd = args[0];
const arg = (flag, fallback) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : fallback; };
const has = (flag) => args.includes(flag);

if (has('--help')) {
  console.log(`brainstorm-run — resumable claim/source/decision ledger for a brainstorming run

  node scripts/brainstorm-run.mjs start      --idea <text> [--mode Generate|Validate|Map] [--surface-plan <json>] [--run-id <id>] [--session-id <id>]
  node scripts/brainstorm-run.mjs checkpoint --run-id <id> [--stage <name>] [--summary <text>] [--claim <text>] [--source <path-or-url>]
  node scripts/brainstorm-run.mjs finish     --run-id <id> [--verdict <text>] [--decision <text>] [--summary <text>]
  node scripts/brainstorm-run.mjs hook       --event UserPromptSubmit|Stop   (reads the host hook JSON on stdin)
  node scripts/brainstorm-run.mjs --self-test    create the run directory and print it
  --help                                         this text

Runs are JSON under <workspace>/.octocode/brainstorming/runs/. An unwritable workspace is an error;
OCTOCODE_BRAINSTORM_RUN_DIR overrides the run folder but must stay under the workspace .octocode root.
A run belongs to the session that started it (--session-id, else CLAUDE_CODE_SESSION_ID).
The Stop hook exits 2 only for that session's unfinished run, once per stop; other sessions,
unowned runs, and runs idle for 24h never block. OCTOCODE_BRAINSTORM_NO_STOP_GATE=1 bypasses it.`);
  process.exit(0);
}
// Hooks stay fast and never read secrets, so they skip .env loading.
if (cmd !== 'hook') propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

const outputBase = resolve(process.cwd(), '.octocode');
const STALE_MS = 24 * 60 * 60 * 1000;
const requestedRunRoot = process.env.OCTOCODE_BRAINSTORM_RUN_DIR;
const runRoot = requestedRunRoot ? resolve(requestedRunRoot) : join(outputBase, 'brainstorming', 'runs');
const runRootRelative = relative(outputBase, runRoot);
if (runRootRelative.startsWith('..') || isAbsolute(runRootRelative)) {
  throw new Error(`OCTOCODE_BRAINSTORM_RUN_DIR must be under ${outputBase}`);
}
function ensure() { mkdirSync(runRoot, { recursive: true, mode: 0o700 }); }
function nowId() { return new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14); }
function fileFor(id) { return join(runRoot, `${id}.json`); }
function readJson(p, fallback) { return existsSync(p) ? JSON.parse(readFileSync(p, 'utf8')) : fallback; }
function writeRun(run) { ensure(); writeFileSync(fileFor(run.id), `${JSON.stringify(run, null, 2)}\n`); }
function latestActive(sessionId) {
  if (!existsSync(runRoot)) return undefined;
  return readdirSync(runRoot).filter((f) => f.endsWith('.json')).map((f) => readJson(join(runRoot, f), null)).filter(Boolean)
    .filter((r) => r.status !== 'finished' && (!sessionId || !r.sessionId || r.sessionId === sessionId))
    .sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)))[0];
}
function readHookInput() {
  if (process.stdin.isTTY) return {};
  try { return JSON.parse(readFileSync(0, 'utf8') || '{}'); } catch { return {}; }
}

async function main() {
  if (has('--self-test')) { ensure(); console.log(`brainstorm-run: ok ${runRoot}`); return; }
  if (cmd === 'start') {
    const id = arg('--run-id', nowId());
    const run = { id, sessionId: arg('--session-id', process.env.CLAUDE_CODE_SESSION_ID || null), idea: arg('--idea', ''), mode: arg('--mode', 'Generate'), surfacePlan: JSON.parse(arg('--surface-plan', '{}')), status: 'active', checkpoints: [], createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
    writeRun(run);
    console.log(JSON.stringify({ runId: id, path: fileFor(id) }));
    return;
  }
  if (cmd === 'checkpoint') {
    const id = arg('--run-id');
    if (!id) throw new Error('--run-id required');
    const run = readJson(fileFor(id), null);
    if (!run) throw new Error(`run not found: ${id}`);
    run.checkpoints.push({ at: new Date().toISOString(), stage: arg('--stage', 'unknown'), summary: arg('--summary', ''), claim: arg('--claim', ''), source: arg('--source', '') });
    run.updatedAt = new Date().toISOString();
    writeRun(run);
    console.log(JSON.stringify({ runId: id, checkpoints: run.checkpoints.length }));
    return;
  }
  if (cmd === 'finish') {
    const id = arg('--run-id');
    if (!id) throw new Error('--run-id required');
    const run = readJson(fileFor(id), null);
    if (!run) throw new Error(`run not found: ${id}`);
    Object.assign(run, { status: 'finished', verdict: arg('--verdict', ''), decision: arg('--decision', ''), summary: arg('--summary', ''), updatedAt: new Date().toISOString(), finishedAt: new Date().toISOString() });
    writeRun(run);
    console.log(JSON.stringify({ runId: id, status: run.status }));
    return;
  }
  if (cmd === 'hook') {
    const event = arg('--event', 'unknown');
    const input = readHookInput();
    const run = latestActive(input.session_id);
    if (!run) return;
    const owned = Boolean(run.sessionId) && run.sessionId === input.session_id;
    const fresh = Date.now() - Date.parse(run.updatedAt) < STALE_MS;
    if (event === 'Stop' && owned && fresh && !input.stop_hook_active && process.env.OCTOCODE_BRAINSTORM_NO_STOP_GATE !== '1') {
      console.error(`Active brainstorming run ${run.id}; checkpoint or finish before stopping.`);
      process.exit(2);
    }
    if (event === 'UserPromptSubmit') console.log(`[BRAINSTORM_RUN] ${run.id} stage=${run.checkpoints.at(-1)?.stage || 'start'} summary=${run.checkpoints.at(-1)?.summary || run.idea}`);
    return;
  }
  throw new Error(`Unknown command: ${cmd || '(none)'}`);
}
main().catch((e) => { console.error(e.message); process.exit(1); });
