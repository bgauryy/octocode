// Loopback GitHub, npm registry and classification provider: the providers/ mock over test-utils/fixtures/demo-repo, plus a
// bulk pull request and the profile extras below. A TLS registry proxy answers the public registries and OSV
// from fixtures, so `artifactSearch` on public ecosystems and `securityCheck` never leave the machine.
// Answers are small unless an extra says otherwise, so a stub flow shows a mechanism, not the cost of a real answer.
// Usage: STUB_TOKENS=<tokens> node stub.mjs <urlFile>. Writes {url, proxyUrl, caFile, countsUrl} as JSON. Stop it by its pid.
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { startMock } from '../providers/mock-server.mjs';
import { addBulkPullRequest, sha, DATE } from '../providers/mock-model.mjs';
import { startRegistryProxy } from '../providers/mock-registry-proxy.mjs';
import { recordedJson } from '../providers/recorded-answers.mjs';
import { DEMO_REPO } from '../providers/paths.mjs';

/** One deterministic minified line of `bytes` bytes, as a bundler would emit it. */
const minifiedLine = bytes => {
  const parts = [];
  let size = 0;
  for (let i = 0; size < bytes; i++) {
    const part = `function f${i}(a){return needle_call(a,${i})}var v${i}=f${i}(1);`;
    parts.push(part);
    size += part.length;
  }
  return parts.join('').slice(0, bytes);
};

/**
 * The profile extras, beside the mock's own model (nothing it reads changes):
 * - issue 7: 365 comments, the RR-D1 count, for `gh.item.issue.comments`;
 * - pull request 5: a branch adding one 1 MiB single-line bundle, for `gh.item.patch.long`;
 * - octo/demo-docs `big.txt`: 20,000 lines, for `gh.file.big`;
 * - main's first commit answers to `prof-base`, so `gh.item.compare` has commits and files between refs.
 */
function addProfileExtras(model) {
  addBulkPullRequest(model);
  const comments = Array.from({ length: 365 }, (_, i) => ({ id: 70000 + i, body: `Comment ${i + 1}: the prefix option should accept a template; this is discussion evidence for paging.`, date: DATE(i % 28) }));
  model.issues.push({ number: 7, title: 'Long prefix discussion', body: 'Many comments on the prefix option.', state: 'open', date: DATE(3), comments });
  const files = { 'dist/bundle.min.js': `${minifiedLine(1 << 20)}\n`, 'src/entry.ts': 'export { greet } from "./greeter";\n' };
  const parent = model.head;
  const message = 'Ship the minified bundle';
  const head = { oid: sha(`branch 5 ${message}`), message, date: DATE(5), get parent() { return parent.oid; }, added: Object.keys(files).toSorted(), tree: sha('branch tree 5'), files, branch: 'feature-5' };
  model.branchCommits.push(head);
  model.pulls.push({ number: 5, title: 'Ship the minified bundle', body: 'Adds dist/bundle.min.js.', state: 'open', merged: false, base: parent, head, date: DATE(5), reviews: [], reviewComments: [] });
  const docs = model.repos.find(repo => repo.name === 'demo-docs');
  docs.files['big.txt'] = `${Array.from({ length: 20000 }, (_, i) => `Line ${i + 1}: the greeter manual repeats this sentence so a read pages through a large document.`).join('\n')}\n`;
  model.commits[0].branch = 'prof-base';
  return model;
}

/**
 * OSV for `security.package`: every query of a package gets three pages of 40 advisories (120 in all), linked by
 * page_token, so the tool walks OSV's pagination. The answer reads the page token the proxy just recorded.
 */
function pagedOsv(registry) {
  const real = recordedJson('osv/advisory-ghsa', '/v1/vulns/');
  const vuln = id => ({ ...real, id, aliases: [], affected: [{ ...real.affected[0], package: { name: 'proof', ecosystem: 'npm' }, ranges: [{ type: 'SEMVER', events: [{ introduced: '0' }, { fixed: '1.2.3' }] }] }] });
  registry.osvAnswers.shift = () => {
    const page = Number(registry.osvBodies.at(-1)?.page_token ?? 0);
    const vulns = Array.from({ length: 40 }, (_, i) => vuln(`GHSA-prof-${String(page * 40 + i).padStart(4, '0')}`));
    return { body: { vulns, ...(page < 2 && { next_page_token: String(page + 1) }) } };
  };
  for (let i = 0; i < 120; i++) {
    const id = `GHSA-prof-${String(i).padStart(4, '0')}`;
    registry.osvRecords.set(id, { body: vuln(id) });
  }
}

const tmp = fs.mkdtempSync(path.join(process.env.PROF_TMP ?? os.tmpdir(), 'stub-'));
const mock = await startMock({
  fixtureDir: DEMO_REPO,
  tmpDir: tmp,
  token: process.env.STUB_TOKENS.split(','),
  configureModel: addProfileExtras,
});
const registry = await startRegistryProxy(tmp);
pagedOsv(registry);
// Monotonic request counts beside the logs (which are cleared): GET <countsUrl> answers {github, registry, osv, classification}.
const counts = { github: 0, registry: 0, osv: 0, classification: 0 };
const counted = (log, key) => { const push = log.push.bind(log); log.push = (...items) => { counts[key] += items.length; return push(...items); }; };
counted(mock.requests, 'github');
counted(registry.requests, 'registry');
counted(registry.osvBodies, 'osv');
counted(mock.classificationRequests, 'classification');
const countsServer = http.createServer((req, res) => { res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(counts)); });
await new Promise(resolve => countsServer.listen(0, '127.0.0.1', resolve));
fs.writeFileSync(process.argv[2], JSON.stringify({ url: mock.url, proxyUrl: registry.proxyUrl, caFile: registry.caFile, countsUrl: `http://127.0.0.1:${countsServer.address().port}/` }));
// Keep the request logs from growing through long loops.
setInterval(() => {
  for (const log of [mock.requests, mock.requestsFrom, mock.classificationRequests, mock.gitAuth, registry.requests, registry.osvBodies]) log.length = 0;
}, 5000).unref();
process.on('SIGTERM', () => {
  fs.rmSync(tmp, { recursive: true, force: true });
  process.exit(0);
});
await new Promise(() => {});
