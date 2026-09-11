# `@octocodeai/octocode-skill-installer`

Shared installation engine for skills bundled by Octocode packages.

It materializes each bundled skill into the durable canonical store at
`$OCTOCODE_HOME/skills/<name>`, then installs a directory symlink into each
selected agent platform. On Windows, symlink mode uses directory junctions.
`copy` is available when a host cannot follow links, and `auto` selects the
platform's declared compatibility mode.

The engine is deliberately package-independent: callers provide their bundled skill
sources and choose explicit global or project targets. It never links a platform
directory directly to an npm or `npx` package cache.

```ts
import { installBundledSkills } from '@octocodeai/octocode-skill-installer';

const result = installBundledSkills({
  skills: [{ name: 'my-skill', sourcePath: '/package/skills/my-skill' }],
  targets: [{ platform: 'codex', scope: 'project', projectDir: process.cwd() }],
  mode: 'symlink',
  dryRun: true,
});
```

The installer preserves existing content unless `force: true` is explicit. The structured
result distinguishes materialized, linked, copied, unchanged, conflicting, and
failed actions.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and safety invariants.
