# `@octocodeai/octocode-skill-installer`

Shared installation engine for skills bundled by Octocode packages.

It materializes each bundled skill into the durable canonical store at
`$OCTOCODE_HOME/skills/<name>`, then installs a directory symlink into each
selected agent platform. On Windows, symlink mode uses directory junctions.
`copy` remains an explicit portability fallback; `auto` selects links
for every supported host.

The engine is deliberately package-independent: callers provide their bundled skill
sources and choose explicit global or project targets. It never links a platform
directory directly to an npm or `npx` package cache.

```ts
import { installBundledSkills } from '@octocodeai/octocode-skill-installer';

const result = installBundledSkills({
  skills: [{ name: 'my-skill', sourcePath: '/package/skills/my-skill' }],
  targets: [{ platform: 'codex', scope: 'project', projectDir: process.cwd() }],
  mode: 'symlink',
  upgrade: true,
  dryRun: true,
});
```

`upgrade: true` refreshes changed bundled content in the installer-owned canonical
store. Existing links follow that update automatically. A copy-mode destination is
refreshed only when it still matches the previous canonical tree; arbitrary
destination drift remains a conflict. `force: true` is the separate, explicit
override for replacing such conflicts.

The structured result distinguishes installed, upgraded, linked, copied,
unchanged, conflicting, and failed actions. CLI consumers should render the shared
platform contract with `formatSkillPlatformHelp()` instead of duplicating platform
or alias strings.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and safety invariants.
