// One explicitly selected development probe; never changes or resumes a frozen cohort.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { build } from 'esbuild';
import { propagateOctocodeEnv, loadOctocodeEnv, getOctocodeHome } from '@octocodeai/config';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../../..');
const prior = path.join(root, '.octocode/octocode-eval-benchmark/jev-tool-terra-30-2026-09-20-v6');
const caseFile = process.argv[3] ? path.resolve(process.argv[3]) : null;
const probe = caseFile ? JSON.parse(fs.readFileSync(caseFile, 'utf8')) : {
  id: 'Q5', directoryName: 'jev-tool-terra-post-observation-2026-09-20',
  prompt: fs.readFileSync(path.join(prior, 'runs/Q5/prompt.txt'), 'utf8'),
  protocol: '# Post-observation Q5 development probe\n\nOne fresh Terra-medium candidate run with current built Octocode+optional Jev, no skill, unchanged Q5 prompt, same 300s/40 ordinary/20 Jev query limits and 250k ex-post token cap. No baseline. No retries. New contracts are frozen before inference. Selected after observing zero adoption in the original cohort, so this is not held-out or a causal efficiency estimate. Primary observation: does an actual Jev judgment change a documented next action? Guardrails: source-grounded answer, complete accounting, no prohibited tools or lost pagination. Zero calls remains a possible failed adoption observation; no call quota is inserted.\n',
};
if (!probe || Object.keys(probe).some(key => !['id', 'directoryName', 'prompt', 'protocol'].includes(key)) ||
  !/^[A-Za-z0-9_-]+$/.test(probe.id ?? '') || !/^[a-z0-9][a-z0-9-]+$/.test(probe.directoryName ?? '') ||
  ![probe.prompt, probe.protocol].every(value => typeof value === 'string' && value.trim())) throw new Error('Invalid explicit probe case');
const home = path.join(root, '.octocode/octocode-eval-benchmark', probe.directoryName);
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const save = (name, value) => fs.writeFileSync(path.join(home, name), JSON.stringify(value, null, 2) + '\n');
if (process.argv[2] !== 'run') throw new Error('Use run [case.json]; one candidate probe, no baseline.');
async function run() {
fs.mkdirSync(home, { recursive: true });
fs.writeFileSync(path.join(home, 'RESERVED'), new Date().toISOString(), { flag: 'wx' });
const snapshot = path.join(home, 'snapshot');
fs.mkdirSync(snapshot);
const options = { bundle: true, platform: 'node', format: 'esm', target: 'node24',
  banner: { js: 'import {createRequire as __benchRequire} from "node:module"; const require=__benchRequire(import.meta.url);' } };
for (const [entry, name] of [[path.join(root, 'packages/octocode-mcp/dist/index.js'), 'mcp.mjs'],
  [path.join(here, 'mcp-proxy.mjs'), 'proxy.mjs'], [path.join(here, 'appserver-runner.mjs'), 'runner.mjs']]) {
  await build({ ...options, entryPoints: [entry], outfile: path.join(snapshot, name) });
}
// N-API and worker are copied only after the parent completes the current build.
fs.copyFileSync(path.join(root, 'packages/octocode-native/octocode-native.darwin-arm64.node'), path.join(snapshot, 'runtime.node'));
fs.copyFileSync(path.join(root, 'packages/octocode-native/npm/darwin-arm64/octocode-regex-worker'), path.join(snapshot, 'octocode-regex-worker'));
const outputSchema = JSON.parse(fs.readFileSync(path.join(prior, 'answer-schema.json'), 'utf8'));
const prompt = probe.prompt;
fs.copyFileSync(fileURLToPath(import.meta.url), path.join(snapshot, 'recheck.source.mjs'));
save('answer-schema.json', outputSchema);
fs.writeFileSync(path.join(home, 'prompt.txt'), prompt);
fs.writeFileSync(path.join(home, 'PROTOCOL.md'), probe.protocol);
save('case.json', probe);
const hashes = {};
for (const name of fs.readdirSync(snapshot)) hashes[`snapshot/${name}`] = digest(path.join(snapshot, name));
for (const name of ['prompt.txt', 'PROTOCOL.md', 'answer-schema.json', 'case.json']) hashes[name] = digest(path.join(home, name));
const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'jev-post-observation-'));
const configPath = path.join(home, 'proxy-config.json');
save('proxy-config.json', { version: 1, arm: 'candidate', entrypoint: path.join(snapshot, 'mcp.mjs'), runDir: home, cwd, requestTimeoutMs: 120000 });
hashes['proxy-config.json'] = digest(configPath);
save('freeze.json', { version: 1, created: new Date().toISOString(), requestedModel: 'gpt-5.6-terra', effort: 'medium',
  arm: 'candidate', case: probe.id, postObservation: true, budget: { seconds: 300, ordinaryQueries: 40, jevQueries: 20, hostTokenEligibilityCap: 250000 },
  node: process.version, codex: execFileSync('codex', ['--version'], { encoding: 'utf8' }).trim(),
  repoHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), files: hashes });
