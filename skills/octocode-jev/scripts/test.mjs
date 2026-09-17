import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, mkdir, writeFile, rm, cp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const binary = join(root, 'bin', `octocode-jev-${process.platform}-${process.arch}${process.platform === 'win32' ? '.exe' : ''}`);
const fixture = {
  state: { message: 'My invoice was charged twice' },
  questions: {
    route: { type: 'choice', instructions: 'Choose the team', criteria: { billing: null, support: { description: 'Technical support' } } },
    urgency: { type: 'score', instructions: 'Rate urgency', criteria: ['Low', 'High'] },
    refund: { type: 'noul', instructions: 'Is a refund needed?' }
  }
};
const valid = () => ({
  model: 'jev-test',
  answers: {
    route: { type: 'choice', choice: 'billing', probabilities: { billing: 0.8, support: 0.2 }, confidence: 0.6 },
    urgency: { type: 'score', score: 0.75, probabilities: { '0': 0.25, '1': 0.75 }, legend: { '0': 'Low', '1': 'High' }, confidence: 0.5 },
    refund: { type: 'noul', noul: 0.9 }
  }, usage: { input_tokens: 40, output_tokens: 0 }
});
function run(args, { input, env = {}, cwd = root, launcher = false, base = root } = {}) {
  return new Promise((resolveResult, reject) => {
    const clean = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith('OCTOCODE_') && !['REQUEST_TIMEOUT', 'MAX_RETRIES'].includes(k)));
    const child = spawn(launcher ? process.execPath : binary, launcher ? [join(base, 'scripts/jev.mjs'), ...args] : args, {
      cwd, env: { ...clean, OCTOCODE_JEV_KEY: 'test-key', ...env }, stdio: ['pipe', 'pipe', 'pipe']
    });
    let stdout = '', stderr = '';
    child.stdout.on('data', x => stdout += x);
    child.stderr.on('data', x => stderr += x);
    child.on('error', reject);
    child.on('close', (code, signal) => resolveResult({ code, signal, stdout, stderr }));
    child.stdin.on('error', () => {});
    child.stdin.end(input === undefined ? '' : typeof input === 'string' ? input : JSON.stringify(input));
  });
}
async function serverTest(fn, action) {
  const requests = [];
  const server = createServer(async (req, res) => {
    let text = ''; for await (const chunk of req) text += chunk;
    requests.push({ method: req.method, url: req.url, auth: req.headers.authorization, body: text ? JSON.parse(text) : null });
    await fn(req, res, requests.length);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const endpoint = `http://127.0.0.1:${server.address().port}`;
  try { await action(endpoint, requests); }
  finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
}
const reply = body => (_req, res) => { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(body)); };

test('help, version, unknown flags, dry run without a key', async () => {
  assert.equal((await run(['--help'])).code, 0);
  assert.match((await run(['--version'])).stdout, /0\.1\.0/);
  assert.equal((await run(['--bogus'])).code, 2);
  const result = await run(['evaluate', '--dry-run'], { input: fixture, env: { OCTOCODE_JEV_KEY: '' } });
  assert.equal(result.code, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).model, 'jev-latest');
  assert.equal((await run(['evaluate'], { input: fixture, env: { OCTOCODE_JEV_KEY: '' } })).code, 2);
});
test('invalid requests fail locally with empty stdout', async () => {
  for (const body of [
    {}, { ...fixture, state: 4 }, { ...fixture, questions: {} },
    { ...fixture, questions: { q: { type: 'score', criteria: ['Only'] } } },
    { ...fixture, questions: { q: { type: 'choice', criteria: Object.fromEntries(Array.from({ length: 256 }, (_, i) => [i, null])) } } },
    { ...fixture, questions: { q: { type: 'noul', criteria: { maybe: 'bad' } } } },
    { ...fixture, stream: true }, 'not-json'
  ]) {
    const result = await run(['evaluate', '--dry-run'], { input: body });
    assert.equal(result.code, 2); assert.equal(result.stdout, '');
  }
});
test('all primitives use the exact authenticated wire contract', async () => {
  await serverTest(reply(valid()), async (endpoint, requests) => {
    const result = await run(['evaluate', '--base-url', endpoint, '--model', 'jev-pinned'], { input: fixture });
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), valid());
    assert.equal(requests.length, 1);
    assert.deepEqual(requests[0], { method: 'POST', url: '/v1/systemone', auth: 'Bearer test-key', body: { ...fixture, model: 'jev-pinned' } });
    assert.ok(!result.stderr.includes('test-key'));
  });
});
test('models uses GET /v1/models', async () => {
  const body = { models: [{ name: 'jev-latest', description: 'Stable', release_date: '2026-09-16' }] };
  await serverTest(reply(body), async (endpoint, requests) => {
    const result = await run(['models', '--base-url', endpoint]);
    assert.equal(result.code, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), body);
    assert.equal(requests[0].method, 'GET'); assert.equal(requests[0].url, '/v1/models');
  });
});
test('malformed or inconsistent answers never reach stdout', async () => {
  const mutations = [
    x => delete x.answers.refund,
    x => x.answers.extra = { type: 'noul', noul: 1 },
    x => x.answers.route.choice = 'invented',
    x => x.answers.route.choice = 'support',
    x => delete x.answers.route.probabilities.support,
    x => x.answers.route.probabilities.support = 0.5,
    x => x.answers.route.confidence = 2,
    x => x.answers.refund.noul = -1,
    x => x.answers.refund.type = 'score',
    x => x.answers.urgency.score = 0.25,
    x => x.answers.urgency.legend['0'] = 'changed',
    x => delete x.usage
  ];
  for (const mutate of mutations) {
    const body = valid(); mutate(body);
    await serverTest(reply(body), async endpoint => {
      const result = await run(['evaluate', '--base-url', endpoint], { input: fixture });
      assert.equal(result.code, 4, result.stderr); assert.equal(result.stdout, '');
    });
  }
});
test('429/503/529 retries are bounded; authentication and redirects do not retry', async () => {
  for (const status of [429, 503, 529]) {
    await serverTest((_req, res, attempt) => {
      if (attempt < 3) { res.statusCode = status; res.setHeader('Retry-After', '0'); res.end('busy'); }
      else res.end(JSON.stringify(valid()));
    }, async (endpoint, requests) => {
      const result = await run(['evaluate', '--base-url', endpoint], { input: fixture });
      assert.equal(result.code, 0, result.stderr); assert.equal(requests.length, 3);
    });
  }
  for (const status of [401, 422, 302, 529]) {
    await serverTest((_req, res) => { res.statusCode = status; res.setHeader('Retry-After', '0'); res.setHeader('Location', '/other'); res.end('test-key sensitive body'); }, async (endpoint, requests) => {
      const result = await run(['evaluate', '--base-url', endpoint], { input: fixture });
      assert.equal(result.code, 3); assert.equal(requests.length, status === 529 ? 3 : 1);
      assert.equal(result.stdout, ''); assert.ok(!result.stderr.includes('test-key')); assert.ok(!result.stderr.includes('sensitive body'));
    });
  }
});
test('timeouts and oversized response are bounded', async () => {
  await serverTest(() => {}, async endpoint => {
    const result = await run(['evaluate', '--base-url', endpoint, '--timeout-ms', '100'], { input: fixture });
    assert.equal(result.code, 3, result.stderr); assert.equal(result.stdout, '');
  });
  await serverTest((_req, res) => res.end('x'.repeat(4 * 1024 * 1024 + 1)), async endpoint => {
    const result = await run(['evaluate', '--base-url', endpoint], { input: fixture });
    assert.equal(result.code, 4); assert.equal(result.stdout, '');
  });
});
test('endpoint restrictions and model precedence', async () => {
  for (const url of ['http://example.com', 'https://user:pass@example.com', 'https://example.com?key=secret', 'http://127.0.0.1.evil.test']) {
    const result = await run(['evaluate', '--base-url', url], { input: fixture });
    assert.equal(result.code, 2, result.stderr); assert.equal(result.stdout, '');
  }
  for (const [args, input, expected] of [
    [[], fixture, 'from-env'],
    [[], { ...fixture, model: 'from-body' }, 'from-body'],
    [['--model', 'from-flag'], { ...fixture, model: 'from-body' }, 'from-flag']
  ]) {
    const result = await run(['evaluate', '--dry-run', ...args], { input, env: { OCTOCODE_JEV_MODEL: 'from-env' } });
    assert.equal(JSON.parse(result.stdout).model, expected);
  }
});
test('standalone launcher config precedence, malformed config, project env opt-in', async () => {
  const temp = await mkdtemp(join(tmpdir(), 'octocode-jev-test-'));
  try {
    const isolated = join(temp, 'skill'); await cp(root, isolated, { recursive: true });
    const home = join(temp, 'home'), cwd = join(temp, 'project');
    await mkdir(home); await mkdir(join(cwd, '.octocode'), { recursive: true });
    await writeFile(join(home, '.octocoderc'), JSON.stringify({ env: { OCTOCODE_JEV_KEY: 'config-key' } }));
    await serverTest(reply(valid()), async (endpoint, requests) => {
      const call = (env = {}, flags = []) => run(['evaluate', '--base-url', endpoint, ...flags], { input: fixture, launcher: true, base: isolated, cwd, env: { OCTOCODE_HOME: home, OCTOCODE_JEV_KEY: '', ...env } });
      assert.equal((await call()).code, 0); assert.equal(requests.at(-1).auth, 'Bearer config-key');
      await writeFile(join(home, '.octocoderc'), JSON.stringify({ OCTOCODE_JEV_KEY: 'top-level-key', env: { OCTOCODE_JEV_KEY: '   ' } }));
      assert.equal((await call()).code, 0); assert.equal(requests.at(-1).auth, 'Bearer top-level-key');
      await writeFile(join(home, '.env'), 'OCTOCODE_JEV_KEY=global-key\n');
      await writeFile(join(cwd, '.octocode/.env'), 'OCTOCODE_JEV_KEY=project-key\n');
      assert.equal((await call()).code, 0); assert.equal(requests.at(-1).auth, 'Bearer global-key');
      assert.equal((await call({}, ['--project-env'])).code, 0); assert.equal(requests.at(-1).auth, 'Bearer project-key');
      assert.equal((await call({ OCTOCODE_JEV_KEY: 'process-key' }, ['--project-env'])).code, 0); assert.equal(requests.at(-1).auth, 'Bearer process-key');
      await writeFile(join(home, '.octocoderc'), '{ secret-malformed-key');
      const error = await call(); assert.equal(error.code, 2); assert.ok(!error.stderr.includes('secret-malformed-key'));
      assert.equal((await run(['--help'], { launcher: true, base: isolated, cwd, env: { OCTOCODE_HOME: home } })).code, 0);
    });
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test('retry hints cannot escape the total deadline; --retries 0 disables replay', async () => {
  for (const [header, value] of [
    ['Retry-After', '600'],
    ['Retry-After', new Date(Date.now() + 600_000).toUTCString()],
    ['retry-after-ms', '600000']
  ]) {
    await serverTest((_req, res) => { res.statusCode = 429; res.setHeader(header, value); res.end(); }, async (endpoint, requests) => {
      const result = await run(['evaluate', '--base-url', endpoint, '--timeout-ms', '100'], { input: fixture });
      assert.equal(result.code, 3); assert.equal(requests.length, 1); assert.match(result.stderr, /retry delay exceeds/);
    });
  }
  await serverTest((_req, res) => { res.statusCode = 529; res.end(); }, async (endpoint, requests) => {
    const result = await run(['evaluate', '--base-url', endpoint, '--retries', '0'], { input: fixture });
    assert.equal(result.code, 3); assert.equal(requests.length, 1);
  });
});

test('SDK structured entries and null state round-trip; non-JSON responses fail closed', async () => {
  const input = { state: null, questions: {
    q: { type: 'score', criteria: [null, { description: ['High'] }] },
    truth: { type: 'noul', instructions: ['Is it relevant?'], criteria: { true: null, false: { description: 'No' } } }
  } };
  const output = { model: 'jev-test', answers: {
    q: { type: 'score', score: 0.8, probabilities: { '0': 0.2, '1': 0.8 }, legend: { '0': null, '1': { description: ['High'] } }, confidence: 0.6 },
    truth: { type: 'noul', noul: 0.6 }
  }, usage: { input_tokens: 1, output_tokens: 0 } };
  await serverTest(reply(output), async endpoint => {
    const result = await run(['evaluate', '--base-url', endpoint], { input });
    assert.equal(result.code, 0, result.stderr); assert.deepEqual(JSON.parse(result.stdout), output);
  });
  await serverTest((_req, res) => res.end('not json'), async endpoint => {
    const result = await run(['evaluate', '--base-url', endpoint], { input: fixture });
    assert.equal(result.code, 4); assert.equal(result.stdout, '');
  });
});

test('shared Octocode network settings honor config, env files, process and CLI precedence', async () => {
  const temp = await mkdtemp(join(tmpdir(), 'octocode-jev-network-test-'));
  try {
    const home = join(temp, 'home'), cwd = join(temp, 'project');
    await mkdir(home); await mkdir(join(cwd, '.octocode'), { recursive: true });
    const config = {
      network: { timeout: 5000, maxRetries: 1 },
      github: { apiUrl: 'https://api.github.com' },
      output: { format: 'yaml' }, storage: { mode: 'memory' },
      env: { OCTOCODE_JEV_KEY: 'config-key' }
    };
    await writeFile(join(home, '.octocoderc'), JSON.stringify(config));
    await serverTest((_req, res) => { res.statusCode = 503; res.setHeader('Retry-After', '0'); res.end(); }, async (endpoint, requests) => {
      async function check(extraEnv, flags, attempts) {
        const before = requests.length;
        const result = await run(['evaluate', '--base-url', endpoint, ...flags], { input: fixture, launcher: true, cwd, env: { OCTOCODE_HOME: home, ...extraEnv } });
        assert.equal(result.code, 3, result.stderr); assert.equal(requests.length - before, attempts);
      }
      await check({}, [], 2);
      await writeFile(join(home, '.env'), 'MAX_RETRIES=2\n');
      await check({}, [], 3);
      await writeFile(join(cwd, '.octocode/.env'), 'MAX_RETRIES=0\n');
      await check({}, ['--project-env'], 1);
      await check({ MAX_RETRIES: '1' }, ['--project-env'], 2);
      await check({ MAX_RETRIES: '0' }, ['--retries', '2'], 3);
    });
    await serverTest((_req, res) => { res.statusCode = 429; res.setHeader('Retry-After', '6'); res.end(); }, async endpoint => {
      const result = await run(['evaluate', '--base-url', endpoint], { input: fixture, launcher: true, cwd, env: { OCTOCODE_HOME: home } });
      assert.equal(result.code, 3); assert.match(result.stderr, /retry delay exceeds/);
    });
    config.network.maxRetries = 10;
    await writeFile(join(home, '.octocoderc'), JSON.stringify(config));
    const result = await run(['evaluate', '--dry-run', '--retries', '10'], { input: fixture, launcher: true, cwd, env: { OCTOCODE_HOME: home } });
    assert.equal(result.code, 0, result.stderr);
  } finally { await rm(temp, { recursive: true, force: true }); }
});
