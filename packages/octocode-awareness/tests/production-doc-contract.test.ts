import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');
const SKILL_ROOT = resolve(REPO_ROOT, 'skills/octocode-awareness');
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
  it('owns the complete Awareness lifecycle in one architecture document', () => {
    const lifecycle = read(resolve(PACKAGE_ROOT, 'docs/HOW_IT_WORKS.md'));
    const catalog = read(resolve(PACKAGE_ROOT, 'docs/README.md'));
    const rootAgents = read(resolve(REPO_ROOT, 'AGENTS.md'));
    expect(lifecycle).toMatch(/AGENTS\.md.*entry.*router[\s\S]*Agent Skills?.*policy[\s\S]*CLI.*control plane[\s\S]*hooks.*automation/i);
    expect(lifecycle).toContain('ENTER -> ACTIVATE -> ATTEND -> CHOOSE');
    expect(lifecycle).toContain('The Awareness CLI is the only agent-facing control plane for durable Awareness state.');
    expect(lifecycle).toMatch(/Plan[\s\S]*Task[\s\S]*Run[\s\S]*RunFile[\s\S]*Lock/);
    expect(lifecycle).toMatch(/successful write[\s\S]*failed write[\s\S]*PreCompact[\s\S]*SessionEnd/i);
    expect(catalog).toContain('complete bootstrap, operating, state, hook, memory, projection, and exit lifecycle');
    expect(rootAgents).toContain('docs/HOW_IT_WORKS.md');
  });
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
    const helpData = read(resolve(PACKAGE_ROOT, 'bin/cli-help-data.ts'));
    expect(helpData).toContain('AGENTS.md = trigger/router');
    expect(helpData).toContain('Agent Skill = operating policy');
    expect(helpData).toContain('CLI/SQLite = canonical live state');
    expect(helpData).toContain('hooks/Pi bridge = deterministic lifecycle automation');
  });
  it('routes every skill reference explicitly and removes mutating compatibility setup', () => {
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const referenceNames = readdirSync(resolve(SKILL_ROOT, 'references'), { withFileTypes: true })
      .filter((entry) => entry.isFile() && entry.name.endsWith('.md'))
      .map((entry) => entry.name);
    const direct = new Set(
      [...skill.matchAll(/references\/([a-z0-9-]+\.md)/g)].map((match) => match[1]!),
    );
    const reachable = new Set(direct);
    const queue = [...direct];
    while (queue.length > 0) {
      const current = queue.shift()!;
      const body = read(resolve(SKILL_ROOT, 'references', current));
      for (const candidate of referenceNames) {
        if (!reachable.has(candidate) && body.includes(candidate)) {
          reachable.add(candidate);
          queue.push(candidate);
        }
      }
    }
    expect([...reachable].sort()).toEqual(referenceNames.sort());
    expect(direct.size).toBeLessThanOrEqual(12);
    expect(skill).not.toContain('scripts/install-hooks.mjs');
    expect(existsSync(resolve(SKILL_ROOT, 'scripts/install-hooks.mjs'))).toBe(false);
    expect(existsSync(resolve(SKILL_ROOT, 'scripts/package.json'))).toBe(false);
    const install = read(resolve(SKILL_ROOT, 'scripts/install.mjs'));
    expect(install).not.toMatch(/npm install|check-only|skip-deps|findNpm|installDependencies|REQUIRED_BUNDLED_SKILLS[^\n]*octocode-skills/);
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

  it('has no compatibility coercion or re-export shim in the canonical v1 runtime', () => {
    const runtimeFiles = [
      resolve(PACKAGE_ROOT, 'bin/awareness.ts'),
      ...readdirSync(resolve(PACKAGE_ROOT, 'src'), { withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith('.ts'))
        .map((entry) => resolve(PACKAGE_ROOT, 'src', entry.name)),
    ];
    expect(runtimeFiles.some((path) => /compatCoerce|compat_coerce/.test(read(path)))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'src/stubs.ts'))).toBe(false);
  });

  it('ships one current database contract with only the exact receipt-table upgrade', () => {
    const databaseFiles = [
      'db.ts',
      'db-init.ts',
      'db-introspection.ts',
      'db-runtime.ts',
      'db-schema.ts',
      'db-search.ts',
    ];
    const databaseSource = databaseFiles
      .map((file) => read(resolve(PACKAGE_ROOT, 'src', file)))
      .join('\n');

    expect(databaseSource).toContain('AWARENESS_APPLICATION_ID = 0x4f435431');
    expect(databaseSource).not.toMatch(/\b(?:legacy|user_version|AWARENESS_SCHEMA_VERSION)\b/i);
    expect(databaseSource).toContain("'prior-hook-receipts'");
    expect(databaseSource).toContain('HOOK_RECEIPTS_DDL');
    expect(existsSync(resolve(PACKAGE_ROOT, 'src/db-legacy.ts'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'src/db-rebuild.ts'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'tests/legacy-migration.test.ts'))).toBe(false);
    const ownedArtifacts = [
      read(resolve(PACKAGE_ROOT, 'src/attend-model.ts')),
      read(resolve(PACKAGE_ROOT, 'src/attend-query.ts')),
      read(resolve(PACKAGE_ROOT, 'src/plans.ts')),
      read(resolve(PACKAGE_ROOT, 'src/repo-projection.ts')),
    ].join('\n');
    expect(ownedArtifacts).not.toContain('schema_version');
  });

  it('documents serialized initialization and separates flat work rows from grouped FilesUnderWork', () => {
    const db = read(resolve(PACKAGE_ROOT, 'docs/DB.md'));
    expect(db).toMatch(/Fresh initialization.{0,100}BEGIN IMMEDIATE/is);
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
      expect(installDoc).toMatch(/octocode-research|out\/skills\/octocode-awareness/is);
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

  it('keeps the generated wiki conditional, copy-runnable, and stale-safe', () => {
    const rootAgents = read(resolve(REPO_ROOT, 'AGENTS.md'));
    const generator = read(resolve(PACKAGE_ROOT, 'src/repo-context.ts'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const taskFlow = read(resolve(SKILL_ROOT, 'references/plan-task-workflow.md'));
    const repoFlow = read(resolve(SKILL_ROOT, 'references/repo-context-management.md'));

    expect(rootAgents).not.toMatch(/attend --compact`, then read `.octocode\/AGENTS\.md`/i);
    expect(generator).not.toContain('attend|work list|query|memory recall|workspace status');
    expect(generator).not.toContain('`repo inject` after important memories');
    expect(repoFlow).not.toMatch(/attend --compact`, then read `.octocode\/AGENTS\.md`/i);
    expect(skill).toContain('CLEAN? -> PROJECT?');
    expect(taskFlow).toContain('# run the acceptance check');
  });

  it('keeps README a bounded landing page instead of a second user guide', () => {
    const readme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const words = readme.trim().split(/\s+/).length;
    expect(words).toBeLessThanOrEqual(800);
    expect(readme).toContain('[docs/SKILLS.md](docs/SKILLS.md)');
    expect(readme).not.toContain('## Shared plan and tasks');
    expect(readme).not.toContain('## Hooks');
  });

  it('keeps agent entrypoints lean and assigns one owner to each workflow layer', () => {
    const rootAgents = read(resolve(REPO_ROOT, 'AGENTS.md'));
    const claude = read(resolve(REPO_ROOT, 'CLAUDE.md'));
    const packageAgents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));
    const architecture = read(resolve(PACKAGE_ROOT, 'docs/HOW_IT_WORKS.md'));
    const userGuide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const hooks = read(resolve(PACKAGE_ROOT, 'docs/HOOKS.md'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const awarenessSection = rootAgents.match(/## Awareness[\s\S]*?(?=\n## |$)/)?.[0] ?? '';

    expect(Buffer.byteLength(awarenessSection, 'utf8')).toBeLessThanOrEqual(1_400);
    expect(Buffer.byteLength(packageAgents, 'utf8')).toBeLessThanOrEqual(3_600);
    expect(Buffer.byteLength(claude, 'utf8')).toBeLessThanOrEqual(256);
    expect(claude).toContain('[AGENTS.md](./AGENTS.md)');
    expect(claude).toContain('.agents/skills');
    expect(claude).not.toContain('Skiils');

    expect(packageAgents).toMatch(/AGENTS.*routes.*skill.*policy.*CLI.*live state.*hooks.*automat/is);
    expect(packageAgents).toContain('follow `attend.next`');
    expect(packageAgents).not.toContain('## Lifecycle');
    expect(packageAgents).not.toContain('## Hooks');
    expect(packageAgents).not.toContain('Standalone WORK');
    expect(skill).toContain('## Lifecycle');
    expect(skill).toContain('## Feature map');
    expect(userGuide).toContain('## Operating Loop');
    expect(hooks).toContain('## Lifecycle');
    expect(architecture).toMatch(/AGENTS\.md \/ CLAUDE\.md[\s\S]*Agent Skill[\s\S]*CLI[\s\S]*hooks/i);
  });

  it('ships one bounded any-agent runbook for checking Awareness end to end', () => {
    const verification = read(resolve(PACKAGE_ROOT, 'docs/VERIFY.md'));
    const docsIndex = read(resolve(PACKAGE_ROOT, 'docs/README.md'));
    const packageReadme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const packageAgents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));

    expect(Buffer.byteLength(verification, 'utf8')).toBeLessThanOrEqual(9 * 1024);
    expect(verification.trim().split('\n').length).toBeLessThanOrEqual(180);
    expect(docsIndex).toContain('[VERIFY.md](VERIFY.md)');
    expect(packageReadme).toContain('[docs/VERIFY.md](docs/VERIFY.md)');
    expect(packageAgents).toContain('docs/VERIFY.md');

    expect(verification).toContain('## Quick Check');
    expect(verification).toContain('## Full Monorepo Check');
    expect(verification).toContain('maintenance self-test --compact');
    expect(verification).toContain('scripts/install.mjs');
    expect(verification).toContain('scripts/smoke-multi-agent.mjs');
    expect(verification).toContain('hooks check --host <claude|codex|cursor>');
    expect(verification).toMatch(/config.*runtime.*unverified/is);
    expect(verification).toContain('wirePiAwarenessHooks(pi)');
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness lint');
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness pack:check');
    expect(verification).not.toContain('skill-review.mjs');
    expect(verification).toMatch(/PASS[\s\S]*FAIL[\s\S]*BLOCKED/);
    expect(verification).toContain('## Receipt');
    expect(verification).toMatch(/Yarn's isolated packed artifact/i);
    expect(verification.indexOf('--dry-run')).toBeLessThan(verification.indexOf('hooks install --host <claude|codex|cursor>'));
  });

  it('keeps lifecycle recipes scoped, executable, and ordered around active presence', () => {
    const finish = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet-finish.md'));
    expect(finish).toMatch(/reflect record --agent-id "\$OCTOCODE_AGENT_ID" --workspace "\$PWD" --task/);
    expect(finish).toMatch(/memory archive --memory-id <id> --workspace "\$PWD" --dry-run/);
    expect(finish).toMatch(/maintenance digest --workspace "\$PWD" --dry-run/);
    expect(finish).toMatch(/query files --workspace "\$PWD"/);

    const collisionGuides = [
      read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md')),
      read(resolve(SKILL_ROOT, 'references/files-awareness.md')),
      read(resolve(SKILL_ROOT, 'references/lock-protocol.md')),
      read(resolve(PACKAGE_ROOT, 'docs/MEMORY_NAVIGATION.md')),
    ].join('\n');
    expect(collisionGuides).not.toContain('work show --file');
    expect(collisionGuides).toContain('work show --workspace "$PWD" --file');

    const hookGuides = [
      read(resolve(PACKAGE_ROOT, 'AGENTS.md')),
      read(resolve(PACKAGE_ROOT, 'docs/HOW_IT_WORKS.md')),
      read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md')),
      read(resolve(SKILL_ROOT, 'references/lock-protocol.md')),
    ].join('\n');
    expect(hookGuides).toMatch(/Post-edit[\s\S]*ACTIVE[\s\S]*(?:Stop|PreCompact|SessionEnd)[\s\S]*PENDING/i);
    expect(hookGuides).toMatch(/PreCompact[\s\S]{0,240}(?:does not end|keeps)[\s\S]{0,80}session/i);
    expect(hookGuides).toMatch(/SessionEnd[\s\S]{0,240}(?:ends|marks)[\s\S]{0,80}session/i);
    expect(hookGuides).not.toMatch(/post-edit[^\n]*(ends|becomes)[^\n]*PENDING/i);

    const taskFlow = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    expect(taskFlow.indexOf('# run acceptance checks while presence remains active'))
      .toBeLessThan(taskFlow.indexOf('octocode-awareness task submit'));
    expect(taskFlow.indexOf('octocode-awareness task submit'))
      .toBeLessThan(taskFlow.indexOf('octocode-awareness verify mark'));
  });

  it('makes fresh-agent install, activation, and hook ownership safe and executable', () => {
    const packageReadme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const userGuide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const skillReadme = read(resolve(SKILL_ROOT, 'README.md'));
    const skillLobby = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const tooling = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet-tooling.md'));
    const hooks = read(resolve(SKILL_ROOT, 'references/hooks.md'));
    const packageHooks = read(resolve(PACKAGE_ROOT, 'docs/HOOKS.md'));

    for (const guide of [packageReadme, userGuide, skillReadme, tooling]) {
      expect(guide).toContain('--dry-run');
      expect(guide.indexOf('--dry-run')).toBeLessThan(guide.indexOf('--force'));
      expect(guide).toMatch(/common[\s\S]{0,240}(?:claude|cursor|codex|pi)/i);
    }
    expect(tooling.indexOf('export OCTOCODE_AGENT_ID')).toBeLessThan(tooling.indexOf('attend --workspace'));
    expect(skillLobby).toContain('first activation');
    expect(skillLobby).toContain('scripts/install.mjs');
    for (const guide of [skillLobby, hooks, packageHooks]) {
      expect(guide).toMatch(/Claude[\s\S]{0,240}frontmatter/i);
      expect(guide).toMatch(/do not (?:also )?install|do not duplicate/i);
    }
    expect(packageHooks).not.toMatch(/Session end\/compact[\s\S]{0,120}close the session/i);
  });

  it('publishes a bounded, measurable Homeostatic Awareness thesis', () => {
    const readme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const docsIndex = read(resolve(PACKAGE_ROOT, 'docs/README.md'));
    const thesis = read(resolve(PACKAGE_ROOT, 'docs/THESIS.md'));
    const references = read(resolve(PACKAGE_ROOT, 'docs/REFERENCES.md'));
    const homeostatic = read(resolve(SKILL_ROOT, 'references/homeostatic-loop.md'));

    expect(readme).toContain('docs/THESIS.md');
    expect(docsIndex).toContain('THESIS.md');
    expect(thesis).toMatch(/human\/agent-in-the-loop software controller/i);
    expect(thesis).toContain('## Why Homeostasis');
    expect(thesis).toMatch(/dynamic regulation.*viable range/is);
    expect(thesis).toMatch(/not.*equilibrium/is);
    expect(thesis).toMatch(/SENSE[\s\S]*COMPARE[\s\S]*ACT[\s\S]*REMEASURE/);
    expect(thesis).toMatch(/living-system.*metaphor|metaphor.*living-system/is);
    expect(thesis).toMatch(/not sentience|not.*sentien/i);
    expect(thesis).toContain('Token pressure');
    expect(thesis).toMatch(/Sensor[\s\S]*Target[\s\S]*Actuator[\s\S]*Guard/);
    expect(thesis.trim().split(/\s+/).length).toBeLessThanOrEqual(1400);
    expect(references).toContain('## Homeostasis And Collective Memory');
    expect(references).toContain('**Homeostasis — adjacent prior art:**');
    expect(homeostatic).toContain('CHOOSE/DECLARE');
    expect(homeostatic).toContain('REMEASURE');
    expect(homeostatic).not.toContain('CONSOLIDATE');
    expect(homeostatic).not.toMatch(/who owns this file|claim on edit/i);
    expect(homeostatic.trim().split('\n').length).toBeLessThanOrEqual(50);
    expect(Buffer.byteLength(homeostatic, 'utf8')).toBeLessThanOrEqual(4 * 1024);
  });
});
