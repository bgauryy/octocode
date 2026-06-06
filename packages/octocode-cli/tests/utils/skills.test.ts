import path from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/utils/fs.js', () => ({
  dirExists: vi.fn(),
  copyDirectory: vi.fn(),
  listSubdirectories: vi.fn(),
  fileExists: vi.fn(),
  readFileContent: vi.fn(),
}));

vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  rmSync: vi.fn(),
  symlinkSync: vi.fn(),
}));

import {
  dirExists,
  copyDirectory,
  listSubdirectories,
  fileExists,
  readFileContent,
} from '../../src/utils/fs.js';
import {
  getSkillsSourcePath,
  getSkillsSourceDir,
  copySkills,
  copySkill,
  getAvailableSkills,
  getSkillMetadata,
  getAllSkillsMetadata,
  isSafeSkillName,
  resolveModeForTarget,
  resolveSkillDestination,
  installSkillToDestination,
} from '../../src/utils/skills.js';
import { existsSync, mkdirSync, rmSync, symlinkSync } from 'node:fs';
import { getSkillsDirForTarget } from '../../src/utils/skills.js';

describe('Skills Utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('getSkillsSourcePath', () => {
    it('should return fromOut path when it exists', () => {
      vi.mocked(dirExists).mockImplementation((p: string) => {
        return p.includes('skills') && !p.includes('../..');
      });

      const result = getSkillsSourcePath();
      expect(result).toMatch(/skills$/);
      expect(dirExists).toHaveBeenCalled();
    });

    it('should return fromSrc path when fromOut does not exist', () => {
      let callCount = 0;
      vi.mocked(dirExists).mockImplementation(() => {
        callCount++;

        return callCount === 2;
      });

      const result = getSkillsSourcePath();
      expect(result).toMatch(/skills$/);
      expect(dirExists).toHaveBeenCalledTimes(2);
    });

    it('should throw error when no candidate skills path exists', () => {
      vi.mocked(dirExists).mockReturnValue(false);

      expect(() => getSkillsSourcePath()).toThrow('Skills directory not found');
      expect(dirExists).toHaveBeenCalledTimes(4);
    });

    it('should check fromOut path first', () => {
      const checkedPaths: string[] = [];
      vi.mocked(dirExists).mockImplementation((p: string) => {
        checkedPaths.push(p);
        return true;
      });

      getSkillsSourcePath();

      expect(checkedPaths).toHaveLength(1);
      expect(checkedPaths[0]).toMatch(/skills$/);
    });
  });

  describe('getSkillsSourceDir', () => {
    it('should return fallback ../skills when no candidate directory exists', () => {
      vi.mocked(dirExists).mockReturnValue(false);

      const result = getSkillsSourceDir();

      expect(result).toMatch(/skills$/);
      expect(dirExists).toHaveBeenCalledTimes(4);
    });

    it('should return the second candidate when the first is missing', () => {
      const checked: string[] = [];
      vi.mocked(dirExists).mockImplementation((p: string) => {
        checked.push(p);
        return checked.length === 2;
      });

      const result = getSkillsSourceDir();

      expect(result).toBe(checked[1]);
      expect(checked).toHaveLength(2);
    });
  });

  describe('copySkills', () => {
    it('should copy skills directory to destination', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory).mockReturnValue(true);

      const result = copySkills('/dest/skills');

      expect(result).toBe(true);
      expect(copyDirectory).toHaveBeenCalledWith(
        expect.stringMatching(/skills$/),
        '/dest/skills'
      );
    });

    it('should return false when copy fails', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory).mockReturnValue(false);

      const result = copySkills('/dest/skills');

      expect(result).toBe(false);
    });

    it('should throw when source path not found', () => {
      vi.mocked(dirExists).mockReturnValue(false);

      expect(() => copySkills('/dest/skills')).toThrow(
        'Skills directory not found'
      );
    });
  });

  describe('copySkill', () => {
    it('should copy specific skill to destination', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory).mockReturnValue(true);

      const result = copySkill('octocode-research', '/dest/skills');

      expect(result).toBe(true);
      expect(copyDirectory).toHaveBeenCalledWith(
        expect.stringMatching(/octocode-research$/),
        expect.stringMatching(/octocode-research$/)
      );
    });

    it('should return false when skill directory does not exist', () => {
      let callCount = 0;
      vi.mocked(dirExists).mockImplementation(() => {
        callCount++;
        return callCount === 1;
      });

      const result = copySkill('nonexistent-skill', '/dest/skills');

      expect(result).toBe(false);
      expect(copyDirectory).not.toHaveBeenCalled();
    });

    it('should return false when copy fails', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory).mockReturnValue(false);

      const result = copySkill('octocode-research', '/dest/skills');

      expect(result).toBe(false);
    });

    it('should construct correct destination path', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory).mockReturnValue(true);

      copySkill('octocode-plan', '/home/user/.claude/skills');

      expect(copyDirectory).toHaveBeenCalledWith(
        expect.any(String),
        '/home/user/.claude/skills/octocode-plan'
      );
    });

    it('should throw when source path not found', () => {
      vi.mocked(dirExists).mockReturnValue(false);

      expect(() => copySkill('octocode-research', '/dest')).toThrow(
        'Skills directory not found'
      );
    });

    it('should reject unsafe skill names before copying', () => {
      vi.mocked(dirExists).mockReturnValue(true);

      const result = copySkill('../evil', '/dest/skills');

      expect(result).toBe(false);
      expect(copyDirectory).not.toHaveBeenCalled();
    });
  });

  describe('shared install helpers', () => {
    it('validates skill names as path segments only', () => {
      expect(isSafeSkillName('octocode-research')).toBe(true);
      expect(isSafeSkillName('../evil')).toBe(false);
      expect(isSafeSkillName('octocode/evil')).toBe(false);
      expect(isSafeSkillName('octocode\\evil')).toBe(false);
      expect(isSafeSkillName('.')).toBe(false);
      expect(isSafeSkillName(' octocode-research')).toBe(false);
    });

    it('resolves skill destinations only under the destination directory', () => {
      expect(resolveSkillDestination('/dest/skills', 'octocode-plan')).toBe(
        '/dest/skills/octocode-plan'
      );
      expect(resolveSkillDestination('/dest/skills', '../evil')).toBeNull();
    });

    it('keeps hybrid mode policy in the shared utility', () => {
      expect(resolveModeForTarget('hybrid', 'claude-code')).toBe('copy');
      expect(resolveModeForTarget('hybrid', 'claude-desktop')).toBe('copy');
      expect(resolveModeForTarget('hybrid', 'cursor')).toBe('symlink');
      expect(resolveModeForTarget('copy', 'cursor')).toBe('copy');
    });
  });

  describe('getAvailableSkills', () => {
    it('should return skills starting with octocode-', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'octocode-research',
        'octocode-plan',
        'octocode-generate',
        'other-skill',
        'random-dir',
      ]);

      const result = getAvailableSkills();

      expect(result).toEqual([
        'octocode-research',
        'octocode-plan',
        'octocode-generate',
      ]);
      expect(result).not.toContain('other-skill');
      expect(result).not.toContain('random-dir');
    });

    it('should return empty array when no skills found', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([]);

      const result = getAvailableSkills();

      expect(result).toEqual([]);
    });

    it('should return empty array when no octocode- prefixed skills', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'other-skill',
        'random-dir',
      ]);

      const result = getAvailableSkills();

      expect(result).toEqual([]);
    });

    it('should filter out non-octocode prefixed directories', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'octocode-pull-request-reviewer',
        '.git',
        'node_modules',
        'octocode-test',
      ]);

      const result = getAvailableSkills();

      expect(result).toEqual([
        'octocode-pull-request-reviewer',
        'octocode-test',
      ]);
      expect(result).toHaveLength(2);
    });

    it('should call listSubdirectories with correct source path', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([]);

      getAvailableSkills();

      expect(listSubdirectories).toHaveBeenCalledWith(
        expect.stringMatching(/skills$/)
      );
    });

    it('should throw when source path not found', () => {
      vi.mocked(dirExists).mockReturnValue(false);

      expect(() => getAvailableSkills()).toThrow('Skills directory not found');
    });
  });

  describe('integration scenarios', () => {
    it('should handle typical install workflow', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'octocode-research',
        'octocode-plan',
        'octocode-generate',
        'octocode-pull-request-reviewer',
      ]);
      vi.mocked(copyDirectory).mockReturnValue(true);

      const skills = getAvailableSkills();
      expect(skills).toHaveLength(4);

      const copyAllResult = copySkills('/home/user/.claude/skills');
      expect(copyAllResult).toBe(true);

      const copyOneResult = copySkill(
        'octocode-research',
        '/home/user/.claude/skills'
      );
      expect(copyOneResult).toBe(true);
    });

    it('should handle partial failure gracefully', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(copyDirectory)
        .mockReturnValueOnce(true)
        .mockReturnValueOnce(false);

      const result1 = copySkill('octocode-research', '/dest');
      const result2 = copySkill('octocode-plan', '/dest');

      expect(result1).toBe(true);
      expect(result2).toBe(false);
    });
  });

  describe('getSkillMetadata', () => {
    const validSkillMd = `---
name: octocode-research
description: Answers questions about codebases, implementations, dependencies.
---

# Octocode Research

Some content here.`;

    it('should parse valid SKILL.md frontmatter', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue(validSkillMd);

      const result = getSkillMetadata('/path/to/octocode-research');

      expect(result).toEqual({
        name: 'octocode-research',
        description:
          'Answers questions about codebases, implementations, dependencies.',
        folder: 'octocode-research',
      });
    });

    it('should return null when SKILL.md does not exist', () => {
      vi.mocked(fileExists).mockReturnValue(false);

      const result = getSkillMetadata('/path/to/missing-skill');

      expect(result).toBeNull();
      expect(readFileContent).not.toHaveBeenCalled();
    });

    it('should return null when file content is null', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue(null);

      const result = getSkillMetadata('/path/to/skill');

      expect(result).toBeNull();
    });

    it('should return null when frontmatter is missing', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue('# No frontmatter here');

      const result = getSkillMetadata('/path/to/skill');

      expect(result).toBeNull();
    });

    it('should return null when name is missing', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue(`---
description: Only description, no name
---`);

      const result = getSkillMetadata('/path/to/skill');

      expect(result).toBeNull();
    });

    it('should return null when description is missing', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue(`---
name: only-name
---`);

      const result = getSkillMetadata('/path/to/skill');

      expect(result).toBeNull();
    });

    it('should extract folder name from path', () => {
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValue(validSkillMd);

      const result = getSkillMetadata('/some/long/path/octocode-plan');

      expect(result?.folder).toBe('octocode-plan');
    });
  });

  describe('getAllSkillsMetadata', () => {
    it('should return metadata for all octocode- skills', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'octocode-research',
        'octocode-plan',
        'other-dir',
      ]);
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent).mockReturnValueOnce(`---
name: octocode-research
description: Research skill description
---`).mockReturnValueOnce(`---
name: octocode-plan
description: Plan skill description
---`);

      const result = getAllSkillsMetadata();

      expect(result).toHaveLength(2);
      expect(result[0].name).toBe('octocode-research');
      expect(result[1].name).toBe('octocode-plan');
    });

    it('should skip skills with invalid SKILL.md', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'octocode-valid',
        'octocode-invalid',
      ]);
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readFileContent)
        .mockReturnValueOnce(
          `---
name: octocode-valid
description: Valid skill
---`
        )
        .mockReturnValueOnce('# No frontmatter');

      const result = getAllSkillsMetadata();

      expect(result).toHaveLength(1);
      expect(result[0].name).toBe('octocode-valid');
    });

    it('should return empty array when no skills exist', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([]);

      const result = getAllSkillsMetadata();

      expect(result).toEqual([]);
    });

    it('should filter non-octocode directories', () => {
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(listSubdirectories).mockReturnValue([
        'some-other-dir',
        '.git',
        'node_modules',
      ]);

      const result = getAllSkillsMetadata();

      expect(result).toEqual([]);
      expect(fileExists).not.toHaveBeenCalled();
    });
  });
});

