// A build rooted inside a workspace whose packages it imports: those packages are externals, not stitched code. The
// build says which, with their import counts, and the command that builds the workspace root so they stitch.
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { workspaceExternals, workspaceNote } from '../src/runtime/workspace-scope.ts';
import { writeBrief } from '../src/runtime/render.ts';
import { computeMetrics } from '../src/runtime/metrics.ts';

function tree(files) {
  const root = tempDir('graph-workspace-');
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), text);
  }
  return root;
}
const ext = (id, count) => ({ id, count, files: Array.from({ length: count }, (_, i) => `src/f${i % 3}.ts`) });
const env = { PATH: '' };

test('npm workspaces: imported member packages outside the build root are named with their imports', () => {
  const ws = tree({
    'package.json': JSON.stringify({ name: 'mono', workspaces: ['packages/*'] }),
    'packages/contracts/package.json': JSON.stringify({ name: '@scope/contracts' }),
    'packages/ai/package.json': JSON.stringify({ name: '@scope/ai' }),
    'packages/coding-agent/package.json': JSON.stringify({ name: '@scope/coding-agent' }),
    'packages/coding-agent/src/main.ts': '',
  });
  const root = join(ws, 'packages/coding-agent/src');
  const found = workspaceExternals(root, { externals: [ext('pkg:@scope/contracts', 684), ext('pkg:react', 9), ext('pkg:@scope/ai/sub', 2)] }, { env });
  assert.equal(found.length, 1);
  const [w] = found;
  assert.equal(w.root, ws);
  assert.deepEqual(w.manifests, ['package.json workspaces']);
  assert.deepEqual(w.packages.map((p) => [p.name, p.paths, p.imports, p.files]), [
    ['@scope/contracts', ['packages/contracts'], 684, 3],
    ['@scope/ai/sub', ['packages/ai'], 2, 2],
  ]);
  assert.equal(w.command, `npx -y @octocodeai/octocode-graph build ${ws}`);
  const note = workspaceNote(w);
  assert.match(note, /2 packages of the npm workspace at .* were treated as externals, not stitched: @scope\/contracts \(684 imports\), @scope\/ai\/sub \(2 imports\)/);
  assert.ok(note.endsWith(`build the workspace root (a larger build): ${w.command}`), note);
});

test('no workspace, no imported member, or a build of the workspace root itself: nothing to say', () => {
  const ws = tree({
    'package.json': JSON.stringify({ workspaces: { packages: ['packages/*'] } }),
    'packages/a/package.json': JSON.stringify({ name: 'a' }),
    'packages/b/package.json': JSON.stringify({ name: 'b' }),
  });
  assert.deepEqual(workspaceExternals(join(ws, 'packages/a'), { externals: [ext('pkg:react', 3)] }, { env }), []);
  assert.deepEqual(workspaceExternals(ws, { externals: [ext('pkg:a', 3)] }, { env }), []);
  // A member inside the build root was built as a chunk, never an external worth a wider build.
  assert.deepEqual(workspaceExternals(join(ws, 'packages'), { externals: [ext('pkg:a', 3)] }, { env }), []);
  assert.equal(workspaceExternals(join(ws, 'packages/b'), { externals: [ext('pkg:a', 3)] }, { env })[0].packages[0].name, 'a');
});

test('pnpm, Cargo and go.work workspaces; Cargo names match with - and _; the build options carry over', () => {
  const pnpm = tree({
    'pnpm-workspace.yaml': "packages:\n  - 'apps/*'\n  - \"libs/**\"\n  - '!libs/skip'\ncatalog:\n  x: 1\n",
    'apps/web/package.json': JSON.stringify({ name: 'web' }),
    'libs/ui/core/package.json': JSON.stringify({ name: '@x/ui-core' }),
    'libs/skip/package.json': JSON.stringify({ name: 'skip' }),
  });
  const p = workspaceExternals(join(pnpm, 'apps/web'), { externals: [ext('pkg:@x/ui-core', 4), ext('pkg:skip', 1)] }, { env, lsp: 'off', exclude: ['gen'] });
  assert.deepEqual(p[0].manifests, ['pnpm-workspace.yaml']);
  assert.deepEqual(p[0].packages.map((x) => [x.name, x.paths]), [['@x/ui-core', ['libs/ui/core']]]);
  assert.equal(p[0].command, `npx -y @octocodeai/octocode-graph build ${pnpm} --lsp off --exclude gen`);

  const cargo = tree({
    'Cargo.toml': '[workspace]\nresolver = "2"\nmembers = [\n  "crates/*", # every crate\n  "tools/cli",\n]\nexclude = ["crates/old"]\n\n[workspace.dependencies]\nserde = "1"\n',
    'crates/io/Cargo.toml': '[package]\nname = "octo-io"\n',
    'crates/engine/Cargo.toml': '[package]\nname = "engine"\n[lib]\nname = "octo_engine"\n',
    'crates/old/Cargo.toml': '[package]\nname = "old"\n',
    'tools/cli/Cargo.toml': '[package]\nname = "cli"\n',
  });
  const c = workspaceExternals(join(cargo, 'tools/cli/src'), { externals: [ext('pkg:octo_io', 7), ext('pkg:octo_engine', 1), ext('pkg:old', 1), ext('pkg:serde', 3)] }, { env });
  assert.deepEqual(c[0].manifests, ['Cargo.toml [workspace]']);
  assert.deepEqual(c[0].packages.map((x) => [x.name, x.paths, x.imports]), [['octo_io', ['crates/io'], 7], ['octo_engine', ['crates/engine'], 1]]);

  const go = tree({
    'go.work': 'go 1.22\n\nuse (\n\t./svc\n\t./lib/kit // shared\n)\nuse ./tools\n',
    'svc/go.mod': 'module github.com/acme/mono/svc\n',
    'lib/kit/go.mod': 'module github.com/acme/kit\n',
    'tools/go.mod': 'module example.com/tools\n',
  });
  const g = workspaceExternals(join(go, 'svc'), { externals: [ext('pkg:github.com/acme/kit', 5), ext('pkg:example.com/tools/gen', 2), ext('pkg:github.com/other/x', 1)] }, { env });
  assert.deepEqual(g[0].manifests, ['go.work']);
  assert.deepEqual(g[0].packages.map((x) => [x.name, x.paths]), [['github.com/acme/kit', ['lib/kit']], ['example.com/tools/gen', ['tools']]]);
});

