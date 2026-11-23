// Builds dist/: Node entry points and command scripts as one esbuild graph (shared chunks, one copy of
// every module), the viewer and offline-replay browser bundles, the renderer with its licenses, copied
// HTML/CSS/schema assets and the public declarations. `--check-generated` only checks the generated
// contract types.
import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { buildRenderer } from './renderer.mjs';
import { graphDeclarations } from './contract.mjs';
import {
  chmod,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const check = process.argv.includes('--check-generated');
// Command scripts: package.json bin entries and `octocode-graph <command>` (#graph-runtime/*).
const COMMANDS = [
  'build',
  'refresh',
  'diff',
  'query',
  'serve',
  'channel',
  'wiki',
  'reply',
  'render',
  'preflight',
  'link',
  'mcp',
];
const BINS = new Set(COMMANDS.map(name => `runtime/${name}.js`));
// Copied verbatim beside the bundles.
const STATIC = /\.(?:html|css|json)$/;

async function update(path, contents) {
  const current = await readFile(path).catch(error => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (current?.equals(contents)) return;
  if (check)
    throw new Error(`Stale generated file: ${path}. Run the package build.`);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
}

const contractPath = join(root, 'src/contract.generated.d.ts');
await update(contractPath, Buffer.from(await graphDeclarations()));
if (!check) {
  const dst = join(root, 'dist');
  const files = new Set();
  const emit = async file => {
    const name = relative(dst, file.path);
    files.add(name);
    await update(file.path, file.contents);
    if (BINS.has(name)) await chmod(file.path, 0o755);
  };
  const declarations = await mkdtemp(join(tmpdir(), 'octocode-graph-types-'));
  try {
    const compiler = fileURLToPath(
      new URL(
        'bin/tsgo',
        import.meta.resolve('@typescript/native-preview/package.json')
      )
    );
    execFileSync(
      process.execPath,
      [compiler, '-p', 'tsconfig.build.json', '--outDir', declarations],
      { cwd: root, stdio: 'inherit' }
    );
    const common = {
      absWorkingDir: root,
      bundle: true,
      format: 'esm',
      logLevel: 'silent',
      outdir: 'dist',
      write: false,
    };
    const node = build({
      ...common,
      platform: 'node',
      target: 'node24',
      entryPoints: {
        index: 'src/index.ts',
        cli: 'src/cli.ts',
        mcp: 'src/mcp.ts',
        analysis: 'src/analysis.ts',
        ...Object.fromEntries(
          COMMANDS.map(name => [
            `runtime/${name}`,
            `src/runtime/commands/${name}.ts`,
          ])
        ),
      },
      packages: 'external',
      splitting: true,
      chunkNames: 'shared/[name]-[hash]',
    });
    // The live viewer loads the renderer bundle beside it; the offline page maps the same specifier to
    // an embedded copy. The replay and its worker are embedded in exported HTML, so they are minified.
    const browser = { ...common, platform: 'browser', target: 'es2022' };
    const viewer = build({
      ...browser,
      entryPoints: { 'assets/live': 'src/assets/live.ts' },
      external: ['./renderer.generated.js'],
    });
    const offline = build({
      ...browser,
      minify: true,
      entryPoints: {
        'browser/offline': 'src/browser/offline.ts',
        'browser/worker': 'src/browser/worker.ts',
      },
      external: ['./renderer.generated.js'],
    });
    await buildRenderer(root, async (path, contents) =>
      emit({ path, contents })
    );
    for (const result of await Promise.all([node, viewer, offline]))
      for (const file of result.outputFiles) await emit(file);
    for (const dir of ['assets', 'scheme']) {
      const source = join(root, 'src', dir);
      for (const entry of await readdir(source, { withFileTypes: true })) {
        if (!entry.isFile() || !STATIC.test(entry.name)) continue;
        const name = join(dir, entry.name);
        files.add(name);
        await mkdir(join(dst, dir), { recursive: true });
        await cp(join(source, entry.name), join(dst, name));
      }
    }
    for (const entry of await readdir(declarations, {
      recursive: true,
      withFileTypes: true,
    })) {
      if (!entry.isFile() || !entry.name.endsWith('.d.ts')) continue;
      const name = relative(declarations, join(entry.parentPath, entry.name));
      files.add(name);
      await mkdir(dirname(join(dst, name)), { recursive: true });
      await cp(join(declarations, name), join(dst, name));
    }
    files.add('contract.generated.d.ts');
    await cp(contractPath, join(dst, 'contract.generated.d.ts'));
    for (const entry of await readdir(dst, {
      recursive: true,
      withFileTypes: true,
    })) {
      const path = join(entry.parentPath, entry.name);
      if (entry.isFile() && !files.has(relative(dst, path))) await rm(path);
    }
  } finally {
    await rm(declarations, { recursive: true, force: true });
  }
}