const env = { ...process.env };
propagateOctocodeEnv({ env, trusted: false, cwd: root });
const globalConfig = loadOctocodeEnv({ home: getOctocodeHome(), trusted: false }).map;
for (const key of ['OCTOCODE_JEV_KEY', 'OCTOCODE_JEV_MODEL', 'OCTOCODE_JEV_BASE_URL']) if (!env[key] && globalConfig[key]) env[key] = globalConfig[key];
if (!env.OCTOCODE_JEV_KEY) throw new Error('Jev credential unavailable before inference');
if (!env.GITHUB_TOKEN && !env.GH_TOKEN) {
  try { env.GITHUB_TOKEN = execFileSync('gh', ['auth', 'token'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(); }
  catch { throw new Error('GitHub credential unavailable before inference'); }
}
Object.assign(env, { OCTOCODE_HOME: path.join(home, 'octocode-home'), OCTOCODE_NATIVE_BINDING: path.join(snapshot, 'runtime.node'),
  OCTOCODE_REGEX_WORKER: path.join(snapshot, 'octocode-regex-worker'), OCTOCODE_JEV_MODEL: 'jev-1.13.0', ENABLE_LOCAL: 'false', ENABLE_CLONE: 'false', MAX_RETRIES: '0',
  OCTOCODE_ENABLE_STATS: 'true', OCTOCODE_STORAGE_MODE: 'persistent', JEV_BENCH_CONFIG: configPath });
fs.mkdirSync(env.OCTOCODE_HOME);
const { runAppServer } = await import(pathToFileURL(path.join(snapshot, 'runner.mjs')).href);
const record = await runAppServer({ cwd, env, model: 'gpt-5.6-terra', effort: 'medium', prompt, outputSchema, runDir: home,
  proxyPath: path.join(snapshot, 'proxy.mjs'), deadlineMs: 300000 });
// Preserve the returned usage even if a subsequent receipt-integrity check fails.
save('record.json', record);
record.runtimeUnchanged = false;
try { record.runtimeUnchanged = Object.entries(hashes).every(([name, hash]) => digest(path.join(home, name)) === hash); } catch {}
Object.assign(record, { id: probe.id, arm: 'candidate', requestedModel: 'gpt-5.6-terra', effort: 'medium', postObservation: true });
let receipts = [];
record.receiptsValid = false;
try {
  receipts = fs.readFileSync(path.join(home, 'calls.jsonl'), 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
  record.receiptsValid = true;
} catch {}
record.catalogPresent = receipts.some(r => r.event === 'catalog' && r.tools?.some(t => t.name === 'jev'));
record.answerPresent = fs.existsSync(path.join(home, 'answer.json'));
record.answerValid = false;
try {
  const a = JSON.parse(fs.readFileSync(path.join(home, 'answer.json'), 'utf8'));
  record.answerValid = typeof a.answer === 'string' && a.answer.trim().length > 0 && typeof a.jevAssessment === 'string' &&
    Array.isArray(a.limitations) && a.limitations.every(x => typeof x === 'string') && Array.isArray(a.citations) &&
    a.citations.every(x => typeof x.url === 'string' && typeof x.claim === 'string');
} catch {}
record.usageValid = record.usage?.length === 1 && [record.usage[0].input_tokens, record.usage[0].output_tokens]
  .every(n => Number.isSafeInteger(n) && n >= 0) && (record.usage[0].cached_input_tokens == null ||
    Number.isSafeInteger(record.usage[0].cached_input_tokens) && record.usage[0].cached_input_tokens >= 0 &&
    record.usage[0].cached_input_tokens <= record.usage[0].input_tokens);
record.hostTokenEligible = record.usageValid && record.usage[0].input_tokens + record.usage[0].output_tokens <= 250000;
save('record.json', record);
console.log(JSON.stringify(record));
if (!record.runtimeUnchanged || record.prohibitedToolEvents || !record.usageValid || !record.catalogPresent || !record.receiptsValid)
  throw new Error('Instrumentation/adherence gate failed; inspect preserved probe. Missing usage is unknown.');
if (record.exitCode !== 0 || record.timedOut || !record.answerValid || !record.hostTokenEligible)
  throw new Error('Probe failed completion/eligibility gates; preserve this outcome without retrying.');
}

try { await run(); }
catch (error) {
  if (fs.existsSync(path.join(home, 'RESERVED')) && !fs.existsSync(path.join(home, 'record.json')))
    save('record.json', { id: probe.id, arm: 'candidate', requestedModel: 'gpt-5.6-terra', postObservation: true,
      exitCode: 1, usage: [], answerValid: false, hostTokenEligible: false,
      error: 'Runner setup or execution failed; inspect retained receipts. Missing usage is unknown.' });
  throw error;
}
