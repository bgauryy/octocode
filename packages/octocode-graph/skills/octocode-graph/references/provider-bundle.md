Load when asking what a bundler shipped: attaching an esbuild, rollup, rolldown or webpack record, the `bundle` query, or external package bytes.

# Bundle evidence

## What a bundle ships

The source graph shows what the code imports; a bundle shows what shipped after tree-shaking, code splitting and packages the graph cannot see into. Attach the build's own record:

- esbuild: `esbuild src/worker.ts --bundle --metafile=dist/meta.json --outdir=dist` (or `metafile: true` in the API and write `result.metafile`).
- rolldown: `bundleAnalyzerPlugin()` from `rolldown/experimental` writes `dist/analyze-data.json`; format `rolldown-analyze`.
- rollup, or rolldown with bytes in output: add this plugin and attach `bundle.json` as `rollup-bundle`.

```js
import { writeFileSync } from 'node:fs';
export const dumpBundle = { name: 'dump-bundle', generateBundle(_, bundle) { writeFileSync('bundle.json', JSON.stringify(Object.values(bundle).map(o => o.type === 'asset'
  ? { type: 'asset', fileName: o.fileName, bytes: Buffer.byteLength(o.source) }
  : { type: 'chunk', fileName: o.fileName, isEntry: o.isEntry, isDynamicEntry: o.isDynamicEntry, facadeModuleId: o.facadeModuleId, imports: o.imports, dynamicImports: o.dynamicImports,
      bytes: Buffer.byteLength(o.code), modules: Object.fromEntries(Object.entries(o.modules).map(([id, m]) => [id, { renderedLength: m.renderedLength }])) }))); } };
```

Relative paths in a metafile start from the bundler's working directory: set the provider's `workingDirectory` (absolute, on the machine that built) when it is not the graph root. Inputs become root-relative paths, `node_modules/…` included; virtual modules keep their names (`\0` is written `\\0`). rollup and rolldown name outputs by file name, relative to their output directory.

| Query | Answer |
| --- | --- |
| `bundle` | Every output with its bytes, kind (`entry`, `dynamic-entry`, `chunk`, `asset`) and entry file, heaviest first. |
| `bundle <output>` | The output's bytes by source file, heaviest first, then by package (`pkg:<name>` from `node_modules`, or the graph's workspace package; `(virtual)`, `(outside root)`). `stats.overhead` is the output's bytes no input accounts for (bundler code, padding); `stats.loads` the chunks it loads; `stats.externals` what it leaves to the runtime. An output is its path or a unique suffix. |
| `bundle <output> --diff` | The output and every chunk it loads statically, against `reach <entry> --runtime`. `tree-shaken`: reached, read by the bundler, no byte shipped. `not shipped`: reached, named by no input of this build (never read, or a format that lists only rendered modules). `unexpected`: shipped but not reached: an edge the graph missed, a file outside the graph, a package only another package pulls in; `importedBy` is the bundler's importer. `package`: a file of a package the reached files import, which the graph does not see into. Reached files in chunks loaded by `import()` are no difference (`stats.dynamicFiles`). Dropped rows carry their source bytes, shipped rows their bytes in output. |

`trace build:<artifact> OUTPUTS BUNDLES` walks the same evidence: `OUTPUTS`, `BUNDLES`, `LOADS`, `INPUTS` (each input's own size), `BUNDLE_IMPORTS` (the bundler's module graph) and `EXTERNAL`.

A build also finds these files by itself, without `enrich`: a metafile, stats or bundle dump (`*meta*.json`, `*stats*.json`, `analyze-data.json`, `*bundle*.json`, recognized by shape) in `.`, `dist`, `build`, `out`, `output`, `.output`, `stats` or `reports` (and one level below) of the root or a chunk. Each external package then gets its bytes in the bundle; a package no bundle holds gets its installed closure: its entry as Node resolves it from the importing file, plus every file its static imports, `import()` and `require()` load across installed packages. `data.json` `externals[]` carries `bytes`, `bytesMethod` (`bundle` or `installed-closure`) and `bytesEvidence` (the file it came from, files counted, version, unresolved specifiers); a builtin, an uninstalled package or another ecosystem's package keeps `bytes: null` with `bytesEvidence.reason`. A closure counts what loading the package can read, before tree-shaking: on d3, json-schema-to-typescript and 3d-force-graph it was 1.07×, 0.89× and 1.55× their bytes in an unminified esbuild bundle.
