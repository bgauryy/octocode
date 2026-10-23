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

/** Packs the package offline into work; returns the npm env and the pack manifest. */
function packArchive(root, work) {
  const npmEnv = {
    ...process.env,
    npm_config_cache: join(work, 'npm-cache'),
  };
  const packed = spawnSync(
    'npm',
    ['pack', '--ignore-scripts', '--json', '--pack-destination', work],
    { cwd: root, encoding: 'utf8', env: npmEnv, timeout: 30000 }
  );
  assert.equal(packed.status, 0, packed.stderr);
  const [manifest] = JSON.parse(packed.stdout);
  return { npmEnv, manifest };
}

/** Every file under directory, relative and sorted. */
const entries = directory =>
  readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile())
    .map(entry =>
      join(entry.parentPath, entry.name).slice(directory.length + 1)
    )
    .sort();

/** The bundled skill is a byte-identical copy of the canonical guide. */
function assertBundledSkill(bundledGuide, guide) {
  assert.deepEqual(entries(bundledGuide), entries(guide));
  for (const file of entries(guide))
    assert.deepEqual(
      readFileSync(join(bundledGuide, file)),
      readFileSync(join(guide, file)),
      `bundled skill differs: ${file}`
    );
}

/** Reads one skill topic through every MCP continuation page. */
async function readTopic(client, topic) {
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
  return text;
}

test('npm archive runs CLI and MCP alone and serves complete package guidance', async () => {
  const root = resolve(import.meta.dirname, '../..'),
    work = mkdtempSync(join(tmpdir(), 'octo-chrome-pack-'));
  let client;
  try {
    const { npmEnv, manifest } = packArchive(root, work);
    const npx = spawnSync(
      'npm',
      [
        'exec',
        '--yes',
        '--offline',
        '--package',
        join(work, manifest.filename),
        '--',
        'octocode-chrome-devtools',
        '/cli',
        'cdp',
        '--help',
        '--json',
      ],
      { cwd: work, encoding: 'utf8', env: npmEnv, timeout: 30000 }
    );
    assert.equal(npx.status, 0, npx.stderr);
    assert(JSON.parse(npx.stdout).inputSchema.properties.method);
    for (const file of [
      'dist/runtime.js',
      'dist/cli.js',
      'dist/mcp.js',
      'bin/octocode-chrome-devtools.mjs',
      'dist/engine/octocode-config.mjs',
      'OPERATING.md',
      'docs/browser-execution.md',
      'docs/web-research.md',
      'dist/skills/octocode-chrome-devtools/SKILL.md',
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
    const guide = resolve(root, '../../skills/octocode-chrome-devtools');
    const bundledGuide = join(pkg, 'dist/skills/octocode-chrome-devtools');
    assertBundledSkill(bundledGuide, guide);
    assert.equal(
      JSON.parse(readFileSync(join(pkg, 'package.json'), 'utf8')).exports[
        './skill'
      ],
      './dist/skills/octocode-chrome-devtools/SKILL.md'
    );
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
    ])
      assert.equal(
        await readTopic(client, topic),
        readFileSync(join(pkg, file), 'utf8')
      );
    assert.deepEqual(readdirSync(guide, { recursive: true }).sort(), [
      'README.md',
      'SKILL.md',
      'output.md',
      'references',
      join('references', 'configuration.md'),
      join('references', 'evidence.md'),
      join('references', 'octocode-usage.md'),
      join('references', 'presentation.md'),
      join('references', 'protocol-catalog.md'),
      join('references', 'report-mermaid.md'),
      join('references', 'report-output.md'),
      join('references', 'report-widgets.md'),
      'scripts',
      join('scripts', 'lib'),
      join('scripts', 'lib', 'entrypoint.mjs'),
      join('scripts', 'lib', 'report'),
      ...readdirSync(join(root, '../../skills/common/scripts/lib/report'))
        .sort()
        .map(name => join('scripts', 'lib', 'report', name)),
      join('scripts', 'render-report.mjs'),
    ]);
  } finally {
    await client?.close();
    rmSync(work, { recursive: true, force: true });
  }
});
