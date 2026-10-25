import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import { data, noul, PNG, same } from './classification-checks.mjs';
import { errorCodes } from './driver.mjs';
import { tokenFor, withoutReadinessPings } from './surfaces.mjs';

async function loopbackCanary() {
  let connections = 0;
  const server = net.createServer(socket => { connections += 1; socket.destroy(); });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    port: server.address().port,
    connections: () => connections,
    close: () => new Promise(resolve => server.close(resolve)),
  };
}

const sourceQuery = url => ({ operation: 'evaluate', source: { url }, questions: { evidence: noul('Is evidence available?') } });
const assetQuery = url => ({ operation: 'evaluate', content: { image: 'screen' }, assets: { screen: { source: { url } } }, questions: { visible: noul('Is screen visible?') } });

function deniedInputs(port) {
  const local = `127.0.0.1:${port}/capture`;
  return [
    { label: 'source.loopback', query: sourceQuery(`https://${local}`), expected: 'securityValidationFailed' },
    { label: 'source.localhost', query: sourceQuery(`https://localhost:${port}/capture`), expected: 'securityValidationFailed' },
    { label: 'image.loopback', query: assetQuery(`https://${local}`), expected: 'securityValidationFailed' },
    { label: 'source.http', query: sourceQuery(`http://${local}`), expected: 'invalidInput' },
    { label: 'image.http', query: assetQuery(`http://${local}`), expected: 'invalidInput' },
    { label: 'source.userinfo', query: sourceQuery(`https://user:canary-password@${local}`), expected: 'invalidInput' },
    { label: 'image.fragment', query: assetQuery(`https://${local}#crop`), expected: 'invalidInput' },
  ];
}

export async function urlPolicyChecks(ctx, surface, provider) {
  const canary = await loopbackCanary();
  const token = tokenFor('clasify', surface);
  const requests = () => withoutReadinessPings(ctx.mock.requestsFrom.filter(entry => entry.token === token)).length;
  const before = { http: requests(), inference: provider.sent.length };
  const checks = [];
  try {
    for (const { label, query, expected } of deniedInputs(canary.port)) {
      const response = await ctx.call(ctx.q(query), surface);
      const codes = errorCodes(response.json);
      checks.push([`urlPolicy.${label}`, same(codes, [expected]) && !data(response)?.answers
        && !JSON.stringify(response.json).includes('canary-password') && canary.connections() === 0
        && requests() === before.http && provider.sent.length === before.inference,
      { codes, connections: canary.connections(), http: requests() - before.http, inference: provider.sent.length - before.inference }]);
    }
  } finally { await canary.close(); }
  return checks;
}

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const blob = bytes => createHash('sha1').update(Buffer.from(`blob ${bytes.length}\0`)).update(bytes).digest('hex');

function imageFixture(ctx, surface) {
  const token = tokenFor('clasify', surface), file = `assets/revisions-${surface}.png`;
  const changed = Buffer.concat([PNG, Buffer.from([0])]);
  const revisions = new Map([['a'.repeat(40), PNG], ['b'.repeat(40), changed], ['c'.repeat(40), PNG]]);
  const requests = [];
  assert.equal(ctx.mock.model.githubResponders.has(token), false, 'GitHub fixture credential is exclusively owned');
  ctx.mock.model.githubResponders.set(token, request => {
    if (request.p !== `/repos/octo/demo/contents/${file}`) return null;
    const ref = request.url.searchParams.get('ref'), bytes = revisions.get(ref);
    requests.push({ method: request.method, ref });
    if (!bytes) return { status: 404, body: { message: 'Unknown image revision' } };
    return { body: { type: 'file', encoding: 'base64', path: file, size: bytes.length, sha: blob(bytes), content: bytes.toString('base64') } };
  });
  const query = ref => ({
    operation: 'evaluate', content: { image: 'screen', evidence: 'GitHub snapshot edge fixture' },
    assets: { screen: { source: { tool: 'ghGetFileContent', query: { owner: 'octo', repo: 'demo', path: file, ref } } } },
    questions: { visible: noul('Is screen available as explicit remote evidence?') },
  });
  return { revisions, requests, query, close: () => ctx.mock.model.githubResponders.delete(token) };
}

function capturedImage(result, bytes, ref) {
  const asset = result?.assets?.screen;
  assert.ok(asset, JSON.stringify(result));
  return result.answers.visible.noul === 0.9 && asset.source.query.ref === ref
    && asset.snapshot === digest(bytes) && asset.bytes === bytes.length && asset.width === 280 && asset.height === 280
    && fs.existsSync(asset.path) && fs.readFileSync(asset.path).equals(bytes)
    && !JSON.stringify(result).includes(bytes.toString('base64'));
}

function imageEffects(fixture, packets, entry) {
  const { ref, bytes, result, ordinal } = entry;
  const expected = ref[0] === 'a' ? 1 : 2;
  return packets.length === expected && fixture.requests.length === ordinal
    && same(fixture.requests.at(-1), { method: 'GET', ref })
    && (ref[0] === 'c' ? result.replayed === true : packets.at(-1)?.request.assets?.screen?.data_base64 === bytes.toString('base64'));
}

export async function githubImageChecks(ctx, surface, provider) {
  const fixture = imageFixture(ctx, surface), checks = [], saved = [];
  const before = provider.sent.length;
  try {
    for (const [ref, bytes] of fixture.revisions) {
      const result = data(await ctx.call(ctx.q(fixture.query(ref)), surface));
      saved.push(result);
      checks.push([`remoteImage.capture.${ref[0]}`, capturedImage(result, bytes, ref), result.assets]);
      const packets = provider.sent.slice(before);
      checks.push([`remoteImage.effects.${ref[0]}`, imageEffects(fixture, packets, { ref, bytes, result, ordinal: saved.length }),
      { acquisitions: fixture.requests.slice(), inference: packets.length, replayed: result.replayed }]);
    }
    const [first, changed, sameBytes] = saved;
    const ref = fixture.revisions.keys().next().value;
    const replay = data(await ctx.call(ctx.q(fixture.query(ref)), surface));
    checks.push(['remoteImage.originalReplay', capturedImage(replay, PNG, ref) && replay.replayed === true
      && same(replay.answers, first.answers) && replay.assets.screen.snapshot === first.assets.screen.snapshot
      && changed.assets.screen.snapshot !== first.assets.screen.snapshot && sameBytes.assets.screen.snapshot === first.assets.screen.snapshot
      && fs.readFileSync(first.assets.screen.path).equals(PNG) && fixture.requests.length === 3 && provider.sent.length === before + 2,
    { snapshots: saved.map(item => item.assets.screen.snapshot), acquisitions: fixture.requests.length, inference: provider.sent.length - before }]);
  } finally { fixture.close(); }
  return checks;
}
