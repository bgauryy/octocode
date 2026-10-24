import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { contract, expand, golden, runtime } from './golden-support.ts';
import { tempWorkspace } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

const { catalog, paths, timing, unicode16, validation, workspace, VERSION, ValidationError } = runtime;
const repository = fileURLToPath(new URL('../../../', import.meta.url));

test('version is the package version', () => {
  assert.equal(VERSION, JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version);
});

// ---- unicode and lease keys -------------------------------------------------------------------------------------

const SAMPLES = [
  'README.MD', 'Straße.ts', 'STRAẞE.ts', 'STRASSE.ts', 'ﬁle', 'FILE', 'Ångström', 'Å', 'Å', 'ǅ', 'İstanbul', 'ΣΊΣΥΦΟΣ', 'ᾈ',
  'café', 'café', 'Ǆ', '한글', '각', '각', 'ｆｕｌｌｗｉｄｔｈ', '\u1e9b\u0323', 'a\u0323\u0307', 'a\u0307\u0323', '😀.TXT', '𐐀', 'ǰ', '\u0130', 'ÿ', '', '.', '..',
  'Makefile', 'tab\there', 'ß', 'ẞ', 'ꭰ', 'Ꭰ', 'ᏸ', 'Ω', 'Ω',
];

test('component folding equals the recorded reference output for samples and for every code point of the main planes', () => {
  const differing = golden.fold.samples.filter(([input, expected]: [string, string]) => unicode16.normalizePathComponent(input) !== expected);
  assert.deepEqual(differing, []);
  assert.deepEqual(golden.fold.samples.map(([input]: [string]) => input), SAMPLES);
  // The golden lists only the code points whose fold differs from themselves; every other point folds to itself.
  const changed = new Map(golden.fold.points);
  const wrong: number[] = [];
  for (let point = 0; point < 0x30000; point += 1) {
    if (point >= 0xd800 && point <= 0xdfff) continue;
    const character = String.fromCodePoint(point);
    if (unicode16.normalizePathComponent(character) !== (changed.get(point) ?? character)) wrong.push(point);
  }
  for (let point = 0xe0000; point < 0xe0200; point += 1) {
    const character = String.fromCodePoint(point);
    if (unicode16.normalizePathComponent(character) !== (changed.get(point) ?? character)) wrong.push(point);
  }
  assert.deepEqual(wrong, []);
});

test('lease keys, ancestors and overlap equal the recorded reference output', () => {
  const { inputs, keys, overlap } = golden.keys;
  assert.deepEqual(inputs.map((path: string) => [paths.leaseKey(path), paths.ancestors(paths.leaseKey(path))]), keys);
  const pairs: [string, string, string, string][] = [];
  for (const a of inputs) for (const b of inputs) for (const ak of ['file', 'tree']) for (const bk of ['file', 'tree']) pairs.push([a, ak, b, bk]);
  assert.equal(pairs.map(pair => (paths.overlap(pair[0], pair[1], pair[2], pair[3]) ? '1' : '0')).join(''), overlap);
});

