import { build } from 'esbuild';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { emitCatalogTypes } from './catalog-types.ts';
import { emitCatalog } from './emit-catalog.ts';
import { emitDeclarations } from './emit-types.ts';
import { libraryEntries, root } from './entry-points.ts';
import { stageAndPublish, waitingMessage } from './publish-dist.ts';
import { errorMessage, isReply } from './script-support.ts';

// schema.sql, catalog.json, unicode16.json, the dashboard and the docs the runtime serves are inlined, so dist/ needs no assets/ directory.
const config = join(root, '../octocode-config/dist/index.js');
if (!existsSync(config)) throw Error('Build @octocodeai/config before the communication runtime');
// Every CLI call and hook is a fresh process that imports from dist/, and `yarn typecheck` reads dist/ declarations. So the
// whole build runs under dist/.publish.lock and writes into a staging directory inside dist/ (same filesystem);
// stageAndPublish then renames each file into place after the files it imports. A build that has to wait says for whom.
await stageAndPublish(join(root, 'dist'), async staging => {
  // catalog.json is generated from the Zod sources in src/catalog, before anything reads it.
  emitCatalog();
  // The validator compiles catalog patterns with JavaScript's `u` flag, so a pattern that does not compile fails the build here.
  const patterns: string[] = [];
  (function collect(value: unknown): void {
    if (Array.isArray(value)) value.forEach(collect);
    else if (isReply(value)) for (const [key, item] of Object.entries(value)) typeof item === 'string' && key === 'pattern' ? patterns.push(item) : collect(item);
  })(JSON.parse(readFileSync(join(root, 'assets/catalog.json'), 'utf8')));
  for (const pattern of patterns) {
    try { new RegExp(pattern, 'u'); } catch (error) { throw Error(`catalog.json pattern ${JSON.stringify(pattern)} does not compile as a JavaScript RegExp with the u flag: ${errorMessage(error)}`); }
  }
  // One splitting build: the runnable entry (communication.js), the library (runtime.js), the two package exports
  // (mcp.js, cli.js) and the host adapters (hosts/*.js) share their chunks, so the runtime is bundled once.
  await build({
    entryPoints: { communication: join(root, 'src/runtime/main.ts'), ...libraryEntries() },
    outdir: staging, chunkNames: 'chunk-[hash]', bundle: true, splitting: true, platform: 'node', format: 'esm', target: 'node24',
    banner: { js: "import { createRequire as __createRequire } from 'node:module'; const require = __createRequire(import.meta.url);" },
    loader: { '.sql': 'text', '.md': 'text', '.html': 'text', '.css': 'text' },
    alias: { 'octocode-mcp-cli': join(root, '../octocode-mcp-cli/src/index.ts') },
  });
  await emitCatalogTypes();
  emitDeclarations(staging);
}, { onWait: holder => console.error(waitingMessage(holder)) });
