import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { listModels } from '../src/lib.mjs';

const execFileAsync = promisify(execFile);
const cli = fileURLToPath(new URL('../src/cli.mjs', import.meta.url));

async function mockServer(t, status, payload, observe = () => {}) {
  const server = createServer((request, response) => {
    observe(request);
    response.writeHead(status, { 'content-type': 'application/json' });
    response.end(JSON.stringify(payload));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  return `http://127.0.0.1:${server.address().port}`;
}

test('lists models with the existing auth and endpoint policy without changing provider JSON', async t => {
  const provider = {
    models: [
      { name: 'jev-latest', description: 'Stable alias', release_date: '2026-09-15' },
      { name: 'jev-preview', description: 'Preview alias', release_date: '2026-09-15' },
    ],
    future_field: { source: 'provider' },
  };
  let requestDetails;
  const baseUrl = await mockServer(t, 200, provider, request => {
    requestDetails = {
      method: request.method,
      path: request.url,
      authorization: request.headers.authorization,
      contentType: request.headers['content-type'],
    };
  });

  const result = await listModels({ key: 'test-key', baseUrl });
  assert.equal(result.ok, true);
  assert.equal(result.status, 200);
  assert.deepEqual(result.response, provider);
  assert.deepEqual(requestDetails, {
    method: 'GET',
    path: '/v1/models',
    authorization: 'Bearer test-key',
    contentType: undefined,
  });

  const { stdout, stderr } = await execFileAsync(process.execPath, [cli, '--models', '--compact'], {
    env: {
      ...process.env,
      OCTOCODE_CLASSIFICATION_API: 'test-key',
      OCTOCODE_CLASSIFICATION_API_HOST: baseUrl,
    },
  });
  assert.equal(stderr, '');
  assert.deepEqual(JSON.parse(stdout), provider);
});

test('models mode preserves provider errors and rejects conflicting CLI arguments', async t => {
  const provider = { detail: 'rate limited', retry_after: 1 };
  const baseUrl = await mockServer(t, 429, provider);
  const result = await listModels({ key: 'test-key', baseUrl });
  assert.equal(result.ok, false);
  assert.equal(result.status, 429);
  assert.deepEqual(result.response, provider);

  await assert.rejects(
    execFileAsync(process.execPath, [cli, '--models', '--compact'], {
      env: {
        ...process.env,
        OCTOCODE_CLASSIFICATION_API: 'test-key',
        OCTOCODE_CLASSIFICATION_API_HOST: baseUrl,
      },
    }),
    error => {
      assert.equal(error.code, 1);
      assert.deepEqual(JSON.parse(error.stdout), provider);
      return true;
    },
  );

  await assert.rejects(
    execFileAsync(process.execPath, [cli, '--models', '--input', 'unused.json'], {
      env: { ...process.env, OCTOCODE_CLASSIFICATION_API: 'test-key' },
    }),
    error => error.code === 1 && error.stderr.includes('exactly one of --input or --models'),
  );
});
