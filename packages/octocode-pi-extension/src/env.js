// Octocode env + config loader — the @octocodeai/config source (zero-dep).
// Repo-time: this re-export resolves via the workspace link (tests, src/index.js).
// Build: scripts/build.mjs inlines the @octocodeai/config source AS dist/env.js, so the
// published extension carries the loader itself — @octocodeai/config is a build-time
// (dev) dependency only, never a runtime/published dependency.
export * from '@octocodeai/config';
