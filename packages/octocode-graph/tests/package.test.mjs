import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
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

test('the installed cookbook contains only Markdown and the package owns saved search', () => {
  const folder = new URL('.', import.meta.resolve('@octocodeai/octocode-graph/skill'));
  const resources = readdirSync(folder, {recursive:true, withFileTypes:true}).filter(entry=>entry.isFile());
  assert.ok(resources.every(entry=>entry.name.endsWith('.md')), 'runtime files in cookbook');
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url)));
  // One command: every saved-graph operation is a subcommand of octocode-graph. npm reads a string `bin` as one
  // command named after the unscoped package.
  const bin = typeof pkg.bin === 'string' ? { [pkg.name.split('/').pop()]: pkg.bin } : pkg.bin;
  assert.deepEqual(Object.keys(bin), ['octocode-graph']);
});
