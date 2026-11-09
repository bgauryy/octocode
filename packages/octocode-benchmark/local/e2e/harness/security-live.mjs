// Opt-in live checks of securityCheck against OSV, GitHub and the public registries, run twice:
// anonymous (fresh home, no token variable, `gh` hidden) and authenticated (a fresh home with the
// caller's GitHub token as GH_TOKEN). Each probe asks the provider itself in the same run and
// compares the tool's row with that answer (security-live-oracle.mjs), never with a copied count.
//
// security-live-probes.mjs reproduces the securityCheck RFC's live evidence E1-E21
// (.octocode/security-check/rfc-robust.md, section 3); security-live-matrix.mjs the B169
// validation matrix. OSV takes no token, so the OSV-only provider probes (E18, E20, E21) run once,
// anonymously. GitHub allows 60 anonymous requests an hour per IP, which the E probes alone spend:
// the anonymous pass runs them only (provider truth is read with the caller's token when there is
// one), and the B169 matrix runs authenticated. A row that meets the limit must disclose it
// (sources[github] failed or unavailable, errorCode, why, sourceFailed); a call that instead waits
// for the reset is killed at the call deadline, fails, and the rest of that mode is skipped. A
// check that cannot run (no token, a provider read the limit refused) is a SKIP, never a pass.
//
//   OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs security-live
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { checks, writeResults } from './mcp-client.mjs';
import { contract, contractValidators } from './contract.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { snapshotBuild } from './build-snapshot.mjs';
import { CREDENTIAL_ENV } from '../../../../../test-utils/hermetic-env.ts';
import { contractPolicy } from './security-live-oracle.mjs';
import { configureLiveCalls, probes as recorded } from './security-live-calls.mjs';
import * as probes from './security-live-probes.mjs';
import { matrix, mcpParity, TRUST_MATRIX } from './security-live-matrix.mjs';
snapshotBuild();

if (process.env.OCTOCODE_LIVE !== '1') throw new Error('security-live requires OCTOCODE_LIVE=1');
const { check, skip, summary } = checks('security-live');
const provenance = { contract: contract.fingerprint, native: nativeBinaryStamp() };
const policy = contractPolicy(contract);
const timeoutMs = Number(process.env.OCTOCODE_TEST_CALL_TIMEOUT_MS ?? 240_000);
configureLiveCalls({ check, skip, validateOutput: contractValidators().validateOutput, timeoutMs });
const tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-security-live-')));
process.once('exit', () => fs.rmSync(tmp, { recursive: true, force: true }));

function callerToken() {
  for (const name of ['GH_TOKEN', 'GITHUB_TOKEN']) if (process.env[name]?.trim()) return { token: process.env[name].trim(), from: name };
  const gh = spawnSync('gh', ['auth', 'token'], { encoding: 'utf8' });
  return gh.status === 0 && gh.stdout.trim() ? { token: gh.stdout.trim(), from: 'gh auth token' } : null;
}

/** A fresh home per mode; `gh` is shadowed so the token source is exactly the one given. */
function modeOf(name, token, truthToken) {
  const root = path.join(tmp, name);
  const shim = path.join(root, 'bin');
  fs.mkdirSync(shim, { recursive: true });
  fs.writeFileSync(path.join(shim, 'gh'), '#!/bin/sh\necho "gh is hidden in security-live" >&2\nexit 1\n', { mode: 0o755 });
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !CREDENTIAL_ENV.test(key)));
  if (token) env.GH_TOKEN = token;
  Object.assign(env, { OCTOCODE_HOME: path.join(root, 'home'), GH_CONFIG_DIR: path.join(root, 'gh'), PATH: `${shim}${path.delimiter}${process.env.PATH}`, OCTOCODE_SECURITY_CHECKS: 'true', OCTOCODE_LSP_AUTO_INSTALL: 'off' });
  return { name, token, truthToken, env };
}

async function step(mode, label, fn) {
  if (mode.quotaWait) return skip(`${mode.name}.${label}`, mode.quotaWait);
  try {
    return await fn();
  } catch (error) {
    check(`${mode.name}.${label}.runs`, false, error.stack);
    return undefined;
  }
}

const quota = {};
async function run(mode) {
  const at = (label, fn) => step(mode, label, fn);
  quota[mode.name] = await at('E16', () => probes.e16(mode));
  await at('E17', () => probes.e17(mode));
  const osvOnly = mode.token ? [] : [['E18', probes.e18], ['E20', probes.e20], ['E21', probes.e21]];
  for (const [label, fn] of [...osvOnly, ['E19', probes.e19], ['E8', probes.e8], ['E14', probes.e14], ['E15', probes.e15]]) await at(label, () => fn(mode));
  const e9 = await at('E9', () => probes.e9(mode));
  const e11 = await at('E11', () => probes.e11(mode));
  await at('E12-E13', () => probes.e12e13(mode));
  for (const [id, repo] of [['E4', 'tmcw/leftpad'], ['E5', 'broofa/node-uuid'], ['E6', 'request/request'], ['E7', 'lodash/lodash']]) await at(id, () => probes.repository(mode, id, repo));
  for (const [label, fn] of [['E1', probes.e1], ['E2', probes.e2], ['E3', probes.e3]]) await at(label, () => fn(mode));
  if (mode.token) {
    for (const repo of TRUST_MATRIX) await at(`B169.trust.${repo}`, () => probes.repository(mode, `B169.trust.${repo}`, repo));
    await at('MCP', () => mcpParity(mode, {
      E9: { input: { operation: 'package', ecosystem: 'npm', packageName: 'lodash', version: '4.17.15' }, row: e9 },
      E11: { input: { operation: 'commit', commit: '6879efc2c1596d11a6a6ad296f80063b558d5e0f' }, row: e11 },
    }, timeoutMs));
    await at('B169', () => matrix(mode));
  }
}

const caller = callerToken();
await run(modeOf('anonymous', null, caller?.token));
if (caller) await run(modeOf('authenticated', caller.token, caller.token));
else skip('authenticated', 'no GitHub token: set GH_TOKEN or GITHUB_TOKEN, or sign in with `gh auth login`');
if (quota.anonymous && quota.authenticated) check('E16.anonymousBelowAuthenticated', quota.anonymous.limit < quota.authenticated.limit, `${quota.anonymous.limit} < ${quota.authenticated.limit}`);
provenance.stable = !!provenance.native && provenance.native.sha256 === nativeBinaryStamp()?.sha256;
check('built native artifact unchanged during the run', provenance.stable);
const result = summary();
writeResults('security-live', { ...result, provenance, policy, quota, tokenFrom: caller?.from ?? null, probes: recorded });
process.exitCode = result.failed.length ? 1 : 0;
