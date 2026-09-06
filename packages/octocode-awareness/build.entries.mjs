/**
 * Entry-point declarations for @octocodeai/octocode-awareness.
 *
 * Imported by build.mjs (for esbuild) and by tests/build-config.test.ts
 * (contract tests that verify the build surface hasn't drifted).
 * Importing this file has NO side effects.
 */
export const coreEntryPoints = {
  index:                'src/index.ts',
  'octocode-awareness': 'bin/awareness.ts',
  'hook-runner':        'bin/hook-runner-entry.ts',
  'extract-hook-files': 'bin/extract-hook-files.ts',
  'schema-api':         'src/schema/cli.ts',
};

/** Standalone (non-split) bundles that land in the Agent Skill's scripts/. */
export const skillScriptEntries = [
  { entryPoints: ['bin/awareness.ts'],          outfileName: 'awareness.mjs' },
  { entryPoints: ['bin/hook-runner-entry.ts'],  outfileName: 'hook-runner.mjs' },
  { entryPoints: ['bin/extract-hook-files.ts'], outfileName: 'extract-hook-files.mjs' },
];
