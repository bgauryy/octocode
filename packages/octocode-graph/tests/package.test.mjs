import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { graphBuild, graphQuery, startMcp } from '@octocodeai/octocode-graph';
import { runGraphCli } from '@octocodeai/octocode-graph/cli';
import { startMcp as stdioServer } from '@octocodeai/octocode-graph/mcp';

test('the graph package exports both native tools, its stdio server and the bundled skill', () => {
  assert.equal(typeof graphBuild, 'function');
  assert.equal(typeof graphQuery, 'function');
  assert.equal(typeof startMcp, 'function');
  assert.equal(startMcp, stdioServer);
  assert.equal(typeof runGraphCli, 'function');
  const manifest = new URL(import.meta.resolve('@octocodeai/octocode-graph/skill'));
  assert.match(readFileSync(manifest, 'utf8'), /^name: octocode-graph$/m);
});
