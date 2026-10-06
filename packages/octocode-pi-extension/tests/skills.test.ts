import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { extraSkillDirs, extraSkillsEnabled } from '../src/skills.js';
import { tmp } from './helpers.js';

describe('skills', () => {
  it('discovers only existing skill directories', () => {
    const home = tmp();
    fs.mkdirSync(path.join(home, '.claude', 'skills'), { recursive: true });
    expect(extraSkillDirs(tmp(), home)).toEqual([path.join(home, '.claude', 'skills')]);
  });

  it('reads Octocode skills from OCTOCODE_HOME when set, else ~/.octocode', () => {
    const home = tmp();
    const custom = tmp();
    fs.mkdirSync(path.join(home, '.octocode', 'skills'), { recursive: true });
    fs.mkdirSync(path.join(custom, 'skills'), { recursive: true });
    expect(extraSkillDirs(tmp(), home, true, {})).toEqual([path.join(home, '.octocode', 'skills')]);
    expect(extraSkillDirs(tmp(), home, true, { OCTOCODE_HOME: custom })).toEqual([path.join(custom, 'skills')]);
  });

  it('loads extra skill directories only in the root session, unless turned off', () => {
    expect(extraSkillsEnabled({})).toBe(true);
    expect(extraSkillsEnabled({ OCTOCODE_SUBAGENT: '1' })).toBe(false);
    expect(extraSkillsEnabled({ OCTOCODE_EXTRA_SKILLS: '0' })).toBe(false);
  });

  it('adds project skill directories only for trusted projects', () => {
    const cwd = tmp();
    fs.mkdirSync(path.join(cwd, '.claude', 'skills'), { recursive: true });
    expect(extraSkillDirs(cwd, tmp(), false)).toEqual([]);
    expect(extraSkillDirs(cwd, tmp(), true)).toEqual([path.join(cwd, '.claude', 'skills')]);
  });

  it('finds project skills from a subdirectory up to the repository root', () => {
    const repo = tmp();
    fs.mkdirSync(path.join(repo, '.git'));
    fs.mkdirSync(path.join(repo, '.claude', 'skills'), { recursive: true });
    const sub = path.join(repo, 'packages', 'a');
    fs.mkdirSync(path.join(sub, '.claude', 'skills'), { recursive: true });
    expect(extraSkillDirs(sub, tmp(), true)).toEqual([path.join(sub, '.claude', 'skills'), path.join(repo, '.claude', 'skills')]);
  });

  it("leaves out claude.ai's synced skills under ~/.claude/skills/synced, as Pi loads the result", async () => {
    const home = tmp();
    const root = path.join(home, '.claude', 'skills');
    const skill = (dir: string, name: string) => {
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, 'SKILL.md'), `---\nname: ${name}\ndescription: ${name} skill\n---\nBody\n`);
    };
    skill(path.join(root, 'local-one'), 'local-one');
    skill(path.join(root, 'group', 'nested'), 'nested');
    skill(path.join(root, 'synced', 'docx'), 'docx');
    skill(path.join(root, 'synced', 'deep', 'pptx'), 'pptx');
    fs.writeFileSync(path.join(root, 'loose.md'), '---\nname: loose\ndescription: loose skill\n---\nBody\n');
    fs.writeFileSync(path.join(root, 'notes.txt'), 'not a skill');
    fs.mkdirSync(path.join(root, '.hidden'));
    const dirs = extraSkillDirs(tmp(), home);
    expect(dirs).toEqual([path.join(root, 'group'), path.join(root, 'local-one'), path.join(root, 'loose.md')]);
    // Pi's own loader over these paths finds the local skills and none of the synced ones.
    const piDist = path.dirname(fileURLToPath(import.meta.resolve('@earendil-works/pi-coding-agent')));
    const { loadSkills } = (await import(pathToFileURL(path.join(piDist, 'core', 'skills.js')).href)) as { loadSkills: (options: unknown) => { skills: Array<{ name: string }> } };
    const names = loadSkills({ cwd: tmp(), agentDir: tmp(), skillPaths: dirs, includeDefaults: false }).skills.map((s) => s.name).sort();
    expect(names).toEqual(['local-one', 'loose', 'nested']);
    // Without a synced folder, the skills folder is passed whole.
    fs.rmSync(path.join(root, 'synced'), { recursive: true });
    expect(extraSkillDirs(tmp(), home)).toEqual([root]);
  });
});