describe('Skills Config', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  describe('getCustomSkillsDestDir', () => {
    it('should return null when config file does not exist', async () => {
      const { existsSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(false);

      const { getCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getCustomSkillsDestDir();

      expect(result).toBeNull();
    });

    it('should return null when config has no skillsDestDir', async () => {
      const { existsSync, readFileSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{}');

      const { getCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getCustomSkillsDestDir();

      expect(result).toBeNull();
    });

    it('should return custom path when set in config', async () => {
      const { existsSync, readFileSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({ skillsDestDir: '/custom/path' })
      );

      const { getCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getCustomSkillsDestDir();

      expect(result).toBe('/custom/path');
    });

    it('should return null when config file is invalid JSON', async () => {
      const { existsSync, readFileSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('invalid json');

      const { getCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getCustomSkillsDestDir();

      expect(result).toBeNull();
    });
  });

  describe('setCustomSkillsDestDir', () => {
    it('should create config directory if it does not exist', async () => {
      const { existsSync, mkdirSync, readFileSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(false);
      vi.mocked(readFileSync).mockReturnValue('{}');

      const { setCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      setCustomSkillsDestDir('/new/path');

      expect(mkdirSync).toHaveBeenCalledWith(
        expect.stringContaining('.octocode'),
        { recursive: true, mode: 0o700 }
      );
    });

    it('should save custom path to config file', async () => {
      const { existsSync, readFileSync, writeFileSync } =
        await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{}');

      const { setCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      setCustomSkillsDestDir('/custom/skills/path');

      expect(writeFileSync).toHaveBeenCalledWith(
        expect.stringContaining('config.json'),
        expect.stringContaining('/custom/skills/path'),
        expect.objectContaining({
          encoding: 'utf-8',
          mode: 0o600,
        })
      );
    });

    it('should remove skillsDestDir from config when null is passed', async () => {
      const { existsSync, readFileSync, writeFileSync } =
        await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({ skillsDestDir: '/old/path', otherSetting: true })
      );

      const { setCustomSkillsDestDir } =
        await import('../../src/utils/skills.js');
      setCustomSkillsDestDir(null);

      expect(writeFileSync).toHaveBeenCalledWith(
        expect.stringContaining('config.json'),
        expect.not.stringContaining('skillsDestDir'),
        expect.objectContaining({
          encoding: 'utf-8',
          mode: 0o600,
        })
      );
    });
  });

  describe('getDefaultSkillsDestDir', () => {
    it('should return default path', async () => {
      const { getDefaultSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getDefaultSkillsDestDir();

      expect(result).toMatch(/[Cc]laude.*skills$/);
    });

    it('should return AppData Claude/skills path on Windows', async () => {
      vi.doMock('../../src/utils/platform.js', () => ({
        isWindows: true,
        isMac: false,
        HOME: 'C:\\Users\\test',
        getAppDataPath: vi.fn(() => 'C:\\Users\\test\\AppData\\Roaming'),
      }));

      try {
        const { getDefaultSkillsDestDir } =
          await import('../../src/utils/skills.js');

        expect(getDefaultSkillsDestDir()).toBe(
          path.join('C:\\Users\\test\\AppData\\Roaming', 'Claude', 'skills')
        );
      } finally {
        vi.doUnmock('../../src/utils/platform.js');
      }
    });
  });

  describe('getSkillsDestDir', () => {
    it('should return custom path when set', async () => {
      const { existsSync, readFileSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({ skillsDestDir: '/custom/path' })
      );

      const { getSkillsDestDir } = await import('../../src/utils/skills.js');
      const result = getSkillsDestDir();

      expect(result).toBe('/custom/path');
    });

    it('should return default path when no custom path set', async () => {
      const { existsSync } = await import('node:fs');
      vi.mocked(existsSync).mockReturnValue(false);

      const { getSkillsDestDir, getDefaultSkillsDestDir } =
        await import('../../src/utils/skills.js');
      const result = getSkillsDestDir();
      const defaultPath = getDefaultSkillsDestDir();

      expect(result).toBe(defaultPath);
    });
  });
});

describe('getSkillsDirForTarget — all targets', () => {
  it('returns defaultDestDir for claude-code', () => {
    const result = getSkillsDirForTarget('claude-code', '/custom/dest');
    expect(result).toBe('/custom/dest');
  });

  it('returns HOME-based path for cursor', () => {
    const result = getSkillsDirForTarget('cursor', '/custom/dest');
    expect(result).toContain('.cursor');
    expect(result).toContain('skills');
  });

  it('returns HOME-based path for claude-desktop', () => {
    const result = getSkillsDirForTarget('claude-desktop', '/custom/dest');
    expect(result).toContain('skills');
  });

  it('returns HOME-based path for codex', () => {
    const result = getSkillsDirForTarget('codex', '/custom/dest');
    expect(result).toContain('skills');
  });

  it('returns HOME-based path for opencode', () => {
    const result = getSkillsDirForTarget('opencode', '/custom/dest');
    expect(result).toContain('skills');
  });

  it('returns HOME-based path for agents', () => {
    const result = getSkillsDirForTarget('agents', '/custom/dest');
    expect(result).toContain('.agents');
    expect(result).toContain('skills');
  });
});

describe('installSkillToDestination', () => {
  beforeEach(() => {
    vi.mocked(existsSync).mockReset();
    vi.mocked(mkdirSync).mockReset();
    vi.mocked(rmSync).mockReset();
    vi.mocked(symlinkSync).mockReset();
    vi.mocked(dirExists).mockReset();
    vi.mocked(copyDirectory).mockReset();
  });

  it('creates parent dir when it does not exist then copies (copy mode)', () => {
    vi.mocked(existsSync).mockReturnValue(false);
    vi.mocked(dirExists).mockImplementation(
      (p: string) => p === '/src/my-skill'
    );
    vi.mocked(copyDirectory).mockReturnValue(true);

    const result = installSkillToDestination({
      sourcePath: '/src/my-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'copy',
      force: false,
    });

    expect(vi.mocked(mkdirSync)).toHaveBeenCalledWith(
      '/dest/skills',
      expect.objectContaining({ recursive: true })
    );
    expect(result).toBe('installed');
  });

  it('installs via symlink when mode is symlink', () => {
    vi.mocked(existsSync).mockReturnValue(false);
    vi.mocked(dirExists).mockReturnValue(true);

    const result = installSkillToDestination({
      sourcePath: '/src/my-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'symlink',
      force: false,
    });

    expect(vi.mocked(symlinkSync)).toHaveBeenCalled();
    expect(result).toBe('installed');
  });

  it('returns failed when an error is thrown', () => {
    vi.mocked(existsSync).mockReturnValue(false);
    vi.mocked(dirExists).mockReturnValue(true);
    vi.mocked(copyDirectory).mockImplementation(() => {
      throw new Error('disk full');
    });

    const result = installSkillToDestination({
      sourcePath: '/src/my-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'copy',
      force: false,
    });

    expect(result).toBe('failed');
  });

  it('returns skipped when destination exists and force=false', () => {
    vi.mocked(existsSync).mockReturnValue(true);
    vi.mocked(dirExists).mockReturnValue(true);

    const result = installSkillToDestination({
      sourcePath: '/src/my-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'copy',
      force: false,
    });

    expect(result).toBe('skipped');
  });

  it('removes and reinstalls when destination exists and force=true', () => {
    vi.mocked(existsSync).mockReturnValue(true);
    vi.mocked(dirExists).mockReturnValue(true);
    vi.mocked(copyDirectory).mockReturnValue(true);

    const result = installSkillToDestination({
      sourcePath: '/src/my-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'copy',
      force: true,
    });

    expect(vi.mocked(rmSync)).toHaveBeenCalledWith(
      '/dest/skills/my-skill',
      expect.objectContaining({ recursive: true })
    );
    expect(result).toBe('installed');
  });

  it('returns failed when source does not exist', () => {
    vi.mocked(dirExists).mockReturnValue(false);

    const result = installSkillToDestination({
      sourcePath: '/nonexistent-skill',
      destinationPath: '/dest/skills/my-skill',
      mode: 'copy',
      force: false,
    });

    expect(result).toBe('failed');
  });
});
