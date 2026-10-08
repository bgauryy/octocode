import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { connectStdio } from '../../dist/runtime.js';

test('npm archive runs CLI and MCP alone and serves complete package guidance', async () => {
  const root = resolve(import.meta.dirname, '../..'),
    work = mkdtempSync(join(tmpdir(), 'octo-chrome-pack-'));
  let client;
  try {
    const packed = spawnSync(
      'npm',
      ['pack', '--ignore-scripts', '--json', '--pack-destination', work],
      { cwd: root, encoding: 'utf8' }
    );
    assert.equal(packed.status, 0, packed.stderr);
    const [manifest] = JSON.parse(packed.stdout);
    for (const file of [
      'dist/runtime.js',
      'dist/cli.js',
      'dist/mcp.js',
      'bin/octocode-chrome-devtools.mjs',
      'dist/engine/octocode-config.mjs',
      'OPERATING.md',
      'docs/browser-execution.md',
      'docs/web-research.md',
    ])
      assert(
        manifest.files.some(row => row.path === file),
        file + ' missing'
      );
    assert(
      !manifest.files.some(
        row =>
          /^(src|node_modules|tools|tests)\//.test(row.path) ||
          row.path === 'SKILL.md'
      )
    );
    const extracted = join(work, 'extracted');
    mkdirSync(extracted);
    execFileSync('tar', [
      '-xzf',
      join(work, manifest.filename),
      '-C',
      extracted,
    ]);
    const pkg = join(extracted, 'package'),
      bin = join(pkg, 'bin/octocode-chrome-devtools.mjs');
    const help = spawnSync(
      process.execPath,
      [bin, '/cli', 'cdp', '--help', '--json'],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(help.status, 0, help.stderr);
    assert(JSON.parse(help.stdout).inputSchema.properties.method);
    const raw = spawnSync(process.execPath, [bin, '/raw', 'schema', 'run'], {
      cwd: work,
      encoding: 'utf8',
    });
    assert.equal(raw.status, 0, raw.stderr);
    assert(JSON.parse(raw.stdout).plan.operations.readStream);
    client = await connectStdio({
      command: process.execPath,
      args: [bin],
      cwd: work,
    });
    assert(
      (await client.listTools()).tools.some(tool => tool.name === 'skill')
    );
    for (const [topic, file] of [
      ['operating', 'OPERATING.md'],
      ['web-research', 'docs/web-research.md'],
    ]) {
      const first = (
        await client.callTool({
          name: 'skill',
          arguments: { topic, length: 500 },
        })
      ).structuredContent;
      assert(first.ok);
      let page = first.data,
        text = page.content;
      while (page.next) {
        const result = await client.callTool({
          name: page.next.tool,
          arguments: page.next.query,
        });
        assert(result.structuredContent.ok);
        page = result.structuredContent.data;
        text += page.content;
      }
      assert.equal(text, readFileSync(join(pkg, file), 'utf8'));
    }
    const guide = resolve(root, '../../skills/octocode-chrome-devtools');
    assert.deepEqual(readdirSync(guide).sort(), ['README.md', 'SKILL.md']);
  } finally {
    await client?.close();
    rmSync(work, { recursive: true, force: true });
  }
});
