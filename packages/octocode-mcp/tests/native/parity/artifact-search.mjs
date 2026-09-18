import assert from 'node:assert/strict';
import http from 'node:http';
import process from 'node:process';
import { Harness } from './harness.mjs';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer && addon && regexWorker,
  'usage: node artifact-search.mjs <reference> <native> <addon> <regex-worker>',
);

const packages = [
  { name: 'fixture-alpha', version: '1.0.0', description: 'first fixture' },
  { name: 'fixture-beta', version: '2.0.0', description: 'second fixture' },
];
const server = http.createServer((request, response) => {
  const url = new URL(request.url ?? '/', 'http://127.0.0.1');
  if (url.pathname !== '/-/v1/search') {
    response.writeHead(404).end('{}');
    return;
  }
  const offset = Number(url.searchParams.get('from') ?? 0);
  const size = Number(url.searchParams.get('size') ?? 10);
  response.setHeader('content-type', 'application/json');
  response.end(JSON.stringify({
    total: packages.length,
    objects: packages.slice(offset, offset + size).map(pkg => ({
      package: {
        ...pkg,
        license: 'MIT',
        links: { repository: `https://github.com/example/${pkg.name}` },
      },
    })),
  }));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const address = server.address();
assert.ok(address && typeof address === 'object');
const registry = `http://127.0.0.1:${address.port}`;

const harness = await Harness.connect({
  reference: referenceServer,
  native: nativeServer,
  addon,
  regexWorker,
});
try {
  await harness.testTool('artifactSearch', {
    queries: [{ type: 'npm', keywords: ['fixture'], pageSize: 2, registry }],
  });
  await harness.testTool('artifactSearch', { queries: [{}] });
  console.log(JSON.stringify({ tool: 'artifactSearch', cases: 2, status: 'covered' }));
} finally {
  await harness.close();
  await new Promise(resolve => server.close(resolve));
}
