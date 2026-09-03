// Octocode env + config loader — the @octocodeai/config source (zero-dep).
// Repo-time: this re-export resolves via the workspace link (tests, src/index.ts).
// Build: scripts/build.mjs inlines the @octocodeai/config source AS dist/env.js, so the
// published extension carries the loader itself — @octocodeai/config is a build-time
// (dev) dependency only, never a runtime/published dependency.
//
// L8: Do NOT add any logic here. Any new config API belongs in @octocodeai/config.
// Contributors reading `import { propagateOctocodeEnv } from './env.js'` in index.ts
// should know they are importing from @octocodeai/config at dev time.
export * from '@octocodeai/config';
