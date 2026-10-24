// Isolated built-host fixture with a loopback classifier and recorded CLI calls.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { hermeticEnv } from '../../../../../test-utils/hermetic-env.ts';
import { spawnCli } from './mcp-client.mjs';
import { FIXTURES, ROOT } from './paths.mjs';
import { startMock } from './tool-health/mock-server.mjs';

export function answer(request) {
  const answers = Object.fromEntries(Object.entries(request.questions).map(([id, question]) => {
    assert.equal(question.type, 'noul');
    return [id, { type: 'noul', noul: 0.1 }];
  }));
  return { model: request.model, answers, usage: { input_tokens: 10, output_tokens: 2 } };
}

export async function setup({ classificationResponder, label }) {
  const scratch = path.join(ROOT, '.octocode/tmp/classification-e2e');
  fs.mkdirSync(scratch, { recursive: true });
  const fixture = fs.mkdtempSync(path.join(scratch, `${label}-`));
  fs.cpSync(path.join(FIXTURES, 'tool-health'), fixture, { recursive: true });
  const mock = await startMock({ fixtureDir: fixture, token: 'fixture-token', classificationResponder });
  const initialized = spawnSync('git', ['init', '--quiet', fixture], { encoding: 'utf8' });
  assert.equal(initialized.status, 0, initialized.stderr);
  const isolated = hermeticEnv(process.env, `classification-${label}`);
  const env = {
    ...isolated, GITHUB_TOKEN: 'fixture-token', GITHUB_API_URL: `${mock.url}/api/v3`,
    OCTOCODE_CLASSIFICATION_API: 'fixture-key', OCTOCODE_CLASSIFICATION_API_HOST: mock.url,
    OCTOCODE_STORAGE_MODE: 'memory', OCTOCODE_ALLOW_PRIVATE_REGISTRY: 'true', OCTOCODE_ENABLE_LOCAL: 'true', OCTOCODE_BETA: 'true',
    OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off',
    DISABLE_TOOLS: '', TOOLS_TO_RUN: '', ALLOWED_PATHS: fixture, WORKSPACE_ROOT: fixture,
  };
  const trace = [];
  const cliArgs = async (args, overrides = {}) => {
    const run = await spawnCli(args, { cwd: fixture, env: { ...env, ...overrides }, timeoutMs: 60_000 });
    const result = { json: JSON.parse(run.stdout), rawResponse: run.stdout, stderr: run.stderr, code: run.status, ms: run.ms };
    trace.push({ args, ...result });
    return result;
  };
  const cli = (tool, query, overrides) => cliArgs([tool, JSON.stringify({ queries: [query] }), '--json'], overrides);
  const close = async () => {
    await mock.close();
    fs.rmSync(fixture, { recursive: true, force: true });
    fs.rmSync(isolated.OCTOCODE_HOME, { recursive: true, force: true });
  };
  return { fixture, env, mock, cli, cliArgs, close, trace, calls: () => mock.classificationRequests.length };
}
