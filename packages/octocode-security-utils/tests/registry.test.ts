/**
 * Tests for SecurityRegistry extensibility.
 * These tests run in the security package where source imports share singletons.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ContentSanitizer } from '../src/contentSanitizer.js';
import { maskSensitiveData } from '../src/mask.js';
import { validateCommand } from '../src/commandValidator.js';
import {
  shouldIgnorePath,
  shouldIgnoreFile,
} from '../src/ignoredPathFilter.js';
import { securityRegistry, SecurityRegistry } from '../src/registry.js';

describe('SecurityRegistry', () => {
  beforeEach(() => {
    securityRegistry.reset();
  });

  afterEach(() => {
    securityRegistry.reset();
  });

  describe('custom secret patterns', () => {
    it('should detect custom patterns via ContentSanitizer.sanitizeContent', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'customCorpToken',
          description: 'Custom corp token',
          regex: /\bMYCORP_[A-Z0-9]{32}\b/g,
          matchAccuracy: 'high',
        },
      ]);

      const secret = 'MYCORP_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345';
      const result = ContentSanitizer.sanitizeContent(`Token: ${secret}`);
      expect(result.hasSecrets).toBe(true);
      expect(result.content).not.toContain(secret);
      expect(result.content).toContain('[REDACTED-CUSTOMCORPTOKEN]');
      expect(result.secretsDetected).toContain('customCorpToken');
    });

    it('should mask custom patterns via maskSensitiveData', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'customInternalKey',
          description: 'Internal API key',
          regex: /\bINT_KEY_[a-f0-9]{40}\b/g,
          matchAccuracy: 'high',
        },
      ]);

      const secret = 'INT_KEY_' + 'a'.repeat(40);
      const masked = maskSensitiveData(`Key: ${secret}`);
      expect(masked).not.toContain(secret);
    });

    it('should not detect custom patterns after reset', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'ephemeral',
          description: 'temp',
          regex: /\bTEMP_[A-Z]{20}\b/g,
          matchAccuracy: 'high',
        },
      ]);

      const secret = 'TEMP_ABCDEFGHIJKLMNOPQRST';
      expect(ContentSanitizer.sanitizeContent(secret).hasSecrets).toBe(true);

      securityRegistry.reset();

      expect(ContentSanitizer.sanitizeContent(secret).hasSecrets).toBe(false);
    });

    it('should merge with built-in patterns, not replace them', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'myPattern',
          description: 'custom',
          regex: /\bCUSTOM_[A-Z]{10}\b/g,
          matchAccuracy: 'high',
        },
      ]);

      const awsKey = 'AKIAIOSFODNN7EXAMPLE';
      const result = ContentSanitizer.sanitizeContent(`key: ${awsKey}`);
      expect(result.hasSecrets).toBe(true);
    });
  });

  describe('custom allowed commands', () => {
    it('should allow user-registered commands', () => {
      expect(validateCommand('jq', ['.foo']).isValid).toBe(false);
      securityRegistry.addAllowedCommands(['jq']);
      expect(validateCommand('jq', ['.foo']).isValid).toBe(true);
    });

    it('should not allow removed commands after reset', () => {
      securityRegistry.addAllowedCommands(['yq']);
      expect(validateCommand('yq', ['.']).isValid).toBe(true);
      securityRegistry.reset();
      expect(validateCommand('yq', ['.']).isValid).toBe(false);
    });

    it('should not duplicate commands when added twice', () => {
      securityRegistry.addAllowedCommands(['jq']);
      securityRegistry.addAllowedCommands(['jq']);
      expect(securityRegistry.extraAllowedCommands.length).toBe(1);
    });

    it('should still block built-in disallowed commands', () => {
      securityRegistry.addAllowedCommands(['jq']);
      expect(validateCommand('rm', ['-rf', '/']).isValid).toBe(false);
    });
  });

  describe('custom ignored path patterns', () => {
    it('should block user-registered path patterns', () => {
      expect(shouldIgnorePath('.vault')).toBe(false);
      securityRegistry.addIgnoredPathPatterns([/^\.vault$/]);
      expect(shouldIgnorePath('.vault')).toBe(true);
    });

    it('should not block after reset', () => {
      securityRegistry.addIgnoredPathPatterns([/^\.vault$/]);
      expect(shouldIgnorePath('.vault')).toBe(true);
      securityRegistry.reset();
      expect(shouldIgnorePath('.vault')).toBe(false);
    });
  });

  describe('custom ignored file patterns', () => {
    it('should block user-registered file patterns', () => {
      expect(shouldIgnoreFile('internal-secrets.yml')).toBe(false);
      securityRegistry.addIgnoredFilePatterns([/^internal[-_]secrets\.ya?ml$/]);
      expect(shouldIgnoreFile('internal-secrets.yml')).toBe(true);
    });

    it('should not block after reset', () => {
      securityRegistry.addIgnoredFilePatterns([/^internal[-_]secrets\.ya?ml$/]);
      expect(shouldIgnoreFile('internal-secrets.yml')).toBe(true);
      securityRegistry.reset();
      expect(shouldIgnoreFile('internal-secrets.yml')).toBe(false);
    });
  });

  describe('validation', () => {
    it('should reject empty pattern names', () => {
      expect(() =>
        securityRegistry.addSecretPatterns([
          {
            name: '',
            description: 'bad',
            regex: /x/g,
            matchAccuracy: 'high',
          },
        ])
      ).toThrow();
    });

    it('should reject empty command strings', () => {
      expect(() => securityRegistry.addAllowedCommands([''])).toThrow();
    });

    it('should reject patterns without regex', () => {
      expect(() =>
        securityRegistry.addSecretPatterns([
          {
            name: 'bad',
            description: 'bad',
          } as any,
        ])
      ).toThrow();
    });
  });

  describe('reset', () => {
    it('should clear all extensions at once', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'temp',
          description: 'temp',
          regex: /x/g,
          matchAccuracy: 'high',
        },
      ]);
      securityRegistry.addAllowedCommands(['custom-cmd']);
      securityRegistry.addAllowedRoots(['/tmp/myapp']);
      securityRegistry.addIgnoredPathPatterns([/^\.x$/]);
      securityRegistry.addIgnoredFilePatterns([/^x\.key$/]);

      expect(securityRegistry.extraSecretPatterns.length).toBe(1);
      expect(securityRegistry.extraAllowedCommands.length).toBe(1);
      expect(securityRegistry.extraAllowedRoots.length).toBe(1);
      expect(securityRegistry.extraIgnoredPathPatterns.length).toBe(1);
      expect(securityRegistry.extraIgnoredFilePatterns.length).toBe(1);

      securityRegistry.reset();

      expect(securityRegistry.extraSecretPatterns.length).toBe(0);
      expect(securityRegistry.extraAllowedCommands.length).toBe(0);
      expect(securityRegistry.extraAllowedRoots.length).toBe(0);
      expect(securityRegistry.extraIgnoredPathPatterns.length).toBe(0);
      expect(securityRegistry.extraIgnoredFilePatterns.length).toBe(0);
    });
  });

  describe('exported from index', () => {
    it('should export SecurityRegistry class and singleton', async () => {
      const mod = await import('../src/index.js');
      expect(mod.SecurityRegistry).toBe(SecurityRegistry);
      expect(mod.securityRegistry).toBe(securityRegistry);
    });
  });
});
