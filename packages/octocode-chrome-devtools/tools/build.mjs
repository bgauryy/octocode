import {
  chromeCommands,
  chromeGuideTopics,
  chromeOutputLimits,
  chromePlanOperations,
  chromeExtractionFields,
  chromePlanExamples,
  chromeInputExamples,
} from '@octocodeai/config/schema';
import { build } from 'esbuild';
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  readdirSync,
  rmSync,
} from 'node:fs';
import { dirname, join, relative } from 'node:path';
const root = join(import.meta.dirname, '..'),
  check = process.argv.includes('--check');
const config = join(root, '../octocode-config/dist/index.js');
if (!existsSync(config))
  throw Error('Build @octocodeai/config before the Chrome package');
const walk = dir =>
  readdirSync(dir, { withFileTypes: true }).flatMap(row =>
    row.isDirectory() ? walk(join(dir, row.name)) : [join(dir, row.name)]
  );
const engine = join(root, 'src/engine'),
  dist = join(root, 'dist');
const compiled = await build({
  entryPoints: walk(engine).filter(
    file => file.endsWith('.mts') && !file.endsWith('.d.mts')
  ),
  outbase: engine,
  outdir: join(dist, 'engine'),
  outExtension: { '.js': '.mjs' },
  write: false,
  bundle: false,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  legalComments: 'eof',
});
const runtime = await build({
  entryPoints: [join(root, 'src/adapter.ts')],
  outfile: join(dist, 'runtime.js'),
  write: false,
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node24',
  minify: true,
  legalComments: 'eof',
  alias: { 'octocode-mcp-cli': join(root, '../octocode-mcp-cli/src/index.ts') },
  banner: {
    js: "import { createRequire as bundleRequire } from 'node:module'; const require = bundleRequire(import.meta.url);",
  },
});
const files = new Map(
  [...compiled.outputFiles, ...runtime.outputFiles].map(file => [
    file.path,
    file.contents,
  ])
);
// Engines need the authored discovery/limit data, not the complete schema runtime.
files.set(
  join(dist, 'engine/chrome-contract.mjs'),
  Buffer.from(
    Object.entries({
      chromeCommands,
      chromeGuideTopics,
      chromeOutputLimits,
      chromePlanOperations,
      chromeExtractionFields,
      chromePlanExamples,
      chromeInputExamples,
    })
      .map(
        ([name, value]) => `export const ${name} = ${JSON.stringify(value)};`
      )
      .join('\n') + '\n'
  )
);
files.set(join(dist, 'engine/octocode-config.mjs'), readFileSync(config));
for (const name of ['cli', 'mcp'])
  files.set(
    join(dist, name + '.js'),
    Buffer.from(
      name === 'cli'
        ? "import { main } from './runtime.js';\nexport { spec } from './runtime.js';\nexport const runChromeCli = (args = process.argv.slice(2)) => main(args);\n"
        : "import { main } from './runtime.js';\nexport { spec } from './runtime.js';\nexport const runChromeMcp = (args = []) => main(['--mcp', ...args]);\n"
    )
  );
for (const file of walk(engine).filter(file => !file.endsWith('.mts')))
  files.set(join(dist, 'engine', relative(engine, file)), readFileSync(file));
for (const [file, bytes] of files) {
  if (check) {
    if (!existsSync(file) || !readFileSync(file).equals(bytes))
      throw Error('Stale ' + relative(root, file) + '; rebuild Chrome package');
  } else {
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, bytes);
  }
}
if (existsSync(dist))
  for (const file of walk(dist))
    if (!files.has(file)) {
      if (check) throw Error('Obsolete generated file ' + relative(root, file));
      rmSync(file);
    }
console.log(
  check
    ? 'Chrome build matches TypeScript source and shared config'
    : 'Chrome TypeScript runtime, CLI and MCP built'
);
