import { describe, it, expect } from 'vitest';
import os from 'os';
import { redactPath } from '../src/pathUtils.js';

const HOME = os.homedir();

describe('redactPath', () => {
  describe('workspace-relative paths', () => {
    it('should return project-relative path for file inside workspace', () => {
      expect(redactPath('/app/project/src/index.ts', '/app/project')).toBe(
        'src/index.ts'
      );
    });

    it('should return "." when path equals workspace root', () => {
      expect(redactPath('/app/project', '/app/project')).toBe('.');
    });

    it('should handle deeply nested paths', () => {
      expect(
        redactPath('/app/project/src/components/ui/Button.tsx', '/app/project')
      ).toBe('src/components/ui/Button.tsx');
    });

    it('should not match prefix-colliding paths', () => {
      const result = redactPath('/app/project2/file.ts', '/app/project');
      expect(result).not.toBe('2/file.ts');
    });
  });

  describe('home-relative paths', () => {
    it('should show ~/... for paths inside home directory', () => {
      const result = redactPath(`${HOME}/.config/secrets.json`, '/app/other');
      expect(result).toBe('~/.config/secrets.json');
    });

    it('should show ~ for the home directory itself', () => {
      const result = redactPath(HOME, '/app/other');
      expect(result).toBe('~');
    });
  });

  describe('outside-all-roots fallback', () => {
    it('should return filename only for paths outside workspace and home', () => {
      const result = redactPath('/opt/system/config.yaml', HOME + '/project');
      expect(result).toBe('config.yaml');
    });
  });

  describe('edge cases', () => {
    it('should return empty string for empty input', () => {
      expect(redactPath('')).toBe('');
    });

    it('should return empty string for falsy input', () => {
      expect(redactPath(undefined as unknown as string)).toBe('');
    });

    it('should normalize backslashes', () => {
      expect(redactPath('/app/project\\src\\index.ts', '/app/project')).toBe(
        'src/index.ts'
      );
    });

    it('should normalize double slashes', () => {
      expect(redactPath('/app/project//src//index.ts', '/app/project')).toBe(
        'src/index.ts'
      );
    });

    it('should handle trailing slashes in workspace root', () => {
      expect(redactPath('/app/project/src/file.ts', '/app/project/')).toBe(
        'src/file.ts'
      );
    });
  });
});
