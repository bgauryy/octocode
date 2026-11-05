// Loopback GitHub, npm registry and classification provider: tool-health's mock over its own fixture, plus a
// bulk pull request. Answers are small, so a stub flow shows a mechanism, not the cost of a real answer.
// Usage: STUB_TOKENS=<tokens> node stub.mjs <urlFile>. Stop it by its pid.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const e2e = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../packages/octocode-benchmark/local/e2e');
const load = file => import(pathToFileURL(path.join(e2e, 'harness/tool-health', file)).href);
const { startMock } = await load('mock-server.mjs');
const { addBulkPullRequest } = await load('mock-model.mjs');
const tmp = fs.mkdtempSync(path.join(process.env.PROF_TMP ?? os.tmpdir(), 'stub-'));
const mock = await startMock({
  fixtureDir: path.join(e2e, 'fixtures/tool-health'),
  tmpDir: tmp,
  token: process.env.STUB_TOKENS.split(','),
  configureModel: addBulkPullRequest,
});
fs.writeFileSync(process.argv[2], mock.url);
// Keep the mock's request logs from growing through long loops.
setInterval(() => {
  for (const log of [mock.requests, mock.requestsFrom, mock.classificationRequests, mock.gitAuth]) log.length = 0;
}, 5000).unref();
process.on('SIGTERM', () => {
  fs.rmSync(tmp, { recursive: true, force: true });
  process.exit(0);
});
await new Promise(() => {});
