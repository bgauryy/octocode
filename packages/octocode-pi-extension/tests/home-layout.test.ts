import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadHooks, hookFiles } from '../src/hooks/config.js';
import { globalPaths, HOME_NAMES, workspacePaths } from '../src/shared/home.js';
import { fingerprint, projectConfigFiles, projectTrustNow, saveDecision } from '../src/shared/trust.js';
import { extraSkillDirs } from '../src/skills.js';
import { loadProfiles } from '../src/subagents/profiles.js';
import { tmp } from './helpers.js';

/** A git repo with `files` written under it, and a nested `pkg/src` folder to start sessions from. */
function repo(files: Record<string, string> = {}): { root: string; sub: string } {
  const root = fs.realpathSync(tmp('octocode-layout-'));
  fs.mkdirSync(path.join(root, '.git'));
  for (const [file, text] of Object.entries(files)) write(path.join(root, file), text);
  const sub = path.join(root, 'pkg', 'src');
  fs.mkdirSync(sub, { recursive: true });
  return { root, sub };
}

function write(file: string, text: string): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}

const profile = (name: string, prompt: string) => `---\nname: ${name}\ndescription: d\n---\n${prompt}\n`;

beforeEach(() => {
  vi.stubEnv('OCTOCODE_HOME', '');
  vi.stubEnv('PI_CODING_AGENT_DIR', tmp('octocode-layout-agent-'));
});
afterEach(() => vi.unstubAllEnvs());

describe('layout', () => {
  it('puts every global path under OCTOCODE_HOME, else <home>/.octocode', () => {
    const home = tmp();
    expect(globalPaths({}, home).home).toBe(path.join(home, '.octocode'));
    const moved = tmp();
    const paths = globalPaths({ OCTOCODE_HOME: moved }, home);
    expect(paths.home).toBe(moved);
    for (const [key, name] of Object.entries(HOME_NAMES)) expect(paths[key as keyof typeof HOME_NAMES]).toBe(path.join(moved, name));
    expect(paths.team).toBe(path.join(moved, 'pi-team'));
  });

  it('resolves workspace paths from the repository root when started in a subfolder', () => {
    const { root, sub } = repo();
    const paths = workspacePaths(sub);
    expect(paths.root).toBe(root);
    expect(paths).toMatchObject({ dir: path.join(root, '.octocode'), skills: path.join(root, '.octocode', 'skills'), agents: path.join(root, '.octocode', 'agents'), hooks: path.join(root, '.octocode', 'hooks.json'), tmp: path.join(root, '.octocode', 'tmp') });
  });
});

describe('trust covers the .octocode workspace files', () => {
  it('lists them from the repo root and re-asks when .octocode/hooks.json changes', () => {
    vi.stubEnv('OCTOCODE_HOME', tmp('octocode-layout-home-'));
    const { root, sub } = repo({ '.octocode/hooks.json': '{}', '.octocode/agents/x.md': profile('x', 'p'), '.octocode/skills/s/SKILL.md': '---\nname: s\n---\n' });
    expect(projectConfigFiles(sub)).toEqual([path.join(root, '.octocode', 'hooks.json'), path.join(root, '.octocode', 'agents', 'x.md'), path.join(root, '.octocode', 'skills', 's', 'SKILL.md')].sort());
    const ctx = { cwd: sub, isProjectTrusted: () => true };
    saveDecision(sub, fingerprint(projectConfigFiles(sub)), true);
    expect(projectTrustNow(ctx)).toBe(true);
    write(path.join(root, '.octocode', 'hooks.json'), '{"hooks":{}}');
    expect(projectTrustNow(ctx)).toBeUndefined();
  });
});

describe('profiles, hooks and skills in .octocode', () => {
  it('profiles: bundled → ~/.pi/agent/agents → ~/.octocode/agents → <root>/.pi/agents → <root>/.octocode/agents', () => {
    const home = tmp();
    write(path.join(home, '.pi', 'agent', 'agents', 'p.md'), profile('p', 'pi-user'));
    write(path.join(home, '.octocode', 'agents', 'p.md'), profile('p', 'octocode-user'));
    write(path.join(home, '.octocode', 'agents', 'q.md'), profile('q', 'octocode-user'));
    expect(loadProfiles(tmp(), home, true, {}).get('p')?.prompt).toBe('octocode-user');
    const { sub } = repo({ '.pi/agents/q.md': profile('q', 'pi-project'), '.octocode/agents/p.md': profile('p', 'octocode-project') });
    const trusted = loadProfiles(sub, home, true, {});
    expect(trusted.get('p')?.prompt).toBe('octocode-project');
    expect(trusted.get('q')?.prompt).toBe('pi-project');
    const untrusted = loadProfiles(sub, home, false, {});
    expect(untrusted.get('p')?.prompt).toBe('octocode-user');
    expect(untrusted.get('q')?.prompt).toBe('octocode-user');
  });

  it('hooks: reads ~/.octocode/hooks.json always and <root>/.octocode/hooks.json only when trusted', () => {
    const home = tmp();
    const hook = (command: string) => JSON.stringify({ hooks: { PreToolUse: [{ matcher: 'bash', hooks: [{ type: 'command', command }] }] } });
    write(path.join(home, '.octocode', 'hooks.json'), hook('user'));
    const { root, sub } = repo({ '.octocode/hooks.json': hook('project') });
    expect(hookFiles(sub, true, home)).toContain(path.join(root, '.octocode', 'hooks.json'));
    const commands = (trusted: boolean) => loadHooks(sub, trusted, home).config.PreToolUse.flatMap((group) => group.hooks.map((item) => item.command));
    expect(commands(true)).toEqual(['user', 'project']);
    expect(commands(false)).toEqual(['user']);
  });

  it('skills: adds <root>/.octocode/skills only when trusted', () => {
    const home = tmp();
    const { root, sub } = repo({ '.octocode/skills/s/SKILL.md': 'x' });
    expect(extraSkillDirs(sub, home, true, {})).toEqual([path.join(root, '.octocode', 'skills')]);
    expect(extraSkillDirs(sub, home, false, {})).toEqual([]);
  });
});
