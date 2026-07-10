import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');
const SKILL_ROOT = resolve(PACKAGE_ROOT, 'skills/octocode-awareness');

function read(path: string): string {
  return readFileSync(path, 'utf8');
}

function markdownFiles(root: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = resolve(root, entry.name);
    if (entry.isDirectory()) files.push(...markdownFiles(path));
    else if (entry.isFile() && entry.name.endsWith('.md')) files.push(path);
  }
  return files;
}

describe('production guidance contract', () => {
  it('uses one standalone WORK term and lazy command/reference discovery', () => {
    const authored = [
      resolve(PACKAGE_ROOT, 'README.md'),
      resolve(PACKAGE_ROOT, 'AGENTS.md'),
      ...markdownFiles(resolve(PACKAGE_ROOT, 'docs')),
      ...markdownFiles(SKILL_ROOT),
    ];
    const terminologyFailures = authored
      .filter((path) => /quick work|quick independent work|taskless/i.test(read(path)))
      .map((path) => relative(REPO_ROOT, path));
    expect(terminologyFailures).toEqual([]);

    const cheatSheet = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md'));
    expect(cheatSheet).not.toContain('<cli> schema commands --compact');
    expect(cheatSheet).not.toContain('<cli> docs list --compact');
    expect(cheatSheet).toContain('only when');

    const agents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));
    expect(agents).not.toContain('$AWARENESS schema commands --compact');
    expect(read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'))).not.toContain('<command> --help --compact');
  });

  it('routes every skill reference explicitly and removes mutating compatibility setup', () => {
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    for (const entry of readdirSync(resolve(SKILL_ROOT, 'references'), { withFileTypes: true })) {
      if (!entry.isFile() || !entry.name.endsWith('.md')) continue;
      expect(skill, entry.name).toContain(`references/${entry.name}`);
    }
    expect(skill).not.toContain('scripts/install-hooks.mjs');

    expect(existsSync(resolve(SKILL_ROOT, 'scripts/install-hooks.mjs'))).toBe(false);
    expect(existsSync(resolve(SKILL_ROOT, 'scripts/package.json'))).toBe(false);
    const install = read(resolve(SKILL_ROOT, 'scripts/install.mjs'));
    expect(install).not.toMatch(/npm install|check-only|skip-deps|findNpm|installDependencies/);
  });

  it('makes Pi advisory-first and reserves exclusivity for sensitive files', () => {
    const skillsPrompt = read(resolve(REPO_ROOT, 'packages/octocode-pi-extension/src/prompts/sections/skills.md'));
    const memoryPrompt = read(resolve(REPO_ROOT, 'packages/octocode-pi-extension/src/prompts/sections/memory.md'));
    const piReadme = read(resolve(REPO_ROOT, 'packages/octocode-pi-extension/README.md'));
    const combined = `${skillsPrompt}\n${memoryPrompt}\n${piReadme}`;

    expect(combined).toMatch(/advisory (?:file )?(?:work|presence)/i);
    expect(combined).toMatch(/exclusive.{0,80}sensitive|sensitive.{0,80}exclusive/is);
    expect(combined).not.toMatch(/taskless lock|lock exact files|file_lock without a task for quick work/i);
    expect(piReadme).not.toContain('Before every Pi write/edit call, the awareness bridge claims a lock');
    expect(piReadme).not.toContain('Before edits (`file_lock` or CLI `lock acquire`)');
    expect(piReadme).not.toContain('Before edit:** Claims a file lock for each target path');

    const piMemoryTool = read(resolve(REPO_ROOT, 'packages/octocode-pi-extension/src/tools/memory.ts'));
    expect(piMemoryTool).not.toMatch(/memory_workspace_status|memory_file_lock|memory_notify/);
    expect(piMemoryTool).not.toMatch(/FILE_LOCK_KINDS|lock_type|SHARED/);
  });

  it('removes legacy notify and lock-kind inputs from the shared tool adapter', () => {
    const operations = read(resolve(PACKAGE_ROOT, 'src/tool-operations.ts'));
    const types = read(resolve(PACKAGE_ROOT, 'src/types.ts'));
    const intents = read(resolve(PACKAGE_ROOT, 'src/intents.ts'));

    expect(operations).not.toMatch(/\|\s*'notify'|case 'notify'/);
    expect(operations).not.toMatch(/request\['lock_type'\]|request\['lockType'\]/);
    expect(types).not.toContain('lockType?: LockType');
    expect(intents).not.toContain('params.lockType');
  });

  it('has no compatibility coercion or re-export shim in the v3 runtime', () => {
    const runtimeFiles = [
      resolve(PACKAGE_ROOT, 'bin/awareness.ts'),
      ...readdirSync(resolve(PACKAGE_ROOT, 'src'), { withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith('.ts'))
        .map((entry) => resolve(PACKAGE_ROOT, 'src', entry.name)),
    ];
    expect(runtimeFiles.some((path) => /compatCoerce|compat_coerce/.test(read(path)))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'src/stubs.ts'))).toBe(false);
  });

  it('publishes and verifies Awareness before the Pi extension', () => {
    const release = read(resolve(REPO_ROOT, 'release/RELEASE_GUIDE.md'));
    const awarenessOrder = release.indexOf('@octocodeai/octocode-awareness');
    const piOrder = release.indexOf('@octocodeai/pi-extension', awarenessOrder + 1);
    expect(awarenessOrder).toBeGreaterThanOrEqual(0);
    expect(piOrder).toBeGreaterThan(awarenessOrder);
    expect(release).toContain('yarn workspace @octocodeai/octocode-awareness pack:check');
    expect(release).toContain('npm publish packages/octocode-awareness --access public --provenance');
    expect(release).toMatch(/npm install[^\n]*@octocodeai\/octocode-awareness/);
  });

  it('documents serialized migration and separates flat work rows from grouped FilesUnderWork', () => {
    const db = read(resolve(PACKAGE_ROOT, 'docs/DB.md'));
    expect(db).toMatch(/complete migration.{0,100}BEGIN IMMEDIATE/is);
    expect(db).toMatch(/`work list\|show`.{0,100}flat/is);
    expect(db).toMatch(/FilesUnderWork.{0,100}group/i);
  });

  it('keeps user-facing setup and examples copy-pasteable', () => {
    const readme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const guide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const reflection = read(resolve(PACKAGE_ROOT, 'docs/REFLECTION.md'));
    const navigation = read(resolve(PACKAGE_ROOT, 'docs/MEMORY_NAVIGATION.md'));

    expect(readme).not.toContain('docs show full-flow');
    expect(read(resolve(PACKAGE_ROOT, 'bin/awareness.ts'))).not.toContain('docs show full-flow');

    for (const installDoc of [readme, guide]) {
      expect(installDoc).not.toContain('<package>');
      expect(installDoc).toContain('npm install --global @octocodeai/octocode-awareness');
      expect(installDoc).toContain('$(npm root --global)/@octocodeai/octocode-awareness');
      expect(installDoc).toMatch(/octocode-skills.{0,80}optional|optional.{0,80}octocode-skills/is);
    }

    expect(readme).not.toContain('Installed skill: `node scripts/awareness.mjs`');
    expect(guide).not.toContain('installed skill scripts/awareness.mjs');
    expect(reflection).toContain('--outcome worked');
    expect(reflection).not.toContain('--outcome worked|partial|failed');
    expect(navigation).toContain('`omitted_peer_count`');
  });

  it('documents evidence, truthful compact routing, and safe read/write boundaries', () => {
    const readme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const docsIndex = read(resolve(PACKAGE_ROOT, 'docs/README.md'));
    const references = read(resolve(PACKAGE_ROOT, 'docs/REFERENCES.md'));
    const navigation = read(resolve(PACKAGE_ROOT, 'docs/MEMORY_NAVIGATION.md'));
    const wiki = read(resolve(PACKAGE_ROOT, 'docs/WIKI.md'));
    const guide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const skillReadme = read(resolve(SKILL_ROOT, 'README.md'));

    expect(readme).toContain('docs/REFERENCES.md');
    expect(docsIndex).toContain('REFERENCES.md');
    expect(references).toMatch(/implemented invariant/i);
    expect(references).toMatch(/adjacent prior art/i);
    expect(references).toMatch(/follow-on hypothesis/i);
    expect(wiki).toContain('## Read And Write Map');
    expect(wiki).toMatch(/access metadata|expiry cleanup/i);
    expect(navigation).toMatch(/limit applies per lane/i);
    expect(navigation).toMatch(/minifies JSON/i);
    expect(guide).not.toMatch(/verify audit[^\n]*--all-pending/i);
    expect(skill).not.toMatch(/before start, read `references\/agent-cheatsheet\.md`/i);
    expect(skillReadme).not.toContain('`--name octocode-awareness`');
  });

  it('keeps the always-loaded skill lobby byte-bounded and finish work conditional', () => {
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const finish = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet-finish.md'));
    expect(Buffer.byteLength(skill, 'utf8')).toBeLessThanOrEqual(6 * 1024);
    expect(finish).toContain('Always');
    expect(finish).toContain('Only when');
    expect(finish).toContain('verify audit');
    expect(finish).not.toMatch(/query all[^\n]*repo inject/is);
  });
});
