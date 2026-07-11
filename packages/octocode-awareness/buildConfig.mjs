// Pure, side-effect-free build configuration shared by build.mjs and its tests.
// Importing this module must NOT trigger a build — it only computes config.
import { builtinModules } from 'node:module';

export const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((name) => `node:${name}`),
];

// Awareness ships zero npm runtime dependencies (enforced by
// scripts/verify-package.mjs), so Node builtins are the only external.
export const external = nodeExternals;

export const sharedBuildOptions = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  external,
  sourcemap: false,
  treeShaking: true,
};

// One Awareness-owned output graph. Shared domain modules become chunks; the
// schema lane stays lazy from the main CLI and carries the bundled Zod runtime.
export const coreEntryPoints = {
  index: 'src/index.ts',
  'octocode-awareness': 'bin/awareness.ts',
  'hook-runner': 'bin/hook-runner-entry.ts',
  'extract-hook-files': 'bin/extract-hook-files.ts',
  schema: 'bin/schema.ts',
  'schema-api': 'src/schema/cli.ts',
};

// Standalone (non-split) bundles for the Agent Skill: it must remain runnable
// after being copied away from node_modules, so no shared chunks, tools-core,
// or octocode CLI source may participate in these. Each outputs one
// self-contained file, named for the generated-artifact banner build.mjs adds.
export const skillScriptEntries = [
  { entryPoints: ['bin/awareness.ts'], outfileName: 'awareness.mjs' },
  { entryPoints: ['bin/hook-runner-entry.ts'], outfileName: 'hook-runner.mjs' },
  { entryPoints: ['bin/extract-hook-files.ts'], outfileName: 'extract-hook-files.mjs' },
  { entryPoints: ['bin/schema.ts'], outfileName: 'schema.mjs' },
];