test('the nearest enclosing workspace with imported members is reported first; each names its own build', () => {
  const ws = tree({
    'package.json': JSON.stringify({ workspaces: ['packages/*'] }),
    'packages/js/package.json': JSON.stringify({ name: 'js-lib' }),
    'packages/native/package.json': JSON.stringify({ name: 'native' }),
    'packages/native/Cargo.toml': '[workspace]\nmembers = ["crates/*"]\n',
    'packages/native/crates/a/Cargo.toml': '[package]\nname = "a"\n',
    'packages/native/crates/b/Cargo.toml': '[package]\nname = "b"\n',
  });
  const found = workspaceExternals(join(ws, 'packages/native/crates/a'), { externals: [ext('pkg:b', 2), ext('pkg:js-lib', 1)] }, { env });
  assert.deepEqual(found.map((w) => [w.root, w.packages.map((x) => x.name)]), [[join(ws, 'packages/native'), ['b']], [ws, ['js-lib']]]);
});

test('imports the graph left unresolved out of the root count too: bare package names and relative paths into a member', () => {
  const ws = tree({
    'package.json': JSON.stringify({ workspaces: ['packages/*'] }),
    'packages/contracts/package.json': JSON.stringify({ name: '@demo/contracts' }),
    'packages/kit/package.json': JSON.stringify({ name: 'kit' }),
    'packages/app/package.json': JSON.stringify({ name: '@demo/app' }),
  });
  const unresolved = [
    { source: 'src/main.ts', specifier: '@demo/contracts' },
    { source: 'src/util.ts', specifier: '@demo/contracts/types' },
    { source: 'src/deep/x.ts', specifier: '../../../kit/src/index' },
    { source: 'src/main.ts', specifier: './missing' },
    { source: 'src/main.ts', specifier: null },
  ];
  const [w] = workspaceExternals(join(ws, 'packages/app'), { externals: [ext('pkg:@demo/contracts', 1)], unresolved }, { env });
  assert.deepEqual(w.packages.map((p) => [p.id, p.name, p.paths, p.imports, p.files]), [
    ['pkg:@demo/contracts', '@demo/contracts', ['packages/contracts'], 3, 3],
    [undefined, 'kit', ['packages/kit'], 1, 1],
  ]);
});

test('brief.txt names the workspace packages treated as externals and the command that stitches them', () => {
  const out = tempDir('graph-workspace-brief-');
  const D = { section: '/ws/packages/app/src', files: [{ id: 'a.ts', label: 'a.ts' }], imports: [], symbols: [], calls: [], coverage: { files: 1, imports: 0, calls: 0, isPartial: false, partialReasons: [] } };
  D.coverage.workspaceExternals = [{ root: '/ws', manifests: ['package.json workspaces'], packages: [{ id: 'pkg:@s/c', name: '@s/c', paths: ['packages/c'], imports: 684, files: 40 }], command: 'octocode-graph build /ws' }];
  computeMetrics(D);
  const { brief } = writeBrief(D, out);
  const text = readFileSync(brief, 'utf8');
  assert.match(text, /^SCOPE 1 package of the npm workspace at \/ws was treated as an external, not stitched: @s\/c \(684 imports\)/m);
  assert.match(text, /build the workspace root \(a larger build\): octocode-graph build \/ws$/m);
});

test('a build of a directory inside a member names imports into the rest of that package and the build of its root, not the workspace', () => {
  const ws = tree({
    'package.json': JSON.stringify({ workspaces: ['packages/*'] }),
    'packages/app/package.json': JSON.stringify({ name: '@demo/app' }),
    'packages/kit/package.json': JSON.stringify({ name: 'kit' }),
  });
  const pkg = join(ws, 'packages/app');
  const unresolved = [
    { source: 'a.ts', specifier: '../ids' },
    { source: 'b.ts', specifier: '@demo/app/util' },
    { source: 'a.ts', specifier: 'kit' },
  ];
  const found = workspaceExternals(join(pkg, 'src/lib'), { unresolved }, { env });
  assert.deepEqual(found.map((w) => [w.root, w.scope, w.packages.map((p) => [p.name, p.paths, p.imports, p.files])]), [
    [pkg, 'package', [['@demo/app', ['packages/app'], 2, 2]]],
    [ws, undefined, [['kit', ['packages/kit'], 1, 1]]],
  ]);
  assert.equal(found[0].command, `npx -y @octocodeai/octocode-graph build ${pkg}`);
  const note = workspaceNote(found[0]);
  assert.match(note, /^the build root lies inside package @demo\/app at .*: 2 imports leave it into the rest of that package/);
  assert.ok(note.endsWith(`build the package root: ${found[0].command}`), note);
  // Only own-package imports: no workspace row, and never the workspace build.
  const own = workspaceExternals(join(pkg, 'src/lib'), { unresolved: unresolved.slice(0, 2) }, { env });
  assert.deepEqual(own.map((w) => w.root), [pkg]);
});
