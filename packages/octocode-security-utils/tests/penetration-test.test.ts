/**
 * SECURITY PENETRATION TEST
 * Comprehensive attack testing for pathValidator
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PathValidator } from '../src/pathValidator.js';

describe('🔴 SECURITY PENETRATION TEST - PathValidator', () => {
  const mockWorkspace = '/Users/octopus';
  let validator: PathValidator;

  beforeEach(() => {
    validator = new PathValidator({ workspaceRoot: mockWorkspace });
  });

  describe('ATTACK 1: Path Traversal Variations', () => {
    it('should BLOCK classic ../ traversal', () => {
      const result = validator.validate('/Users/octopus/../../../etc/passwd');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('outside allowed directories');
    });

    it('should BLOCK deep traversal (10 levels)', () => {
      const path = '/Users/octopus' + '/..'.repeat(10) + '/etc';
      const result = validator.validate(path);
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK mixed ./ and ../', () => {
      const result = validator.validate(
        '/Users/octopus/./Documents/.././.././.././etc'
      );
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK trailing slash traversal', () => {
      const result = validator.validate('/Users/octopus/../../../etc/');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 2: Encoding Attacks', () => {
    it('URL encoded dots (%2e%2e) - SAFE (Node treats as literal)', () => {
      // Node.js path.resolve() treats %2e as literal characters, not encoded dots
      // This creates a directory named "%2e%2e" (literal), not ".."
      const result = validator.validate('/Users/octopus/%2e%2e/%2e%2e/etc');
      // This is ALLOWED because it stays within workspace
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toContain('%2e%2e');
      // Verify it doesn't escape workspace
      expect(result.sanitizedPath?.startsWith('/Users/octopus')).toBe(true);
    });

    it('should BLOCK backslash on Unix', () => {
      const result = validator.validate('/Users/octopus\\..\\..\\etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK mixed slashes and backslashes', () => {
      const result = validator.validate('/Users/octopus\\..//..\\\\etc');
      expect(result.isValid).toBe(false);
    });

    it('Double URL encoding (%252e) - SAFE (Node treats as literal)', () => {
      // Same as above - literal %252e characters, not double-encoded dots
      const result = validator.validate('/Users/octopus/%252e%252e/etc');
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath?.startsWith('/Users/octopus')).toBe(true);
    });
  });

  describe('ATTACK 3: Special Characters', () => {
    it('should BLOCK null byte injection', () => {
      const result = validator.validate('/Users/octopus\x00/../../../etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK newline injection', () => {
      const result = validator.validate('/Users/octopus\n/../../../etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK tab character', () => {
      const result = validator.validate('/Users/octopus\t/../../../etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK carriage return', () => {
      const result = validator.validate('/Users/octopus\r/../../../etc');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 4: Path Normalization Tricks', () => {
    it('should BLOCK multiple consecutive slashes', () => {
      const result = validator.validate('/Users////octopus/////..//..//etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK excessive ./.', () => {
      const result = validator.validate('/Users/./octopus/./././././../../etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK mixed redundant components', () => {
      const result = validator.validate('/Users/./octopus/./.././.././etc');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 5: Case Sensitivity Bypass (macOS)', () => {
    it('should BLOCK uppercase USERS', () => {
      const result = validator.validate('/USERS/octopus/../../../etc');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK mixed case UsErS', () => {
      const result = validator.validate('/UsErS/octopus/../../../etc');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 6: Sibling Directory Bypass', () => {
    it('should BLOCK sibling with similar name', () => {
      const result = validator.validate('/Users/octopus2');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('outside allowed directories');
    });

    it('should BLOCK sibling with extra characters', () => {
      const result = validator.validate('/Users/octopus_other');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK sibling with prefix match', () => {
      const result = validator.validate('/Users/octopusextra/Documents');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 7: Absolute System Paths', () => {
    const systemPaths = [
      '/etc/passwd',
      '/usr/bin',
      '/tmp',
      '/var/log',
      '/root',
      '/System',
      '/Library',
    ];

    systemPaths.forEach(systemPath => {
      it(`should BLOCK access to ${systemPath}`, () => {
        const result = validator.validate(systemPath);
        expect(result.isValid).toBe(false);
        expect(result.error).toContain('outside allowed directories');
      });
    });
  });

  describe('ATTACK 8: Home Directory Bypass', () => {
    it('should BLOCK parent of allowed root (/Users)', () => {
      const result = validator.validate('/Users');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK root directory (/)', () => {
      const result = validator.validate('/');
      expect(result.isValid).toBe(false);
    });

    it('should BLOCK other user home', () => {
      const result = validator.validate('/Users/root');
      expect(result.isValid).toBe(false);
    });
  });

  describe('ATTACK 9: Unicode & Homoglyphs', () => {
    it('should BLOCK Unicode fraction slash (⁄)', () => {
      const result = validator.validate('/Users/octopus⁄..⁄..⁄etc');
      expect(result.isValid).toBe(false);
    });

    it('Full-width dots (．．) - SAFE (not normalized to ASCII dots)', () => {
      // Unicode full-width dots are NOT normalized to ASCII dots by Node.js
      // They are kept as literal Unicode characters, creating a directory named "．．"
      const result = validator.validate('/Users/octopus/．．/etc');
      // This is ALLOWED because it stays within workspace
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toContain('．');
      // Verify it doesn't escape workspace
      expect(result.sanitizedPath?.startsWith('/Users/octopus')).toBe(true);
    });

    it('should BLOCK RTL override character', () => {
      const result = validator.validate('/Users/octopus\u202e/../../etc');
      expect(result.isValid).toBe(false);
    });
  });

  describe('VALID PATHS - Should ALLOW', () => {
    it('should ALLOW valid workspace subdirectory', () => {
      const result = validator.validate('/Users/octopus/Documents');
      expect(result.isValid).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('should ALLOW workspace root', () => {
      const result = validator.validate('/Users/octopus');
      expect(result.isValid).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('should ALLOW deeply nested valid paths', () => {
      const result = validator.validate(
        '/Users/octopus/Documents/projects/app/src/components'
      );
      expect(result.isValid).toBe(true);
    });
  });
});