test("the Pi extension's generated fold table is the same data and folds the same keys", () => {
  const source = readFileSync(join(repository, 'packages/octocode-pi-extension/src/team/casefold-table.generated.ts'), 'utf8');
  const table = source.match(/CASE_FOLDING = `([^`]*)`/);
  assert.ok(table, 'the Pi fold table declares CASE_FOLDING');
  const piTable = new Map(table[1].trim().split('\n').map(line => {
    const [from, to] = line.split(':');
    return [parseInt(from, 16), to.split(' ').map(point => parseInt(point, 16))];
  }));
  const frozen = JSON.parse(readFileSync(new URL('../assets/unicode16.json', import.meta.url), 'utf8')).casefold;
  assert.equal(piTable.size, Object.keys(frozen).length);
  for (const [point, folded] of piTable) assert.deepEqual(frozen[point], folded, point.toString(16));
  // Pi folds with the runtime's NFD; for assigned characters that equals the frozen decomposition tables.
  const piFold = (component: string) => {
    let folded = '';
    for (const point of component.normalize('NFD')) {
      const code = point.codePointAt(0);
      assert.ok(code !== undefined);
      folded += String.fromCodePoint(...(piTable.get(code) ?? [code]));
    }
    return folded.normalize('NFD');
  };
  for (const sample of SAMPLES.filter(sample => !/^[\u0000-\u007f]*$/.test(sample))) assert.equal(unicode16.normalizePathComponent(sample), piFold(sample), sample);
});

test('folding never uses the runtime case or normalization tables', () => {
  // U+0130 lowercases to i + dot above in ICU; the frozen table has no such mapping for the composed form.
  assert.equal(unicode16.normalizePathComponent('İ'), golden.foldDotted);
  assert.equal(unicode16.normalizePathComponent('ABC'), 'abc');
  assert.equal(unicode16.normalizePathComponent('\uD800'), '\uD800');
});

// ---- physical path resolution ------------------------------------------------------------------------------------

test('resolvePath follows symlinks like the recorded reference output', t => {
  const directory = tempWorkspace(t, 'paths-validation-catalog-', { real: true });
  mkdirSync(join(directory, 'real/sub'), { recursive: true });
  writeFileSync(join(directory, 'real/file.txt'), 'x');
  symlinkSync('real', join(directory, 'rel-link'));
  symlinkSync(join(directory, 'real/sub'), join(directory, 'abs-link'));
  symlinkSync('does/not/exist', join(directory, 'dangling'));
  symlinkSync('../real', join(directory, 'real/sub/up'));
  symlinkSync('loop-b', join(directory, 'loop-a'));
  symlinkSync('loop-a', join(directory, 'loop-b'));
  const actual = golden.resolvePath.cases.map((path: string) => {
    try { return [paths.resolvePath(directory, expand(directory, path))]; } catch (error) { assert.ok(error instanceof Error); return ['error', 'code' in error && error.code === 'ENOTDIR' ? 'NotADirectoryError' : error instanceof ValidationError ? 'ValueError' : error.name]; }
  });
  assert.deepEqual(contract(directory, actual), golden.resolvePath.expected);
});

test('realpath equals the recorded reference resolve, strict and not', t => {
  const directory = tempWorkspace(t, 'paths-validation-catalog-', { real: true });
  mkdirSync(join(directory, 'a/b'), { recursive: true });
  symlinkSync('a/b', join(directory, 'link'));
  assert.deepEqual(contract(directory, golden.realpath.cases.map(([path, strict]: [string, boolean]) => paths.realpath(directory + '/' + path, strict))), golden.realpath.expected);
  assert.throws(() => paths.realpath(directory + '/link/missing', true), { code: 'ENOENT' });
});

// ---- coordination scope --------------------------------------------------------------------------------------------

test('coordinationScope equals the recorded reference output for plain directories, repositories and worktrees', t => {
  const directory = tempWorkspace(t, 'paths-validation-catalog-', { real: true });
  const plain = join(directory, 'plain/deep');
  const main = join(directory, 'main');
  const worktree = join(directory, 'wt');
  const broken = join(directory, 'broken');
  mkdirSync(plain, { recursive: true });
  mkdirSync(join(main, '.git/objects'), { recursive: true });
  mkdirSync(join(main, '.git/worktrees/wt'), { recursive: true });
  mkdirSync(join(main, 'src/nested'), { recursive: true });
  writeFileSync(join(main, '.git/HEAD'), 'ref: refs/heads/main\n');
  writeFileSync(join(main, '.git/worktrees/wt/HEAD'), 'ref: refs/heads/wt\n');
  writeFileSync(join(main, '.git/worktrees/wt/commondir'), '../..\n');
  mkdirSync(worktree, { recursive: true });
  writeFileSync(join(worktree, '.git'), `gitdir: ${join(main, '.git/worktrees/wt')}\n`);
  mkdirSync(join(broken, '.git'), { recursive: true });
  const actual = golden.scope.workspaces.map((item: string) => {
    try { return [workspace.coordinationScope(expand(directory, item))]; } catch (error) { assert.ok(error instanceof Error); return ['error', error instanceof ValidationError ? 'ValueError' : error.name]; }
  });
  assert.deepEqual(contract(directory, actual), golden.scope.expected);
  assert.equal(actual[3][0], join(main, '.git'));
});

test('Git metadata files are bounded, regular and well formed', t => {
  const directory = tempWorkspace(t, 'paths-validation-catalog-', { real: true });
  const file = join(directory, '.git');
  writeFileSync(file, 'gitdir: ' + 'x'.repeat(5000));
  assert.throws(() => workspace.metadataPath(file, 'gitdir:'), /exceeds 4096 bytes/);
  writeFileSync(file, 'nonsense');
  assert.throws(() => workspace.metadataPath(file, 'gitdir:'), /Invalid Git workspace metadata/);
  writeFileSync(file, 'gitdir:   \n');
  assert.throws(() => workspace.metadataPath(file, 'gitdir:'), /Invalid Git workspace metadata/);
  assert.throws(() => workspace.metadataPath(directory), /regular file/);
});

// ---- timing ---------------------------------------------------------------------------------------------------------

test('heartbeat period honors its bounded override', () => {
  assert.equal(timing.heartbeatMs({}), 15000);
  assert.equal(timing.heartbeatMs({ [timing.HEARTBEAT_ENV]: ' 100 ' }), 100);
  for (const bad of ['49', '60001', 'abc', '1.5', '-5', '１００']) assert.throws(() => timing.heartbeatMs({ [timing.HEARTBEAT_ENV]: bad }), /must be an integer from 50 to 60000/, bad);
  assert.equal(timing.deadlineAfter(null), null);
  const deadline = timing.deadlineAfter(1000);
  assert.ok(deadline !== null && deadline > timing.monotonic());
  assert.equal(timing.running({ aborted: true }, null), false);
  assert.equal(timing.running({ aborted: false }, timing.monotonic() - 1), false);
  assert.equal(timing.running({ aborted: false }, null), true);
});

test('Heartbeat beats once per period unless forced', () => {
  const previous = process.env[timing.HEARTBEAT_ENV];
  process.env[timing.HEARTBEAT_ENV] = '60000';
  try {
    let beats = 0;
    const idle = new timing.Heartbeat(() => { beats += 1; });
    assert.equal(idle.tick(), false);
    assert.equal(idle.tick(true), true);
    const due = new timing.Heartbeat(() => { beats += 1; }, true);
    assert.equal(due.tick(), true);
    assert.equal(due.tick(), false);
    assert.equal(beats, 2);
  } finally {
    if (previous === undefined) delete process.env[timing.HEARTBEAT_ENV]; else process.env[timing.HEARTBEAT_ENV] = previous;
  }
});

// ---- validation --------------------------------------------------------------------------------------------------

test('validate equals the recorded reference validator, including error text', () => {
  const actual = golden.validate.map(({ schema, value }: JsonRecord) => {
    try { validation.validate(schema, value); return null; } catch (error) { assert.ok(error instanceof ValidationError); return error.message; }
  });
  assert.deepEqual(actual, golden.validate.map((item: JsonRecord) => item.error));
});

test('lengths count code points and integers are whole numbers', () => {
  assert.equal(runtime.text.codePointLength('a😀b'), 3);
  assert.equal(runtime.text.codePointLength('\ud83d'), 1);
  assert.doesNotThrow(() => validation.validate({ type: 'integer' }, 5.0));
  assert.throws(() => validation.validate({ type: 'integer' }, 5.5), /has invalid type/);
  assert.throws(() => validation.validate({ type: 'number' }, Infinity), /has invalid type/);
});

test('unsupported keywords and uncompilable patterns fail closed', () => {
  assert.throws(() => validation.checkSchema({ type: 'string', $comment: 'x', unevaluated: true }), /Unsupported catalog schema keyword: \$comment, unevaluated/);
  assert.throws(() => validation.checkSchema({ properties: { a: { patternProperties: {} } } }), /Unsupported catalog schema keyword: patternProperties/);
  assert.throws(() => validation.checkSchema({ pattern: '(?P<name>x)' }), /Unsupported catalog pattern/);
  assert.throws(() => validation.checkSchema('string'), /Invalid catalog schema/);
  assert.throws(() => validation.checkSchema({ properties: { a: { type: 'strng' } } }), /Unsupported catalog schema type: strng/);
  assert.throws(() => validation.checkSchema({ type: ['string', 'int'] }), /Unsupported catalog schema type: int/);
  assert.doesNotThrow(() => validation.checkSchema({ type: ['string', 'null'] }));
  assert.doesNotThrow(() => validation.checkSchema({ pattern: '^[\\p{L}]+$' }));
});

test('every catalog pattern compiles as a JavaScript RegExp with the u flag', () => {
  const found: string[] = [];
  (function collect(value: unknown) {
    if (Array.isArray(value)) value.forEach(collect);
    else if (value && typeof value === 'object') for (const [key, item] of Object.entries(value)) typeof item === 'string' && key === 'pattern' ? found.push(item) : collect(item);
  })(JSON.parse(readFileSync(new URL('../assets/catalog.json', import.meta.url), 'utf8')));
  assert.ok(found.length >= 3);
  for (const pattern of found) assert.doesNotThrow(() => new RegExp(pattern, 'u'), pattern);
});

// ---- catalog ---------------------------------------------------------------------------------------------------

test('tool selection, help and accessors derive from the one catalog, and the skill text is the packaged OPERATING.md', () => {
  const dump = catalog.catalog();
  const names = (tools: JsonRecord[]) => tools.map((tool: JsonRecord) => tool.name);
  const all = names(catalog.selectedTools());
  assert.deepEqual(all, names(dump.tools));
  const commands = new Set(dump.commands.map((command: JsonRecord) => command.name));
  assert.deepEqual(all.filter(name => !commands.has(name)), [], 'every tool is a command');
  // A custom selection returns exactly its tools, in catalog order whatever the selection order.
  assert.deepEqual(names(catalog.selectedTools('fetch,peers')), all.filter(name => name === 'peers' || name === 'fetch'));
  // Each profile help() names is a strict superset of the one before it, and every profile is a subset of all tools.
  const profiles = Object.keys(catalog.help().toolProfiles).map(profile => new Set(names(catalog.selectedTools(profile))));
  assert.deepEqual(Object.keys(catalog.help().toolProfiles), ['messaging', 'review', 'editing']);
  for (const [index, profile] of profiles.entries()) {
    assert.ok([...profile].every(name => all.includes(name)), `profile ${index} selects only catalog tools`);
    if (index) assert.ok(profile.size > profiles[index - 1].size && [...profiles[index - 1]].every(name => profile.has(name)), `profile ${index} extends the one before`);
  }
  assert.deepEqual(catalog.help().commands, dump.commands.map((command: JsonRecord) => command.name));
  for (const command of dump.commands) assert.deepEqual(catalog.definition(command.name), command, command.name);
  for (const type of dump.recordTypes) assert.deepEqual(catalog.recordType(type.type), type, type.type);
  assert.equal(dump.database.schemaSha256, runtime.database.schema().schemaSha256);
  // The skill text is the packaged OPERATING.md; a vendor-specific copy starts at its title, a worker without lock tools drops the lease section.
  const operating = readFileSync(new URL('../OPERATING.md', import.meta.url), 'utf8');
  assert.equal(catalog.skillInstructions(), operating);
  assert.equal(catalog.skillInstructions('x'), operating.slice(operating.indexOf('# ')));
  assert.ok(catalog.workerSkill('messaging').length < catalog.skillInstructions('worker').length);
  assert.equal(catalog.workerSkill('editing'), catalog.skillInstructions('worker'));
});

test('catalog accessors return copies and refuse unknown names', () => {
  catalog.catalog().commands.length = 0;
  assert.ok(catalog.catalog().commands.length > 10);
  assert.throws(() => catalog.definition('nope'), /Unknown command: nope; use --help/);
  assert.throws(() => catalog.recordType('nope'), /Unknown record type: nope/);
  assert.throws(() => catalog.selectedTools('peers,nope'), /Unknown selected tool: nope/);
  assert.throws(() => catalog.selectedTools('peers,peers'), /Duplicate selected tool/);
});

test('command validation equals the recorded reference output for accepted and rejected inputs', () => {
  const actual = golden.commands.map(({ name, input }: JsonRecord) => {
    try { catalog.command(name, input); return null; } catch (error) { assert.ok(error instanceof ValidationError, error instanceof Error ? error.stack : String(error)); return error.message; }
  });
  assert.deepEqual(actual, golden.commands.map((item: JsonRecord) => item.error));
});

test('text, ttl and stored lengths follow the catalog rules', () => {
  assert.equal(catalog.text({ a: 'x' }, 'a'), 'x');
  assert.throws(() => catalog.text({}, 'a'), /Missing string: a/);
  assert.throws(() => catalog.text({ a: ' \n' }, 'a'), /Invalid a: blank/);
  assert.throws(() => catalog.text({ reasoning: 'é'.repeat(257) }, 'reasoning'), /513|514 UTF-8 bytes exceeds 512/);
  assert.doesNotThrow(() => catalog.text({ prompt: 'x'.repeat(1 << 20) }, 'prompt'));
  assert.throws(() => catalog.storedLengths({ nested: [{ content: 'x'.repeat(1024 * 1024 + 1) }] }), /exceeds 1 MiB/);
  assert.equal(catalog.ttl({}, 5000), 5000);
  assert.equal(catalog.ttl({ ttlMs: 1000 }, 5000), 1000);
  for (const bad of [999, 86400001, 1.5, '5000', true]) assert.throws(() => catalog.ttl({ ttlMs: bad }, 5000), /Invalid TTL/, String(bad));
  assert.equal(catalog.kindOf({ body: 'x' }), 'note');
  assert.equal(catalog.kindOf({ task: 'x' }), 'task');
  assert.throws(() => catalog.kindOf({}), /Supply exactly one message kind/);
});

test('pages obey the item and byte budgets and continue exactly once', () => {
  const rows = Array.from({ length: 250 }, (_, id) => ({ id: id + 1, text: 'x'.repeat(300) }));
  const first = catalog.page(rows, false, 'inbox', { limit: 1 });
  assert.ok(first.items.length > 0 && first.items.length < 100);
  assert.deepEqual(first.next, { command: 'inbox', input: { limit: 1, after: first.items.at(-1)!.id } });
  assert.equal(catalog.page(rows.slice(0, 3), true, 'x', {}).next, undefined);
  const big = catalog.page([{ id: 1, text: 'x'.repeat(40000) }, { id: 2 }], true, 'x', {});
  assert.equal(big.items.length, 1);
  assert.equal(big.next!.input.after, '1');
  assert.equal(big.budget!.targetBytes, 16384);
  assert.deepEqual(first, golden.page);
});
