import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { inlineRefs, moduleOf, schemaDiff, sizes } from './tool-inventory.mjs';
import { ROOT } from './lib/workspaces.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(HERE, 'tool-inventory.mjs');
const run = (args, cwd = ROOT) => spawnSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: 'utf8', maxBuffer: 2 ** 28 });

function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-inventory-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test('every contract tool resolves to a native module in the repo, whatever the working directory', () => {
  const ran = run(['--json'], HERE);
  assert.equal(ran.status, 0, ran.stderr);
  const { tools } = JSON.parse(ran.stdout);
  assert.ok(tools.length > 0);
  for (const { tool, moduleDir } of tools) {
    assert.ok(moduleDir, `${tool} has a module`);
    assert.ok(moduleDir.startsWith('packages/octocode-native/crates/'), `${tool}: ${moduleDir} is a crate path, not the working directory`);
    assert.ok(fs.existsSync(path.join(ROOT, moduleDir)), `${tool}: ${moduleDir} exists`);
  }
});

test('a module is its folder and .rs root, else the one crate whose description names the tool', t => {
  const crates = tempDir(t);
  const crate = (name, description, files = []) => {
    fs.mkdirSync(path.join(crates, name, 'src'), { recursive: true });
    fs.writeFileSync(path.join(crates, name, 'Cargo.toml'), `[package]\nname = "${name}"\ndescription = "${description}"\n`);
    for (const file of files) {
      fs.mkdirSync(path.dirname(path.join(crates, name, file)), { recursive: true });
      fs.writeFileSync(path.join(crates, name, file), '');
    }
  };
  crate('tool-local', 'Octocode local tools: localSearch', ['src/local_search.rs', 'src/local_search/a.rs']);
  crate('tool-graph', 'Octocode graphBuild and graphQuery');
  crate('tool-two', 'Octocode twinTool');
  crate('tool-twin', 'Octocode twinTool');
  assert.deepEqual(moduleOf(crates, 'localSearch').files.map(file => path.relative(crates, file)), ['tool-local/src/local_search', 'tool-local/src/local_search.rs']);
  assert.equal(path.relative(crates, moduleOf(crates, 'graphQuery').path), 'tool-graph/src');
  assert.equal(moduleOf(crates, 'twinTool'), null, 'two crates naming one tool decide nothing');
  assert.equal(moduleOf(crates, 'absentTool'), null);
});

/** A cli-presentation.json with one tool; `defs` lets a version renumber its $defs. */
const presentation = ({ description = 'Find text.', fieldDescription = 'Globs.', defaultPath = '.', defName = '0', help = 'help' } = {}) => ({
  instructions: { head: 'x' },
  tools: [{
    name: 'localSearch',
    help,
    mcp: {
      title: 'Local Search',
      description,
      annotations: { readOnlyHint: true },
      inputSchema: {
        type: 'object',
        properties: { queries: { type: 'array', items: { type: 'object', properties: { include: { description: fieldDescription, $ref: `#/$defs/${defName}` }, path: { type: 'string', default: defaultPath } } } } },
        $defs: { [defName]: { type: 'array', items: { type: 'string' } } },
      },
    },
  }],
});

function contractRoot(t, current) {
  const root = tempDir(t);
  fs.mkdirSync(path.join(root, 'packages/octocode-config/contract'), { recursive: true });
  fs.writeFileSync(path.join(root, 'packages/octocode-config/contract/cli-presentation.json'), JSON.stringify(current));
  const old = path.join(root, 'old.json');
  return { root, old };
}

test('--schema-diff ignores prose and renumbered $defs and reports a structural change by path', t => {
  const { root, old } = contractRoot(t, presentation({ description: 'Find or count text.', fieldDescription: 'Globs relative to path.', defName: '7' }));
  fs.writeFileSync(old, JSON.stringify(presentation()));
  const same = schemaDiff(root, old);
  assert.equal(same.kind, 'cli-presentation');
  assert.equal(same.structural, false);
  assert.deepEqual(same.prose.tools, ['localSearch']);

  fs.writeFileSync(path.join(root, 'packages/octocode-config/contract/cli-presentation.json'), JSON.stringify(presentation({ defaultPath: 'src' })));
  const changed = schemaDiff(root, old);
  assert.equal(changed.structural, true);
  assert.deepEqual(changed.changes.map(({ tool, path: at, kind, before, after }) => ({ tool, at, kind, before, after })), [
    { tool: 'localSearch', at: '/inputSchema/properties/queries/items/properties/path/default', kind: 'default', before: '.', after: 'src' },
  ]);
});

test('inlining keeps a cycle as a name-free marker', () => {
  const schema = { $ref: '#/$defs/rule', $defs: { rule: { type: 'object', properties: { not: { $ref: '#/$defs/rule' } } } } };
  assert.deepEqual(inlineRefs(schema, schema), { type: 'object', properties: { not: { $recursive: 1 } } });
});

test('--sizes counts UTF-8 prose bytes per tool against its family median', () => {
  const tool = (name, description) => ({ name, help: 'é', mcp: { name, description, inputSchema: { type: 'object', properties: { a: { description: 'ab' } } } } });
  const result = sizes(
    { tools: [tool('a', 'x'), tool('b', 'xxx'), tool('c', 'xxxxx'), tool('g', 'é')] },
    { tools: [{ name: 'a', family: 'local' }, { name: 'b', family: 'local' }, { name: 'c', family: 'local' }, { name: 'g', family: 'github' }] },
  );
  const byTool = Object.fromEntries(result.tools.map(row => [row.tool, row]));
  assert.equal(result.unit, 'UTF-8 bytes');
  assert.equal(byTool.b.proseBytes, byTool.b.descriptionBytes + byTool.b.schemaProseBytes);
  assert.equal(byTool.b.familyMedianProseBytes, byTool.b.proseBytes);
  assert.equal(byTool.c.ratioToFamilyMedian, Number((byTool.c.proseBytes / byTool.b.proseBytes).toFixed(3)));
  assert.deepEqual(byTool.a.siblings, ['b', 'c']);
  assert.equal(byTool.g.descriptionBytes, Buffer.byteLength('é'), 'bytes, not characters');
  assert.equal(byTool.g.cliHelpBytes, 2);
  assert.deepEqual(sizes({ tools: [tool('a', 'x'), tool('b', 'y')] }, { tools: [] }, ['b']).tools.map(row => row.tool), ['b']);
});

test('bad arguments exit 2; a file that is no contract exits 2', t => {
  const dir = tempDir(t);
  assert.equal(run(['--schema-diff']).status, 2);
  assert.equal(run(['--sizes', '--schema-diff', 'x.json']).status, 2);
  assert.equal(run(['--nope']).status, 2);
  fs.writeFileSync(path.join(dir, 'other.json'), '{"tools":[]}');
  const ran = run(['--schema-diff', path.join(dir, 'other.json')]);
  assert.equal(ran.status, 2);
  assert.equal(JSON.parse(ran.stdout).error.code, 'invalidContract');
});
