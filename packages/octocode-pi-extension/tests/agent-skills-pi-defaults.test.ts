/**
 * Locks in a design decision surfaced by cross-monorepo research
 * (packages/octocode-skill-installer's SKILL_PLATFORMS table writes installed
 * skills for the "pi" platform to `.pi/agent/skills` (global) and `.pi/skills`
 * (project) — see that package for the verified-against-upstream-docs source
 * paths). Pi's own host already discovers skills under those directories
 * natively and reports them to this extension via the `piSkills` parameter;
 * `defaultAgentSkillSources` must not *also* disk-scan them (that would be
 * redundant with Pi's native loader) and `discoverSkillCandidates` must not
 * reintroduce a `piSkills` entry that lives under them as a second, extension
 * -owned source (that would let host metadata double up a skill Pi already
 * owns). Neither behavior previously had a regression test.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { defaultAgentSkillSources } from '../src/contracts/agent-skills.js';
import { discoverSkillCandidates } from '../src/tools/skill-discovery.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-agent-skills-pi-defaults-'));
  roots.push(root);
  const cwd = path.join(root, 'repo');
  const homeDir = path.join(root, 'home');
  fs.mkdirSync(path.join(cwd, '.git'), { recursive: true });
  return { root, cwd, homeDir };
}
function skill(file: string, description: string) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `---\nname: native-thing\ndescription: ${description}\n---\nInstructions.`);
}

it('never disk-scans .pi/agent/skills or .pi/skills — Pi reports its own native skills via piSkills, not a directory this extension walks', () => {
  const { cwd, homeDir } = fixture();
  const roots = defaultAgentSkillSources(cwd, homeDir).map(source => source.root);
  expect(roots).not.toContain(path.resolve(homeDir, '.pi', 'agent', 'skills'));
  expect(roots).not.toContain(path.resolve(cwd, '.pi', 'skills'));
  expect(roots.some(root => root.split(path.sep).includes('.pi'))).toBe(false);
});

it('drops a piSkills entry whose file lives under Pi\'s global default skill directory instead of surfacing it as a second source', () => {
  const { cwd, homeDir } = fixture();
  const piNativeFile = path.join(homeDir, '.pi', 'agent', 'skills', 'native-thing', 'SKILL.md');
  skill(piNativeFile, 'reported by the host from its own native skills dir');
  const candidates = discoverSkillCandidates(cwd, [{ name: 'native-thing', description: 'native', path: piNativeFile }], homeDir);
  expect(candidates.some(candidate => candidate.name === 'native-thing')).toBe(false);
});

it('drops a piSkills entry whose file lives under Pi\'s project-scoped .pi directory the same way', () => {
  const { cwd, homeDir } = fixture();
  const piNativeFile = path.join(cwd, '.pi', 'skills', 'native-thing', 'SKILL.md');
  skill(piNativeFile, 'reported by the host from its own project .pi directory');
  const candidates = discoverSkillCandidates(cwd, [{ name: 'native-thing', description: 'native', path: piNativeFile }], homeDir);
  expect(candidates.some(candidate => candidate.name === 'native-thing')).toBe(false);
});

it('still admits a piSkills entry whose file lives outside any Pi default directory', () => {
  const { cwd, homeDir } = fixture();
  const runtimeFile = path.join(cwd, 'custom-location', 'native-thing', 'SKILL.md');
  skill(runtimeFile, 'a runtime-registered skill outside any Pi default dir');
  const candidates = discoverSkillCandidates(cwd, [{ name: 'native-thing', description: 'native', path: runtimeFile }], homeDir);
  const admitted = candidates.find(candidate => candidate.name === 'native-thing');
  expect(admitted).toBeDefined();
  expect(admitted?.vendor).toBe('pi');
});
