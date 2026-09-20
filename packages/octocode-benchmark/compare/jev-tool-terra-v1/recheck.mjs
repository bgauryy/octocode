// One post-observation development probe; never changes or resumes the frozen cohort.
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
const home = path.join(root, '.octocode/octocode-eval-benchmark/jev-tool-terra-post-observation-2026-09-20');
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const save = (name, value) => fs.writeFileSync(path.join(home, name), JSON.stringify(value, null, 2) + '\n');
if (process.argv[2] !== 'run') throw new Error('Explicit run required; one candidate Q5 probe, no baseline.');
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
const prompt = fs.readFileSync(path.join(prior, 'runs/Q5/prompt.txt'), 'utf8');
fs.writeFileSync(path.join(home, 'prompt.txt'), prompt);
fs.writeFileSync(path.join(home, 'PROTOCOL.md'), '# Post-observation Q5 development probe\n\nOne fresh Terra-medium candidate run with current built Octocode+optional Jev, no skill, unchanged Q5 prompt, same 300s/40 ordinary/20 Jev query limits and 250k ex-post token cap. No baseline. No retries. New contracts are frozen before inference. Selected after observing zero adoption in the original cohort, so this is not held-out or a causal efficiency estimate. Primary observation: does an actual Jev judgment change a documented next action? Guardrails: source-grounded answer, complete accounting, no prohibited tools or lost pagination. Zero calls remains a possible failed adoption observation; no call quota is inserted.\n');
const hashes = {};
for (const name of fs.readdirSync(snapshot)) hashes[`snapshot/${name}`] = digest(path.join(snapshot, name));
for (const name of ['prompt.txt', 'PROTOCOL.md']) hashes[name] = digest(path.join(home, name));
save('freeze.json', { created: new Date().toISOString(), files: hashes });
const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'jev-post-observation-'));
const configPath = path.join(home, 'proxy-config.json');
save('proxy-config.json', { version: 1, arm: 'candidate', entrypoint: path.join(snapshot, 'mcp.mjs'), runDir: home, cwd, requestTimeoutMs: 120000 });
const env = { ...process.env };
propagateOctocodeEnv({ env, trusted: false, cwd: root });
const globalConfig = loadOctocodeEnv({ home: getOctocodeHome(), trusted: false }).map;
for (const key of ['OCTOCODE_JEV_KEY', 'OCTOCODE_JEV_MODEL', 'OCTOCODE_JEV_BASE_URL']) if (!env[key] && globalConfig[key]) env[key] = globalConfig[key];
if (!env.OCTOCODE_JEV_KEY) throw new Error('Jev credential unavailable before inference');
if (!env.GITHUB_TOKEN && !env.GH_TOKEN) env.GITHUB_TOKEN = execFileSync('gh', ['auth', 'token'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
Object.assign(env, { OCTOCODE_HOME: path.join(home, 'octocode-home'), OCTOCODE_NATIVE_BINDING: path.join(snapshot, 'runtime.node'),
  OCTOCODE_REGEX_WORKER: path.join(snapshot, 'octocode-regex-worker'), OCTOCODE_JEV_MODEL: 'jev-1.13.0', ENABLE_LOCAL: 'false', ENABLE_CLONE: 'false', MAX_RETRIES: '0',
  OCTOCODE_ENABLE_STATS: 'true', OCTOCODE_STORAGE_MODE: 'persistent', JEV_BENCH_CONFIG: configPath });
fs.mkdirSync(env.OCTOCODE_HOME);
const { runAppServer } = await import(pathToFileURL(path.join(snapshot, 'runner.mjs')).href);
const record = await runAppServer({ cwd, env, model: 'gpt-5.6-terra', effort: 'medium', prompt, outputSchema, runDir: home,
  proxyPath: path.join(snapshot, 'proxy.mjs'), deadlineMs: 300000 });
record.runtimeUnchanged = Object.entries(hashes).every(([name, hash]) => digest(path.join(home, name)) === hash);
record.hostTokenEligible = record.usage?.length === 1 && record.usage[0].input_tokens + record.usage[0].output_tokens <= 250000;
save('record.json', record);
console.log(JSON.stringify(record));
