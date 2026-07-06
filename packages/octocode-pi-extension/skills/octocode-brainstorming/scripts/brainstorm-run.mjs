#!/usr/bin/env node

import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const REPO_ROOT = resolve(SKILL_DIR, '..', '..');

function now() {
  return new Date().toISOString();
}

function runDir() {
  return resolve(process.env.OCTOCODE_BRAINSTORM_RUN_DIR || join(REPO_ROOT, '.octocode', 'brainstorming', 'runs'));
}

function ensureRunDir() {
  const dir = runDir();
  mkdirSync(dir, { recursive: true });
  return dir;
}

function slug(value) {
  return String(value || 'brainstorm')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48) || 'brainstorm';
}

function makeRunId(idea) {
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  return `br_${stamp}_${slug(idea)}`;
}

function runPath(runId) {
  return join(ensureRunDir(), `${runId}.json`);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function writeRun(record) {
  record.updatedAt = now();
  writeFileSync(runPath(record.runId), `${JSON.stringify(record, null, 2)}\n`);
  return record;
}

function listRuns() {
  const dir = ensureRunDir();
  return readdirSync(dir)
    .filter(name => name.endsWith('.json'))
    .map(name => {
      const path = join(dir, name);
      return { path, mtimeMs: statSync(path).mtimeMs, record: readJson(path) };
    })
    .sort((a, b) => b.mtimeMs - a.mtimeMs);
}

function findRun(runId = '', cwd = '') {
  if (runId) {
    const path = runPath(runId);
    if (!existsSync(path)) throw new Error(`No brainstorming run found for id: ${runId}`);
    return readJson(path);
  }
  const runs = listRuns();
  const active = runs.find(row => row.record.status === 'active' && (!cwd || row.record.cwd === cwd));
  if (active) return active.record;
  const anyActive = runs.find(row => row.record.status === 'active');
  if (anyActive) return anyActive.record;
  return runs[0]?.record || null;
}

function parseArgs(argv) {
  const opts = { _: [], claim: [], source: [], json: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--run-id') { opts.runId = argv[++i] || ''; continue; }
    if (arg === '--idea') { opts.idea = argv[++i] || ''; continue; }
    if (arg === '--mode') { opts.mode = argv[++i] || ''; continue; }
    if (arg === '--surface-plan') { opts.surfacePlan = argv[++i] || ''; continue; }
    if (arg === '--stage') { opts.stage = argv[++i] || ''; continue; }
    if (arg === '--summary') { opts.summary = argv[++i] || ''; continue; }
    if (arg === '--claim') { opts.claim.push(argv[++i] || ''); continue; }
    if (arg === '--source') { opts.source.push(argv[++i] || ''); continue; }
    if (arg === '--verdict') { opts.verdict = argv[++i] || ''; continue; }
    if (arg === '--decision') { opts.decision = argv[++i] || ''; continue; }
    if (arg === '--eval-result') { opts.evalResult = argv[++i] || ''; continue; }
    if (arg === '--event') { opts.event = argv[++i] || ''; continue; }
    if (!opts._.length) { opts._.push(arg); continue; }
    throw new Error(`Unknown argument: ${arg}`);
  }
  return opts;
}

function parseMaybeJson(value) {
  if (!value) return null;
  try { return JSON.parse(value); } catch { return value; }
}

function emit(payload, json = true) {
  console.log(json ? JSON.stringify(payload, null, 2) : String(payload));
}

function startRun(opts) {
  if (!opts.idea) throw new Error('--idea is required for start');
  const createdAt = now();
  const record = {
    schemaVersion: 1,
    runId: opts.runId || makeRunId(opts.idea),
    idea: opts.idea,
    mode: opts.mode || 'Validate',
    status: 'active',
    stage: 'clarify',
    cwd: process.cwd(),
    createdAt,
    updatedAt: createdAt,
    surfacePlan: parseMaybeJson(opts.surfacePlan),
    claims: [],
    sources: [],
    events: [
      { at: createdAt, type: 'start', stage: 'clarify', summary: opts.idea },
    ],
    final: null,
    eval: null,
  };
  return writeRun(record);
}

function checkpointRun(opts) {
  const record = findRun(opts.runId);
  if (!record) throw new Error('No brainstorming run exists. Start one first.');
  const event = {
    at: now(),
    type: 'checkpoint',
    stage: opts.stage || record.stage || 'research',
    summary: opts.summary || '',
    claims: opts.claim.filter(Boolean),
    sources: opts.source.filter(Boolean),
  };
  record.stage = event.stage;
  record.events.push(event);
  record.claims.push(...event.claims.map(claim => ({ claim, at: event.at, stage: event.stage })));
  record.sources.push(...event.sources.map(source => ({ source, at: event.at, stage: event.stage })));
  return writeRun(record);
}

function finishRun(opts) {
  const record = findRun(opts.runId);
  if (!record) throw new Error('No brainstorming run exists. Start one first.');
  record.status = 'done';
  record.stage = 'present';
  record.final = {
    at: now(),
    verdict: opts.verdict || '',
    decision: opts.decision || '',
    summary: opts.summary || '',
  };
  if (opts.evalResult) {
    record.eval = parseMaybeJson(opts.evalResult);
  }
  record.events.push({ at: record.final.at, type: 'finish', stage: 'present', summary: record.final.summary });
  return writeRun(record);
}

function compactStatus(record) {
  if (!record) return { active: false, message: 'No brainstorming runs found.' };
  return {
    active: record.status === 'active',
    runId: record.runId,
    idea: record.idea,
    mode: record.mode,
    status: record.status,
    stage: record.stage,
    latestSummary: [...record.events].reverse().find(event => event.summary)?.summary || '',
    claims: record.claims.length,
    sources: record.sources.length,
    decision: record.final?.decision || '',
    verdict: record.final?.verdict || '',
    updatedAt: record.updatedAt,
  };
}

function readStdinJson() {
  const text = readFileSync(0, 'utf8');
  if (!text.trim()) return {};
  try { return JSON.parse(text); } catch { return {}; }
}

function hookAdditionalContext(record) {
  const status = compactStatus(record);
  const missing = [];
  if (status.active) {
    if (!record.surfacePlan) missing.push('surface plan');
    if (!record.claims.length) missing.push('claim ledger');
    if (!record.sources.length) missing.push('sources');
    if (!record.final) missing.push('final decision');
  }
  const lines = [
    `octocode-brainstorming active run: ${status.runId}`,
    `idea: ${status.idea}`,
    `mode/stage: ${status.mode} / ${status.stage}`,
    status.latestSummary ? `latest: ${status.latestSummary}` : '',
    missing.length ? `missing before final: ${missing.join(', ')}` : 'ready for final synthesis if research is complete',
    'Use scripts/brainstorm-run.mjs checkpoint/finish to keep this run resumable.',
  ].filter(Boolean);
  return {
    hookSpecificOutput: {
      hookEventName: 'UserPromptSubmit',
      additionalContext: lines.join('\n'),
    },
  };
}

function hook(opts) {
  const input = readStdinJson();
  const event = opts.event || input.hook_event_name || input.hookEventName || '';
  const cwd = input.cwd || process.cwd();
  const record = findRun('', cwd);
  if (!record || record.status !== 'active') return { ok: true, noop: true, event };

  if (event === 'UserPromptSubmit') {
    return hookAdditionalContext(record);
  }

  if (event === 'SubagentStop') {
    record.events.push({
      at: now(),
      type: 'subagent-stop',
      stage: record.stage,
      summary: input.agent_id || input.session_id || 'subagent stopped',
    });
    writeRun(record);
    return { ok: true, event, recorded: true, runId: record.runId };
  }

  if (event === 'Stop') {
    if (input.stop_hook_active) return { ok: true, noop: true, event, loopGuard: true };
    if (process.env.OCTOCODE_BRAINSTORM_NO_STOP_GATE === '1') return { ok: true, noop: true, event, disabled: true };
    process.stderr.write(
      `octocode-brainstorming: active run ${record.runId} has not been finished. ` +
      'Record a final verdict/decision with brainstorm-run.mjs finish, or set OCTOCODE_BRAINSTORM_NO_STOP_GATE=1 to bypass.\n',
    );
    process.exitCode = 2;
    return null;
  }

  if (event === 'SessionEnd') {
    record.events.push({ at: now(), type: 'session-end', stage: record.stage, summary: 'session ended with active run' });
    writeRun(record);
    return { ok: true, event, recorded: true, runId: record.runId };
  }

  return { ok: true, noop: true, event };
}

function usage() {
  return `Brainstorming run ledger and hook entrypoint

Usage:
  node scripts/brainstorm-run.mjs start --idea "..." --mode Validate --surface-plan '{"local":"active"}'
  node scripts/brainstorm-run.mjs checkpoint --run-id <id> --stage research --summary "..." --claim "..." --source "..."
  node scripts/brainstorm-run.mjs finish --run-id <id> --verdict worth-prototyping --decision "Build RFC" --summary "..."
  node scripts/brainstorm-run.mjs status [--run-id <id>]
  node scripts/brainstorm-run.mjs hook --event UserPromptSubmit < hook-payload.json
  node scripts/brainstorm-run.mjs --self-test

Environment:
  OCTOCODE_BRAINSTORM_RUN_DIR       Override run storage directory
  OCTOCODE_BRAINSTORM_NO_STOP_GATE  Disable Stop-hook blocking

Default run dir: ${runDir()}`;
}

function runSelfTest() {
  const oldDir = process.env.OCTOCODE_BRAINSTORM_RUN_DIR;
  process.env.OCTOCODE_BRAINSTORM_RUN_DIR = mkdtempSync(join(tmpdir(), 'brainstorm-run-'));
  try {
    const started = startRun({ idea: 'Test idea', mode: 'Validate', surfacePlan: '{"local":"active"}', claim: [], source: [] });
    checkpointRun({ runId: started.runId, stage: 'research', summary: 'Found evidence', claim: ['claim one'], source: ['skills/octocode-brainstorming/SKILL.md:57'] });
    const active = compactStatus(findRun(started.runId));
    if (!active.active || active.claims !== 1 || active.sources !== 1) throw new Error('active status mismatch');
    const payload = hookAdditionalContext(findRun(started.runId));
    if (!payload.hookSpecificOutput?.additionalContext.includes(started.runId)) throw new Error('hook payload missing run id');
    const finished = finishRun({ runId: started.runId, verdict: 'worth-prototyping', decision: 'Build RFC', summary: 'Ready' });
    if (finished.status !== 'done' || finished.final.decision !== 'Build RFC') throw new Error('finish mismatch');
    return { ok: true, runDir: runDir(), runId: started.runId };
  } finally {
    if (oldDir === undefined) delete process.env.OCTOCODE_BRAINSTORM_RUN_DIR;
    else process.env.OCTOCODE_BRAINSTORM_RUN_DIR = oldDir;
  }
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    die(err.message || String(err));
    return;
  }
  if (opts.help) {
    console.log(usage());
    return;
  }
  if (opts.selfTest) {
    try {
      emit(runSelfTest(), true);
    } catch (err) {
      die(err.message || String(err));
    }
    return;
  }

  const command = opts._[0] || 'status';
  try {
    if (command === 'start') emit(startRun(opts), true);
    else if (command === 'checkpoint') emit(checkpointRun(opts), true);
    else if (command === 'finish') emit(finishRun(opts), true);
    else if (command === 'status') emit(compactStatus(findRun(opts.runId)), true);
    else if (command === 'hook') {
      const result = hook(opts);
      if (result) emit(result, true);
    } else {
      throw new Error(`Unknown command: ${command}`);
    }
  } catch (err) {
    die(err.message || String(err));
  }
}

main();
