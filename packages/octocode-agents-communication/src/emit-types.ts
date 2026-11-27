import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative } from 'node:path';
import { emitCatalogTypes } from './catalog-types.ts';
import { libraryEntries, root } from './entry-points.ts';

/**
 * Declarations for every library entry: published `types`, and the types the tests see for the bundles they run.
 * tsconfig.types.json compiles src/entries and src/hosts into `<out>/types`, mirroring the source tree from the monorepo
 * packages directory because the CLI library's sources are part of the program; each entry bundle gets a stub declaration
 * beside it (`<out>/<name>.d.ts`). The build passes its staging directory, and publishDist renames the files into dist/.
 */
export function emitDeclarations(out: string): void {
  const declarations = join(out, 'types');
  rmSync(declarations, { recursive: true, force: true });
  execFileSync(process.execPath, [createRequire(import.meta.url).resolve('typescript/bin/tsc'), '-p', join(root, 'tsconfig.types.json'), '--outDir', declarations], { cwd: root, stdio: 'inherit' });
  for (const [name, source] of Object.entries(libraryEntries())) {
    const stub = join(out, `${name}.d.ts`);
    mkdirSync(dirname(stub), { recursive: true });
    const emitted = join(declarations, basename(root), relative(root, source).replace(/\.ts$/, '.d.ts'));
    const path = relative(dirname(stub), emitted).replace(/\.d\.ts$/, '.js');
    const target = path.startsWith('.') ? path : `./${path}`;
    writeFileSync(stub, `export * from '${target}';\n${/^export default /m.test(readFileSync(emitted, 'utf8')) ? `export { default } from '${target}';\n` : ''}`);
  }
}

// `yarn types` regenerates the catalog types only; declarations come only from the build, which publishes them under the lock.
if (import.meta.main) await emitCatalogTypes(process.argv.includes('--check'));
