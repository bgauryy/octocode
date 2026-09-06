import { describe, expect, it } from 'vitest';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
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
  it('owns the complete Awareness lifecycle in one architecture document', () => {
    const lifecycle = read(resolve(PACKAGE_ROOT, 'docs/HOW_IT_WORKS.md'));
    const catalog = read(resolve(PACKAGE_ROOT, 'docs/README.md'));
    const packageAgents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));
    expect(lifecycle).toMatch(/AGENTS\.md.*entry.*router[\s\S]*Agent Skills?.*policy[\s\S]*CLI.*control plane[\s\S]*hooks.*automation/i);
    expect(lifecycle).toContain('ENTER -> ACTIVATE -> ATTEND -> CHOOSE');
    expect(lifecycle).toContain('The Awareness CLI is the only agent-facing control plane for durable Awareness state.');
    expect(lifecycle).toMatch(/Plan[\s\S]*Task[\s\S]*Run[\s\S]*RunFile[\s\S]*Lock/);
    expect(lifecycle).toMatch(/successful write[\s\S]*failed write[\s\S]*PreCompact[\s\S]*SessionEnd/i);
    expect(catalog).toContain('complete bootstrap, operating, state, hook, memory, projection, and exit lifecycle');
    expect(packageAgents).toContain('docs/HOW_IT_WORKS.md');
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
      .map((path) => relative(PACKAGE_ROOT, path));
    expect(terminologyFailures).toEqual([]);
    const cheatSheet = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md'));
    expect(cheatSheet).not.toContain('<cli> schema commands --compact');
    expect(cheatSheet).not.toContain('<cli> docs list --compact');
    expect(cheatSheet).toContain('only when');
    const agents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));
    expect(agents).not.toContain('$AWARENESS schema commands --compact');
    expect(read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'))).not.toContain('<command> --help --compact');
    const helpData = read(resolve(PACKAGE_ROOT, 'bin/cli-help-data.ts'));
    expect(helpData).toContain('ROUTINE LOOP');
    expect(helpData).toContain('DEFAULT POLICY');
    expect(helpData).toContain('attend -> work start -> work end -> verify mark -> verify audit');
    expect(read(resolve(SKILL_ROOT, 'SKILL.md'))).toContain('references/octocode.md');
    expect(read(resolve(PACKAGE_ROOT, 'src/coordination/external-policy.ts'))).toContain('`npx octocode` for local, GitHub, and npm research');
    expect(read(resolve(SKILL_ROOT, 'SKILL.md'))).toContain('After verification, reflect reusable lessons');
    const octocodeReference = read(resolve(SKILL_ROOT, 'references/octocode.md'));
    for (const tool of ['ghSearch', 'ghGetFileContent', 'ghSearchPullRequests', 'ghSearchIssues', 'ghSearchCommits', 'ghListReleases', 'ghSearchDiscussions', 'ghCloneRepo', 'npmSearch', 'localSearch', 'localAnalyzeGraph', 'localGetFileContent', 'lspGetSemantics']) {
      expect(octocodeReference, `missing canonical tool ${tool}`).toContain(tool);
    }
    expect(octocodeReference).not.toMatch(/ghSearchCode|ghSearchRepos|ghViewRepoStructure|localSearchCode|localFindFiles|localViewStructure/);
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
    expect(direct.size).toBeLessThanOrEqual(18);
    expect(skill).not.toContain('scripts/install-hooks.mjs');
    expect(existsSync(resolve(SKILL_ROOT, 'scripts/install-hooks.mjs'))).toBe(false);
    expect(existsSync(resolve(SKILL_ROOT, 'scripts/package.json'))).toBe(false);
    const install = read(resolve(SKILL_ROOT, 'scripts/install.mjs'));
    expect(install).not.toMatch(/npm install|check-only|skip-deps|findNpm|installDependencies|REQUIRED_BUNDLED_SKILLS[^\n]*octocode-skills/);
  });
  it('keeps the current Awareness owner advisory-first and reserves exclusivity for sensitive files', () => {
    const combined = [
      read(resolve(SKILL_ROOT, 'SKILL.md')),
      read(resolve(SKILL_ROOT, 'references/files-awareness.md')),
      read(resolve(SKILL_ROOT, 'references/lock-protocol.md')),
    ].join('\n');

    expect(combined).toMatch(/advisory (?:file )?(?:work|presence)/i);
    expect(combined).toMatch(/exclusive.{0,80}sensitive|sensitive.{0,80}exclusive/is);
    expect(combined).not.toMatch(/taskless lock|lock exact files|file_lock without a task for quick work/i);
    expect(combined).not.toContain('Before every Pi write/edit call, the awareness bridge claims a lock');
    expect(combined).not.toMatch(/FILE_LOCK_KINDS|lock_type/);
  });
  it('removes legacy notify and lock-kind inputs from the shared tool adapter', () => {
    const operations = read(resolve(PACKAGE_ROOT, 'src/tool-operations.ts'));
    const types = read(resolve(PACKAGE_ROOT, 'src/types/locks-reflection.ts'));
    const intents = read(resolve(PACKAGE_ROOT, 'src/intents-preflight.ts'));

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

  it('ships the agent-host database identity with only explicit schema upgrades', () => {
    const databaseFiles = [
      'db-init.ts',
      'db-introspection.ts',
      'db-runtime.ts',
      'db-schema.ts',
      'db-maintenance.ts',
    ];
    const databaseSource = databaseFiles
      .map((file) => read(resolve(PACKAGE_ROOT, 'src', file)))
      .join('\n');

    expect(databaseSource).toContain('AWARENESS_APPLICATION_ID');
    expect(databaseSource).toMatch(/applicationId === AGENT_APPLICATION_ID[\s\S]*refusing Agent SQLite store/);
    expect(databaseSource).not.toMatch(/\b(?:user_version|AWARENESS_SCHEMA_VERSION)\b/i);
    expect(existsSync(resolve(PACKAGE_ROOT, 'src/db-rebuild.ts'))).toBe(false);
    const ownedArtifacts = [
      read(resolve(PACKAGE_ROOT, 'src/attend-model.ts')),
      read(resolve(PACKAGE_ROOT, 'src/attend-query.ts')),
      read(resolve(PACKAGE_ROOT, 'src/plans.ts')),
      read(resolve(PACKAGE_ROOT, 'src/repo-projection.ts')),
    ].join('\n');
    expect(ownedArtifacts).not.toContain('schema_version');
  });

  it('keeps Awareness release verification self-contained', () => {
    const manifest = read(resolve(PACKAGE_ROOT, 'package.json'));
    const verification = read(resolve(PACKAGE_ROOT, 'docs/VERIFY.md'));
    expect(manifest).toContain('"name": "@octocodeai/octocode-awareness"');
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness pack:check');
    expect(verification).toMatch(/isolated packed artifact/i);
  });

  it('documents separate fail-closed storage and flat work rows versus grouped FilesUnderWork', () => {
    const db = read(resolve(PACKAGE_ROOT, 'docs/DB.md'));
    expect(db).toContain('<workspace>/.octocode/awareness.sqlite3');
    expect(db).toContain('$OCTOCODE_HOME/awareness/awareness.sqlite3');
    expect(db).toMatch(/never opens[\s\S]*agent\/agent\.sqlite3[\s\S]*agent\/core\.sqlite3/i);
    expect(db).toMatch(/application_id[\s\S]*(?:OCT1|0x4f435431)/i);
    expect(db).toMatch(/Query ownership[\s\S]*`files`[\s\S]*src\/repo-files\.ts/i);
  });

  it('keeps user-facing setup and examples copy-pasteable', () => {
    const readme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const guide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const reflection = read(resolve(PACKAGE_ROOT, 'docs/REFLECTION.md'));
    const navigation = read(resolve(PACKAGE_ROOT, 'docs/MEMORY_NAVIGATION.md'));

    expect(readme).not.toContain('docs show full-flow');
    expect(read(resolve(PACKAGE_ROOT, 'bin/awareness.ts'))).not.toContain('docs show full-flow');

    for (const installDoc of [readme, guide]) {
      expect(installDoc).toContain('npx @octocodeai/octocode-awareness maintenance init --compact');
      expect(installDoc).not.toMatch(/<package>|npm install --global|npm root --global|node packages\/octocode-awareness\/out|node scripts\/awareness\.mjs|out\/skills\/octocode-skills/);
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
    const guide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const skillReadme = read(resolve(SKILL_ROOT, 'README.md'));

    expect(readme).toContain('docs/REFERENCES.md');
    expect(docsIndex).toContain('REFERENCES.md');
    expect(references).toMatch(/implemented invariant/i);
    expect(references).toMatch(/adjacent prior art/i);
    expect(references).toMatch(/follow-on hypothesis/i);
    expect(navigation).toMatch(/limit applies per lane/i);
    expect(navigation).toMatch(/minifies JSON/i);
    expect(guide).not.toMatch(/verify audit[^\n]*--all-pending/i);
    expect(skill).not.toMatch(/before start, read `references\/agent-cheatsheet\.md`/i);
    expect(skillReadme).not.toContain('`--name octocode-awareness`');
  });

  it('keeps the always-loaded skill lobby byte-bounded and finish work conditional', () => {
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const finish = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md'));
    expect(Buffer.byteLength(skill, 'utf8')).toBeLessThanOrEqual(6_400);
    expect(skill).not.toMatch(/Haiku|Composer 2\.5/);
    expect(finish).toMatch(/Always[\s\S]*verify audit[\s\S]*Only when/);
    expect(finish).not.toMatch(/query all[^\n]*repo inject/is);
  });

  it('removes retired wiki/projection generation guidance and stays copy-runnable', () => {
    const generator = read(resolve(PACKAGE_ROOT, 'src/repo-query.ts'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const taskFlow = read(resolve(SKILL_ROOT, 'references/plan-task-workflow.md'));

    expect(generator).not.toContain('injectRepoContext');
    expect(skill).toMatch(/cleanup remains dry-run-first/i);
    expect(skill).not.toContain('wiki sync');
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
    const packageAgents = read(resolve(PACKAGE_ROOT, 'AGENTS.md'));
    const architecture = read(resolve(PACKAGE_ROOT, 'docs/HOW_IT_WORKS.md'));
    const userGuide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const hooks = read(resolve(PACKAGE_ROOT, 'docs/HOOKS.md'));
    const skill = read(resolve(SKILL_ROOT, 'SKILL.md'));
    expect(Buffer.byteLength(packageAgents, 'utf8')).toBeLessThanOrEqual(3_600);

    expect(packageAgents).toMatch(/AGENTS.*routes.*skill.*policy.*CLI.*live state.*hooks.*automat/is);
    expect(packageAgents).toContain('npx @octocodeai/octocode-awareness');
    expect(packageAgents).toContain('Follow typed `next` results');
    expect(packageAgents).toContain('one canonical ledger and lifecycle');
    expect(packageAgents).not.toContain('two explicit planes');
    expect(packageAgents).not.toMatch(/AWARENESS_CLI="packages\/octocode-awareness\/out\/octocode-awareness\.js"/);
    expect(packageAgents).not.toContain('## Lifecycle');
    expect(packageAgents).not.toContain('## Hooks');
    expect(packageAgents).not.toContain('Standalone WORK');
    expect(skill).toContain('NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE → SETTLE/VERIFY → LEARN');
    expect(skill).toContain('Start small');
    expect(userGuide).toContain('## Use the operating loop');
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
    expect(verification).toContain('npx @octocodeai/octocode-awareness');
    expect(verification).not.toMatch(/node .*octocode-awareness|scripts\/install\.mjs/);
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness test:smoke');
    expect(verification).toContain('hooks check --host <claude|codex|copilot|cursor|gemini|opencode>');
    expect(verification).toMatch(/config.*runtime.*unverified/is);
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness lint');
    expect(verification).toContain('yarn workspace @octocodeai/octocode-awareness pack:check');
    expect(verification).toContain('focused skill-behavior checks');
    expect(verification).not.toContain('skill-review.mjs');
    expect(verification).toMatch(/PASS[\s\S]*FAIL[\s\S]*BLOCKED/);
    expect(verification).toContain('## Receipt');
    expect(verification).toMatch(/Yarn's isolated packed artifact/i);
    expect(verification.indexOf('--dry-run')).toBeLessThan(verification.indexOf('hooks install --host <claude|codex|copilot|cursor|gemini|opencode>'));
  });

  it('keeps lifecycle recipes scoped, executable, and ordered around active presence', () => {
    const finish = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md'));
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
      .toBeLessThan(taskFlow.indexOf('npx @octocodeai/octocode-awareness task submit'));
    expect(taskFlow.indexOf('npx @octocodeai/octocode-awareness task submit'))
      .toBeLessThan(taskFlow.indexOf('npx @octocodeai/octocode-awareness verify mark'));
  });
  it('makes fresh-agent install, activation, and hook ownership safe and executable', () => {
    const packageReadme = read(resolve(PACKAGE_ROOT, 'README.md'));
    const userGuide = read(resolve(PACKAGE_ROOT, 'docs/SKILLS.md'));
    const skillReadme = read(resolve(SKILL_ROOT, 'README.md'));
    const skillLobby = read(resolve(SKILL_ROOT, 'SKILL.md'));
    const tooling = read(resolve(SKILL_ROOT, 'references/agent-cheatsheet.md'));
    const hooks = read(resolve(SKILL_ROOT, 'references/hooks.md'));
    const packageHooks = read(resolve(PACKAGE_ROOT, 'docs/HOOKS.md'));
    for (const guide of [packageReadme, userGuide]) {
      expect(guide).toContain('--dry-run');
      expect(guide.indexOf('--dry-run')).toBeLessThan(guide.indexOf('hooks check'));
      expect(guide).toContain('npx @octocodeai/octocode-awareness');
      expect(guide).not.toMatch(/npm root --global|node packages\/octocode-awareness\/out/);
    }
    for (const guide of [skillReadme, tooling]) {
      expect(guide).toContain('npx @octocodeai/octocode-awareness');
      expect(guide).not.toMatch(/npx octocode skill --add|npm root --global/);
    }
    expect(tooling).toContain('export OCTOCODE_AGENT_ID');
    expect(skillLobby).toContain('npx @octocodeai/octocode-awareness attend');
    expect(skillLobby).toContain('scripts/install.mjs');
    for (const guide of [hooks, packageHooks]) {
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
    expect(thesis).toMatch(/## Why Homeostasis[\s\S]*dynamic regulation.*viable range[\s\S]*not.*equilibrium/is);
    expect(thesis).toMatch(/SENSE[\s\S]*COMPARE[\s\S]*ACT[\s\S]*REMEASURE/);
    expect(thesis).toMatch(/living-system.*metaphor|metaphor.*living-system/is);
    expect(thesis).toMatch(/not sentience|not.*sentien/i);
    expect(thesis).toContain('Token pressure');
    expect(thesis).toMatch(/Sensor[\s\S]*Target[\s\S]*Actuator[\s\S]*Guard/);
    expect(thesis.trim().split(/\s+/).length).toBeLessThanOrEqual(1400);
    expect(references).toContain('## Homeostasis And Collective Memory');
    expect(references).toContain('**Homeostasis — adjacent prior art:**');
    expect(homeostatic).toMatch(/CHOOSE\/DECLARE[\s\S]*REMEASURE/);
    expect(homeostatic).not.toContain('CONSOLIDATE');
    expect(homeostatic).not.toMatch(/who owns this file|claim on edit/i);
    expect(homeostatic.trim().split('\n').length).toBeLessThanOrEqual(50);
    expect(Buffer.byteLength(homeostatic, 'utf8')).toBeLessThanOrEqual(4 * 1024);
  });
});
