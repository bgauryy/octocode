import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { buildRenderer } from './renderer.mjs';
import { graphDeclarations } from './contract.mjs';
import {
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
const options = {
  absWorkingDir: root,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  logLevel: 'silent',
};
const check = process.argv.includes('--check-generated');
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
    const built = await build({
      ...options,
      entryPoints: [
        'src/index.ts',
        'src/cli.ts',
        'src/mcp.ts',
        'src/analysis.ts',
        'src/algorithms.ts',
        'src/graph-model.ts',
        'src/relations.ts',
      ],
      bundle: true,
      packages: 'external',
      splitting: true,
      outdir: 'dist',
      chunkNames: 'shared/[name]-[hash]',
      write: false,
    });

    const browser = await build({
      absWorkingDir: root,
      platform: 'browser',
      format: 'esm',
      target: 'es2022',
      bundle: true,
      minify: true,
      entryPoints: {
        offline: 'scripts/browser/offline.js',
        worker: 'scripts/browser/worker.js',
      },
      alias: Object.fromEntries(
        ['algorithms', 'graph-model', 'relations'].map(name => [
          `#graph-core/${name}`, `./src/${name}.ts`,
        ])
      ),
      external: ['./renderer.generated.js'],
      outdir: 'dist/browser',
      write: false,
    });
    const files = new Set();
    await buildRenderer(root, async (path, contents) => {
      files.add(relative(dst, path));
      await update(path, contents);
    });
    for (const dir of ['runtime', 'assets', 'scheme']) {
      const source = join(root, 'src', dir);
      for (const entry of await readdir(source, { recursive:true, withFileTypes:true })) {
        if (!entry.isFile()) continue;
        const path = join(entry.parentPath, entry.name);
        const name = join(dir, relative(source, path));
        files.add(name);
        await mkdir(dirname(join(dst, name)), { recursive:true });
        await cp(path, join(dst, name));
      }
    }
    for (const file of [...built.outputFiles, ...browser.outputFiles]) {
      files.add(relative(dst, file.path));
      await mkdir(dirname(file.path), { recursive: true });
      await update(file.path, file.contents);
    }
    for (const name of await readdir(declarations)) {
      if (!name.endsWith('.d.ts') || name === 'renderer.d.ts') continue; // Browser internals have no Node export.
      files.add(name);
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
