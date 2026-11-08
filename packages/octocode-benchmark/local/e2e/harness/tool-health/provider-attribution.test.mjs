import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from 'node:test';
import { providerCalls } from './advanced-registry.mjs';
import { createRun } from './run-tool.mjs';
import { startSurfaces, tokenFor, unattributed } from './surfaces.mjs';
import { FIXTURES } from '../paths.mjs';

const cwd = path.join(FIXTURES, 'tool-health');
const resolve = (ecosystem, extra = {}) => ({ queries: [{ operation: 'resolve', ecosystem, packageName: ecosystem === 'npm' ? 'tiny-health' : 'proof', ...extra }] });
const advisory = { queries: [{ operation: 'advisory', id: 'GHSA-happy' }] };
const firstData = result => result.json?.results?.[0]?.data;
const firstArtifact = result => firstData(result)?.artifacts?.[0] ?? {};
/** A request with no credential, as git's first smart-HTTP request is. */
const bare = (surfaces, route) => fetch(`${surfaces.ctx.mock.url}${route}`, { signal: AbortSignal.timeout(10_000) }).then(response => response.arrayBuffer());

/** artifactSearch's npm and PyPI calls beside securityCheck's, which steers PyPI in its own world. */
async function sideBySide(surfaces) {
  const suite = { surfaces };
  const artifacts = createRun(suite, { name: 'artifactSearch', spec: {}, cwd });
  const security = createRun(suite, { name: 'securityCheck', spec: {}, cwd });
  security.ctx.registry.failures.set('pypi.org', { status: 503, body: { error: 'Fixture outage for another tool' } });
  const mockBefore = surfaces.ctx.mock.requestsFrom.length;
  const [npm, pypi, osv] = await Promise.all([
    artifacts.call(resolve('npm', { registryUrl: surfaces.ctx.mock.url }), 'cli'),
    artifacts.call(resolve('pypi'), 'cli'),
    security.call(advisory, 'cli'),
  ]);
  return { artifacts, security, npm, pypi, osv, mock: surfaces.ctx.mock.requestsFrom.slice(mockBefore) };
}

// tool-health runs tools side by side over one mock and one registry proxy. A check
// that a call did no provider work, or that steers a registry, must see only its
// own tool's requests, whatever another tool asks at the same moment.
test('tools running at once each see and steer only their own provider requests', { timeout: 120000 }, async () => {
  const surfaces = await startSurfaces(cwd);
  try {
    const { artifacts, security, npm, pypi, osv, mock } = await sideBySide(surfaces);
    assert.equal(firstArtifact(npm).packageName, 'tiny-health', JSON.stringify(npm.json));
    assert.equal(firstArtifact(pypi).version, '1.2.3', 'a failure another tool injected never answers this one');
    assert.ok(firstData(osv), JSON.stringify(osv.json));
    const own = tokenFor('artifactSearch', 'cli');
    const registry = artifacts.ctx.registry.requestsFrom;
    assert.ok(registry.some(entry => entry.request.includes('//pypi.org/')), 'its registry request is in its world');
    assert.deepEqual(registry.filter(entry => entry.token !== own), [], 'no other tool\'s registry request is');
    assert.ok(security.ctx.registry.requestsFrom.some(entry => entry.request.includes('//api.osv.dev/')));
    const npmRequests = mock.filter(entry => entry.request.includes('tiny-health'));
    assert.ok(npmRequests.length > 0);
    assert.deepEqual(npmRequests.filter(entry => entry.token !== own), [], 'an npm registry request carries its caller\'s credential');
    assert.deepEqual(unattributed(surfaces.ctx), [], 'every request so far carries a call\'s credential');
  } finally { await surfaces.close(); }
});

test('another tool\'s provider work never moves a no-provider-work count, and unclaimed work fails the suite', { timeout: 120000 }, async () => {
  const surfaces = await startSurfaces(cwd);
  try {
    const artifacts = createRun({ surfaces }, { name: 'artifactSearch', spec: {}, cwd });
    const security = createRun({ surfaces }, { name: 'securityCheck', spec: {}, cwd });
    const before = providerCalls(artifacts.ctx, 'cli');
    await Promise.all([security.call(advisory, 'cli'), bare(surfaces, '/octo/demo.git/info/refs?service=git-upload-pack')]);
    assert.equal(providerCalls(artifacts.ctx, 'cli'), before, 'OSV traffic and a git challenge are not artifactSearch\'s work');
    assert.deepEqual(unattributed(surfaces.ctx), [], 'a git challenge is the one request that carries no credential');
    await bare(surfaces, '/repos/octo/demo');
    assert.deepEqual(unattributed(surfaces.ctx), ['mock GET /repos/octo/demo'], 'any other request without one is reported');
  } finally { await surfaces.close(); }
});
