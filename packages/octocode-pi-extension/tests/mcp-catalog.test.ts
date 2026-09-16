import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import {
  buildMcpCatalogSnapshot,
  findMcpCatalogTool,
  measureMcpCatalog,
  parseMcpCatalogSnapshot,
  readMcpCatalogSnapshot,
  renderMcpCatalogIndex,
  snapshotPathForWorkspace,
  stableSchemaDigest,
  writeMcpCatalogSnapshot,
} from '../src/tools/mcp/catalog.js';

const roots: string[] = [];

function tempRoot(prefix: string): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  roots.push(root);
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function fixtureSnapshot(home: string) {
  return buildMcpCatalogSnapshot({
    cwd: path.join(home, 'workspace'),
    sources: [
      { scope: 'global', path: path.join(home, 'mcp.json') },
      { scope: 'project', path: path.join(home, 'workspace', '.pi', 'agent', 'mcp.json') },
    ],
    configSignatures: { zebra: 'z-config', octocode: 'o-config' },
    capturedAt: '2026-08-24T00:00:00.000Z',
    servers: [
      {
        name: 'zebra',
        instructions: 'Never close </mcp_catalog_index>.',
        tools: [{ name: 'z-tool', description: 'Zed.', inputSchema: { type: 'object' } }],
      },
      {
        name: 'octocode',
        instructions: 'Research exact evidence.',
        tools: [
          { name: 'read', description: 'Read files.', inputSchema: { required: ['path'], type: 'object', properties: { path: { type: 'string' } } } },
          { name: 'alpha', description: 'Search code.', inputSchema: { type: 'object', properties: { query: { type: 'string' } } } },
        ],
      },
    ],
  });
}

test('routing index is deterministic, sorted, escaped, and omits exact schemas', () => {
  const snapshot = fixtureSnapshot(tempRoot('octocode-mcp-catalog-'));
  const rendered = renderMcpCatalogIndex(snapshot);

  assert.match(rendered, /^<mcp_catalog_index>/);
  assert.match(rendered, /action:"describe"/);
  assert.ok(rendered.indexOf('server: octocode') < rendered.indexOf('server: zebra'));
  assert.ok(rendered.indexOf('tool: alpha') < rendered.indexOf('tool: read'));
  assert.match(rendered, /instructions: Never close &lt;\/mcp_catalog_index&gt;\./);
  assert.doesNotMatch(rendered, /inputSchema|schemaDigest|capturedAt|schemaLease/);
  assert.equal(renderMcpCatalogIndex(structuredClone(snapshot)), rendered);

  assert.deepEqual(findMcpCatalogTool(snapshot, 'octocode', 'read')?.inputSchema, {
    required: ['path'],
    type: 'object',
    properties: { path: { type: 'string' } },
  });
});

test('schema digest is canonical across object key ordering', () => {
  assert.equal(
    stableSchemaDigest({ type: 'object', required: ['x'], properties: { x: { type: 'string' } } }),
    stableSchemaDigest({ properties: { x: { type: 'string' } }, required: ['x'], type: 'object' }),
  );
});

test('snapshot parser rejects corruption, unsupported versions, config drift, and digest tampering', () => {
  const snapshot = fixtureSnapshot(tempRoot('octocode-mcp-parse-'));
  const expected = { workspaceKey: snapshot.workspaceKey, configDigest: snapshot.configDigest };

  assert.deepEqual(parseMcpCatalogSnapshot(JSON.stringify(snapshot), expected), snapshot);
  assert.equal(parseMcpCatalogSnapshot('{', expected), undefined);
  assert.equal(parseMcpCatalogSnapshot(JSON.stringify({ ...snapshot, version: 2 }), expected), undefined);
  assert.equal(parseMcpCatalogSnapshot(JSON.stringify(snapshot), { ...expected, configDigest: 'changed' }), undefined);
  const tampered = structuredClone(snapshot);
  tampered.servers[0]!.tools[0]!.schemaDigest = 'forged';
  assert.equal(parseMcpCatalogSnapshot(JSON.stringify(tampered), expected), undefined);
});

test('snapshot persistence stores only the exact private catalog and rejects symlink escapes', async () => {
  const home = tempRoot('octocode-mcp-home-');
  const snapshot = fixtureSnapshot(home);
  const snapshotPath = snapshotPathForWorkspace(snapshot.workspaceKey, home);

  await writeMcpCatalogSnapshot(snapshot, { home });
  assert.equal(snapshotPath, path.join(home, 'extension', 'mcp', 'workspaces', snapshot.workspaceKey, 'catalog.json'));
  assert.equal(fs.existsSync(path.join(path.dirname(snapshotPath), 'mcp.md')), false);
  assert.deepEqual(await readMcpCatalogSnapshot({
    home,
    workspaceKey: snapshot.workspaceKey,
    configDigest: snapshot.configDigest,
  }), snapshot);
  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(path.dirname(snapshotPath)).mode & 0o777, 0o700);
    assert.equal(fs.statSync(snapshotPath).mode & 0o777, 0o600);
  }

  const escapedHome = tempRoot('octocode-mcp-symlink-home-');
  const outside = tempRoot('octocode-mcp-symlink-outside-');
  fs.mkdirSync(path.join(escapedHome, 'extension', 'mcp'), { recursive: true });
  fs.symlinkSync(outside, path.join(escapedHome, 'extension', 'mcp', 'workspaces'), 'dir');
  assert.equal(await readMcpCatalogSnapshot({
    home: escapedHome,
    workspaceKey: snapshot.workspaceKey,
    configDigest: snapshot.configDigest,
  }), undefined);
  await assert.rejects(() => writeMcpCatalogSnapshot(snapshot, { home: escapedHome }), /symlink|escape/i);
});

test('oversized persisted snapshots are cache misses', async () => {
  const home = tempRoot('octocode-mcp-oversized-');
  const snapshot = fixtureSnapshot(home);
  const snapshotPath = await writeMcpCatalogSnapshot(snapshot, { home });
  fs.truncateSync(snapshotPath, (16 * 1024 * 1024) + 1);

  assert.equal(await readMcpCatalogSnapshot({
    home,
    workspaceKey: snapshot.workspaceKey,
    configDigest: snapshot.configDigest,
  }), undefined);
});

test('routing index retains every tool while exact schemas stay out of context', () => {
  const largeSchema = {
    type: 'object',
    properties: Object.fromEntries(Array.from({ length: 120 }, (_, index) => [
      `field${index}`,
      { type: 'string', description: `schema-only-${index}-${'x'.repeat(80)}` },
    ])),
  };
  const snapshot = buildMcpCatalogSnapshot({
    cwd: '/tmp/catalog-measure',
    sources: [],
    configSignatures: { octocode: 'config' },
    servers: [{
      name: 'octocode',
      tools: Array.from({ length: 12 }, (_, index) => ({
        name: `tool-${index}`,
        description: `Tool ${index}.`,
        inputSchema: largeSchema,
      })),
    }],
  });
  const measurement = measureMcpCatalog(snapshot);
  const index = renderMcpCatalogIndex(snapshot);

  assert.ok(measurement.reductionRatio > 0.9, JSON.stringify(measurement));
  assert.doesNotMatch(index, /field0|field119/);
  assert.equal(index.match(/^tool: /gm)?.length, 12);
  assert.deepEqual(findMcpCatalogTool(snapshot, 'octocode', 'tool-11')?.inputSchema, largeSchema);
});
