// Contract entities: a tool registry in the MCP tool-definition shape declares names that code in any language and
// the docs spell. One entity joins a TypeScript literal and a Rust literal; a doc mention links to it; noise and
// homonyms do not; a name two registries declare is unresolved with both candidates.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { contractEntities } from '../src/runtime/links-contracts.ts';
import { docLinks } from '../src/runtime/links-docs.ts';

const write = (root, files) => {
  for (const [p, text] of Object.entries(files)) {
    mkdirSync(join(root, dirname(p)), { recursive: true });
    writeFileSync(join(root, p), text);
  }
};
const FILES = {
  'contract/tools.json': JSON.stringify({ tools: [{ name: 'lspSearch', inputSchema: {} }, { name: 'shared', inputSchema: {} }] }, null, 2),
  'other/tools.json': JSON.stringify({ tools: [{ name: 'shared', inputSchema: {} }] }, null, 2),
  'not-a-registry.json': JSON.stringify({ tools: [{ name: 'lspSearch' }] }),
  'app/run.ts': "export const run = () => readResponse('lspSearch', {});\nconst other = 'lspSearchX';\nconst s = \"shared\";\n",
  'core/src/id.rs': 'pub fn id() -> &\'static str {\n    "lspSearch"\n}\n',
  'README.md': '# Tools\n\nCall `lspSearch` first; `.env` holds no secrets here.\n',
};

test('a registry declares each tool at the line of its name; a list without inputSchema declares nothing', () => {
  const root = tempDir('graph-contracts-');
  write(root, FILES);
  const entities = contractEntities(root, Object.keys(FILES));
  assert.deepEqual(entities.map((e) => [e.id, e.line]), [
    ['contract/tools.json#lspSearch', 4],
    ['contract/tools.json#shared', 8],
    ['other/tools.json#shared', 4],
  ]);
  const D = { files: [], symbols: [], calls: [], imports: [] };
  const { links, unresolved } = docLinks(D, root, Object.keys(FILES), [], entities);
  assert.deepEqual(links.filter((l) => l.rule === 'md-contract').map((l) => [l.source, l.line, l.target]), [['README.md', 3, 'contract/tools.json#lspSearch']]);
  assert.equal(unresolved.filter((u) => u.text === '.env').length, 0, 'a name nothing declares is no reference');
});

test('quoted literals in code files link to the entity they spell, in every language; a homonym does not', async () => {
  const root = tempDir('graph-contract-links-');
  write(root, FILES);
  const script = join(root, 'fake-cli.mjs');
  // localSearch, as the engine answers it: each code file's lines holding one of the literal patterns.
  writeFileSync(script, `
    import { readFileSync, readdirSync, statSync } from 'node:fs';
    import { join, relative } from 'node:path';
    if (process.argv[2] === '__graph') process.argv.splice(2, 1);
    const tool = process.argv[2];
    if (tool === 'config') { console.log(JSON.stringify({ key:'WORKSPACE_ROOT', value:null })); process.exit(0); }
    const q = JSON.parse(process.argv.at(-1)).queries[0];
    const walk = (d) => readdirSync(d).flatMap((n) => statSync(join(d, n)).isDirectory() ? walk(join(d, n)) : [join(d, n)]);
    const files = walk(q.path).filter((f) => !f.endsWith('.mjs')).map((f) => ({ path: f, matches: readFileSync(f, 'utf8').split('\\n').flatMap((text, i) => q.matchString.some((p) => text.includes(p)) ? [{ line: i + 1, value: text }] : []) })).filter((f) => f.matches.length);
    console.log(JSON.stringify({ results: [{ index: 0, data: { files } }] }));
  `);
  const source = `
    import { contractEntities, contractLinks } from ${JSON.stringify(new URL('../src/runtime/links-contracts.ts', import.meta.url).href)};
    const root = ${JSON.stringify(root)};
    const entities = contractEntities(root, ${JSON.stringify(Object.keys(FILES))});
    console.log(JSON.stringify(await contractLinks(root, new Set(['app/run.ts', 'core/src/id.rs']), entities)));
  `;
  const { stdout } = await promisify(execFile)(process.execPath, ['--input-type=module', '-e', source], {
    encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${script}` },
  });
  const { links, unresolved } = JSON.parse(stdout);
  assert.deepEqual(links.map((l) => [l.source, l.line, l.target, l.rule]).sort(), [
    ['app/run.ts', 1, 'contract/tools.json#lspSearch', 'contract'],
    ['core/src/id.rs', 2, 'contract/tools.json#lspSearch', 'contract'],
  ]);
  assert.deepEqual(unresolved.map((u) => [u.source, u.line, u.reason, u.candidates]), [
    ['app/run.ts', 3, 'ambiguous', ['contract/tools.json#shared', 'other/tools.json#shared']],
  ]);
});

test('a registry declares each tool input field, following its $defs; a JSON Schema declares its code-shaped properties; the registry wins a shared name', () => {
  const root = tempDir('graph-contract-fields-');
  const files = {
    'contract/tools.json': JSON.stringify({
      $defs: { q: { type: 'object', properties: { matchString: { type: 'array' }, path: { type: 'string' } } } },
      tools: [{ name: 'localSearch', inputSchema: { type: 'object', properties: { queries: { items: { $ref: '#/$defs/q' } }, responseChars: { type: 'integer' } } } }],
    }, null, 2),
    'schemas/config.schema.json': JSON.stringify({ $schema: 'https://json-schema.org/draft/2020-12/schema', type: 'object', properties: { apiHost: { type: 'string' }, responseChars: { type: 'integer' }, mode: { type: 'string' } } }, null, 2),
    'README.md': '# Fields\n\nSet `matchString`, `responseChars` and `apiHost`.\n',
  };
  write(root, files);
  const entities = contractEntities(root, Object.keys(files));
  assert.deepEqual(entities.map((e) => e.id).sort(), [
    'contract/tools.json#field.matchString',
    'contract/tools.json#field.responseChars',
    'contract/tools.json#localSearch',
    'schemas/config.schema.json#apiHost',
    'schemas/config.schema.json#responseChars',
  ]);
  const D = { files: [], symbols: [], calls: [], imports: [] };
  const { links } = docLinks(D, root, Object.keys(files), [], entities);
  // `responseChars` is declared by the registry and a schema: the registry's field wins.
  assert.deepEqual(links.filter((l) => l.rule === 'md-contract').map((l) => l.target).sort(), [
    'contract/tools.json#field.matchString',
    'contract/tools.json#field.responseChars',
    'schemas/config.schema.json#apiHost',
  ]);
});
