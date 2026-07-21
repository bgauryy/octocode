#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { getOctocodeHome, propagateOctocodeEnv } from '@octocodeai/config';

const args = process.argv.slice(2);
const cmd = args[0];
const arg = (flag, fallback) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : fallback; };
const has = (flag) => args.includes(flag);
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

function octocodeOutputBase() {
  const workspace = resolve(process.cwd(), '.octocode');
  try {
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
    return workspace;
  } catch {
    const home = getOctocodeHome();
    mkdirSync(home, { recursive: true, mode: 0o700 });
    return home;
  }
}

const outputBase = octocodeOutputBase();
const requestedRunRoot = process.env.OCTOCODE_BRAINSTORM_RUN_DIR;
const runRoot = requestedRunRoot ? resolve(requestedRunRoot) : join(outputBase, 'brainstorming', 'runs');
if (!runRoot.startsWith(`${outputBase}/`) && runRoot !== outputBase) {
  throw new Error(`OCTOCODE_BRAINSTORM_RUN_DIR must be under ${outputBase}`);
}
function ensure() { mkdirSync(runRoot, { recursive: true, mode: 0o700 }); }
function nowId() { return new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14); }
function fileFor(id) { return join(runRoot, `${id}.json`); }
function readJson(p, fallback) { return existsSync(p) ? JSON.parse(readFileSync(p, 'utf8')) : fallback; }
function writeRun(run) { ensure(); writeFileSync(fileFor(run.id), `${JSON.stringify(run, null, 2)}\n`); }
function latestActive() {
  ensure();
  return readdirSync(runRoot).filter((f) => f.endsWith('.json')).map((f) => readJson(join(runRoot, f), null)).filter(Boolean).filter((r) => r.status !== 'finished').sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)))[0];
}

async function main() {
  if (has('--self-test')) { ensure(); console.log(`brainstorm-run: ok ${runRoot}`); return; }
  if (cmd === 'start') {
    const id = arg('--run-id', nowId());
    const run = { id, idea: arg('--idea', ''), mode: arg('--mode', 'Generate'), surfacePlan: JSON.parse(arg('--surface-plan', '{}')), status: 'active', checkpoints: [], createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
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
    const run = latestActive();
    if (!run) return;
    if (event === 'Stop' && process.env.OCTOCODE_BRAINSTORM_NO_STOP_GATE !== '1') {
      console.error(`Active brainstorming run ${run.id}; checkpoint or finish before stopping.`);
      process.exit(2);
    }
    if (event === 'UserPromptSubmit') console.log(`[BRAINSTORM_RUN] ${run.id} stage=${run.checkpoints.at(-1)?.stage || 'start'} summary=${run.checkpoints.at(-1)?.summary || run.idea}`);
    return;
  }
  throw new Error(`Unknown command: ${cmd || '(none)'}`);
}
main().catch((e) => { console.error(e.message); process.exit(1); });
