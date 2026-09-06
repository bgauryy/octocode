import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { workspaceAgentRoot } from '../src/paths.js';
import {
  defaultAgentSkillRoots,
  defaultAgentSkillSources,
  discoverAgentSkillInventory,
  discoverAgentSkills,
  effectiveAgentSkills,
  listAgentSkillFiles,
  parseAgentSkill,
} from '../src/agent-skills.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

describe('Agent Skills specification', () => {
  it('parses full YAML metadata and keeps allowed-tools informational', () => {
    const parsed = parseAgentSkill(`---\nname: release-check\ndescription: >-\n  Run release checks safely across packages.\nlicense: MIT\ncompatibility: Requires Node.js 22+\nmetadata:\n  author: octocode\n  version: "1"\nallowed-tools: Bash(git:*) Read\n---\n# Release\n\nRun the checks.`, 'release-check');
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.skill.description).toBe('Run release checks safely across packages.');
    expect(parsed.skill.metadata).toEqual({ author: 'octocode', version: '1' });
    expect(parsed.skill.allowedTools).toBe('Bash(git:*) Read');
  });

  it.each([
    ['bad directory name', 'valid-name', 'other-name'],
    ['consecutive hyphens', 'bad--name', 'bad--name'],
    ['non-string metadata', 'valid-name', 'valid-name', 'metadata:\n  version: 1\n'],
  ])('rejects %s', (_label, name, directory, extra = '') => {
    const parsed = parseAgentSkill(`---\nname: ${name}\ndescription: Valid description.\n${extra}---\n# Body`, directory);
    expect(parsed.ok).toBe(false);
  });

  it('discovers standard roots, rejects malformed skills, and preserves root precedence', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skills-'));
    roots.push(root);
    const projectRoot = path.join(root, 'project', '.agents', 'skills');
    const userRoot = path.join(root, 'home', '.agents', 'skills');
    for (const [base, description] of [[userRoot, 'user'], [projectRoot, 'project']] as const) {
      const dir = path.join(base, 'release-check');
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, 'SKILL.md'), `---\nname: release-check\ndescription: ${description} skill\n---\n# Release`);
    }
    const invalid = path.join(projectRoot, 'BAD');
    fs.mkdirSync(invalid, { recursive: true });
    fs.writeFileSync(path.join(invalid, 'SKILL.md'), '---\nname: BAD\ndescription: bad\n---\n# Bad');

    const result = discoverAgentSkills([userRoot, projectRoot]);
    expect(result.skills).toHaveLength(1);
    expect(result.skills[0]?.description).toBe('project skill');
    expect(result.errors).toHaveLength(1);
  });

  it('skips support-file symlinks without hiding later files', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skill-files-'));
    roots.push(root);
    fs.writeFileSync(path.join(root, 'SKILL.md'), '# Skill');
    fs.writeFileSync(path.join(root, 'z-details.md'), '# Details');
    fs.symlinkSync(path.join(root, 'z-details.md'), path.join(root, 'a-link.md'));

    expect(listAgentSkillFiles(root)).toEqual(['z-details.md']);
  });

  it('rejects a SKILL.md symlink that escapes its Skill directory', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skill-leaf-link-'));
    roots.push(root);
    const skillRoot = path.join(root, 'skills');
    const skillDir = path.join(skillRoot, 'escaped');
    const outside = path.join(root, 'outside.md');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(outside, '---\nname: escaped\ndescription: Escaped skill.\n---\n# Escaped');
    fs.symlinkSync(outside, path.join(skillDir, 'SKILL.md'));

    const inventory = discoverAgentSkillInventory([{
      id: 'test',
      vendor: 'custom',
      scope: 'user',
      root: skillRoot,
      precedence: 1,
      defaultEnabled: true,
    }]);

    expect(inventory.entries).toContainEqual(expect.objectContaining({
      name: 'escaped',
      parseStatus: 'invalid',
      diagnostic: 'SKILL.md symbolic links are not allowed',
    }));
    expect(effectiveAgentSkills(inventory.entries)).toEqual([]);
  });

  it('discovers project Skill roots from the repository root through a nested working directory', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skill-roots-'));
    roots.push(root);
    fs.mkdirSync(path.join(root, '.git'));
    const nested = path.join(root, 'packages', 'app');
    fs.mkdirSync(nested, { recursive: true });

    const discovered = defaultAgentSkillRoots(nested, path.join(root, 'home'), path.join(root, 'octocode-home'));

    expect(discovered).toContain(path.join(root, '.agents', 'skills'));
    expect(discovered).toContain(path.join(root, 'packages', '.agents', 'skills'));
    expect(discovered).toContain(path.join(nested, '.agents', 'skills'));
    expect(discovered.indexOf(path.join(root, '.agents', 'skills')))
      .toBeLessThan(discovered.indexOf(path.join(nested, '.agents', 'skills')));
  });

  it('describes canonical Octocode roots as enabled and retained vendor conventions as disabled', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skill-sources-'));
    roots.push(root);
    const workspace = path.join(root, 'repo', 'packages', 'app');
    fs.mkdirSync(path.join(root, 'repo', '.git'), { recursive: true });
    fs.mkdirSync(workspace, { recursive: true });
    const home = path.join(root, 'home');
    const octocodeHome = path.join(root, 'octocode-home');

    const sources = defaultAgentSkillSources(workspace, home, octocodeHome);

    expect(sources).toEqual(expect.arrayContaining([
      expect.objectContaining({ vendor: 'octocode', scope: 'user', root: path.join(octocodeHome, 'agent', 'skills'), defaultEnabled: true }),
      expect.objectContaining({ vendor: 'octocode', scope: 'workspace', root: path.join(workspaceAgentRoot(path.join(root, 'repo'), octocodeHome), 'skills'), defaultEnabled: true }),
      expect.objectContaining({ vendor: 'agents', scope: 'user', root: path.join(home, '.agents', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'agent', scope: 'user', root: path.join(home, '.agent', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'pi', scope: 'user', root: path.join(home, '.pi', 'agent', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'pi', scope: 'workspace', root: path.join(workspace, '.pi', 'agent', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'agent', scope: 'workspace', root: path.join(workspace, '.agent', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'claude', scope: 'workspace', root: path.join(workspace, '.claude', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'cursor', scope: 'workspace', root: path.join(workspace, '.cursor', 'skills'), defaultEnabled: false }),
      expect.objectContaining({ vendor: 'codex', scope: 'workspace', root: path.join(workspace, '.codex', 'skills'), defaultEnabled: false }),
    ]));
    expect(defaultAgentSkillRoots(workspace, home, octocodeHome)).toEqual(sources.map(({ root: sourceRoot }) => sourceRoot));
  });

  it('keeps a provenance-rich lossless inventory and resolves only enabled valid entries', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-skill-inventory-'));
    roots.push(root);
    const nativeRoot = path.join(root, 'native');
    const foreignRoot = path.join(root, 'foreign');
    for (const [base, description] of [[nativeRoot, 'native'], [foreignRoot, 'foreign']] as const) {
      const dir = path.join(base, 'release-check');
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, 'SKILL.md'), `---\nname: release-check\ndescription: ${description} skill\n---\n# Release`);
    }
    const invalidDir = path.join(foreignRoot, 'broken');
    fs.mkdirSync(invalidDir, { recursive: true });
    fs.writeFileSync(path.join(invalidDir, 'SKILL.md'), 'not frontmatter');

    const inventory = discoverAgentSkillInventory([
      { id: 'octocode:user', vendor: 'octocode', scope: 'user', root: nativeRoot, precedence: 0, defaultEnabled: true },
      { id: 'agents:user', vendor: 'agents', scope: 'user', root: foreignRoot, precedence: 1, defaultEnabled: false },
    ]);

    expect(inventory.entries).toEqual(expect.arrayContaining([
      expect.objectContaining({ name: 'release-check', source: 'octocode:user', vendor: 'octocode', scope: 'user', precedence: 0, enabled: true, parseStatus: 'valid', hash: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/) }),
      expect.objectContaining({ name: 'release-check', source: 'agents:user', vendor: 'agents', scope: 'user', precedence: 1, enabled: false, parseStatus: 'valid', hash: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/) }),
      expect.objectContaining({ name: 'broken', source: 'agents:user', enabled: false, parseStatus: 'invalid', diagnostic: expect.stringMatching(/frontmatter/i), hash: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/) }),
    ]));
    expect(effectiveAgentSkills(inventory.entries).map(({ description }) => description)).toEqual(['native skill']);

    const overridden = discoverAgentSkillInventory([
      { id: 'agents:user', vendor: 'agents', scope: 'user', root: foreignRoot, precedence: 1, defaultEnabled: false },
    ], () => true);
    expect(effectiveAgentSkills(overridden.entries).map(({ name }) => name)).toEqual(['release-check']);
  });
});
