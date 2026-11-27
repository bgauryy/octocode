import assert from 'node:assert/strict';
import { test } from 'node:test';
import { localScriptImports } from '../../../test-utils/bundled-imports.mjs';
import { definePluginPackageTests } from '../../../test-utils/plugin-package-tests.ts';
import { build, packageRoot, repoRoot } from '../scripts/build.ts';

definePluginPackageTests({
  packageRoot,
  repoRoot,
  build,
  manifest: '.claude-plugin/plugin.json',
  mcp: '.mcp.json',
  marketplace: '.claude-plugin/marketplace.json',
  archiveRequired: ['.claude-plugin/plugin.json', '.mcp.json'],
});

test('script import discovery separates dependencies from fixture source', () => {
  const source = `
    import './side-effect.mjs';
    import { value } from './static.mjs';
    export { value } from '../export.mjs';
    export * from './all.mjs';
    const lazy = import('./lazy.mjs');
    const legacy = require('./legacy.cjs');
    const fixture = "import { run } from './fixture';";
    const template = \`import x from './template'; \${import('./nested.mjs')}\`;
    // import './comment.mjs';
    import 'node:fs';
  `;
  assert.deepEqual(localScriptImports(source).sort(), [
    '../export.mjs',
    './all.mjs',
    './lazy.mjs',
    './legacy.cjs',
    './nested.mjs',
    './side-effect.mjs',
    './static.mjs',
  ]);
});
