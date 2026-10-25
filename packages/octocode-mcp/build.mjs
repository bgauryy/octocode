// Bundles the launcher (dist/index.js): resolves the native binary and starts
// its MCP server.
import * as esbuild from 'esbuild';
import { chmodSync, readFileSync } from 'node:fs';
import { rm } from 'node:fs/promises';
import {
  assertDeclaredRuntimeImports,
  packageExternals,
} from '../../skills-dev/octocode-dev/scripts/package-build.mjs';

const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));

await rm('dist', { recursive: true, force: true });
const result = await esbuild.build({
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'esm',
  minify: true,
  treeShaking: true,
  // Published runtime dependencies stay external; the binary itself comes from
  // the per-platform package npm installs (`optionalDependencies`).
  external: packageExternals(pkg),
  define: {
    __OCTOCODE_BUNDLED__: 'true',
    // The platform package version a skipped optional dependency is fetched at.
    __OCTOCODE_NATIVE_VERSION__: JSON.stringify(
      Object.values(pkg.optionalDependencies)[0]
    ),
  },
  // The bundled native scripts are CommonJS: give them `require`, resolved
  // from this package, so they find the installed per-platform package.
  banner: {
    js: "#!/usr/bin/env node\nimport { createRequire as __octocodeRequire } from 'node:module';\nconst require = __octocodeRequire(import.meta.url);",
  },
  metafile: true,
  logLevel: 'info',
});

assertDeclaredRuntimeImports({
  metafiles: [result.metafile],
  dependencies: pkg.dependencies,
  label: `${pkg.name} bundle`,
});
chmodSync('dist/index.js', 0o755);
console.log('✓ esbuild complete');
