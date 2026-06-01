/**
 * Configuration Validator Tests
 */

import { describe, it, expect } from 'vitest';
import { validateConfig } from '../../src/config/validator.js';
import {
  MIN_TIMEOUT,
  MAX_TIMEOUT,
  MIN_RETRIES,
  MAX_RETRIES,
} from '../../src/config/defaults.js';

describe('config/validator', () => {
  describe('validateConfig', () => {
    it('validates empty config as valid', () => {
      const result = validateConfig({});
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('validates complete valid config', () => {
      const result = validateConfig({
        version: 1,
        github: {
          apiUrl: 'https://api.github.com',
        },
        local: {
          enabled: true,
          allowedPaths: ['/home/user/projects'],
        },
        tools: {
          enabled: ['githubSearchCode'],
          disabled: ['packageSearch'],
        },
        network: {
          timeout: 30000,
          maxRetries: 3,
        },
        telemetry: {
          logging: true,
        },
        lsp: {
          configPath: '~/.octocode/lsp-servers.json',
        },
        output: {
          format: 'yaml',
          pagination: {
            defaultCharLength: 8000,
          },
        },
      });

      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('rejects non-object config', () => {
      expect(validateConfig('string').valid).toBe(false);
      expect(validateConfig(123).valid).toBe(false);
      expect(validateConfig(null).valid).toBe(false);
      expect(validateConfig([]).valid).toBe(false);
    });

    describe('version validation', () => {
      it('accepts valid version', () => {
        const result = validateConfig({ version: 1 });
        expect(result.valid).toBe(true);
      });

      it('rejects non-integer version', () => {
        const result = validateConfig({ version: 1.5 });
        expect(result.valid).toBe(false);
        expect(result.errors).toContain('version: Must be an integer');
      });

      it('warns about newer version', () => {
        const result = validateConfig({ version: 999 });
        expect(result.valid).toBe(true);
        expect(
          result.warnings.some(w => w.includes('newer than supported'))
        ).toBe(true);
      });
    });

    describe('github validation', () => {
      it('rejects invalid apiUrl', () => {
        const result = validateConfig({
          github: { apiUrl: 'not-a-url' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('github.apiUrl'))).toBe(true);
      });

      it('rejects non-http/https URL', () => {
        const result = validateConfig({
          github: { apiUrl: 'ftp://api.github.com' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('Only http/https'))).toBe(
          true
        );
      });
    });

    describe('local validation', () => {
      it('rejects non-boolean enabled', () => {
        const result = validateConfig({
          local: { enabled: 'yes' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('local.enabled'))).toBe(true);
      });

      it('rejects non-array allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: '/path' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('local.allowedPaths'))).toBe(
          true
        );
      });
    });

    describe('tools validation', () => {
      it('accepts null enabled/disabled', () => {
        const result = validateConfig({
          tools: { enabled: null, disabled: null },
        });
        expect(result.valid).toBe(true);
      });

      it('rejects non-array enabled', () => {
        const result = validateConfig({
          tools: { enabled: 'githubSearchCode' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('tools.enabled'))).toBe(true);
      });

      it('accepts null enableAdditional', () => {
        const result = validateConfig({
          tools: { enableAdditional: null },
        });
        expect(result.valid).toBe(true);
      });

      it('rejects non-array enableAdditional', () => {
        const result = validateConfig({
          tools: { enableAdditional: 'localSearchCode' },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('tools.enableAdditional'))
        ).toBe(true);
      });

      it('accepts valid enableAdditional array', () => {
        const result = validateConfig({
          tools: { enableAdditional: ['localSearchCode', 'lspGotoDefinition'] },
        });
        expect(result.valid).toBe(true);
      });

      it('rejects non-array disabled', () => {
        const result = validateConfig({
          tools: { disabled: 'packageSearch' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('tools.disabled'))).toBe(
          true
        );
      });

      it('accepts valid disabled array', () => {
        const result = validateConfig({
          tools: { disabled: ['packageSearch'] },
        });
        expect(result.valid).toBe(true);
      });

      it('rejects non-object tools', () => {
        const result = validateConfig({ tools: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('tools: Must be an object'))
        ).toBe(true);
      });

      it('rejects array tools', () => {
        const result = validateConfig({ tools: [] });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('tools: Must be an object'))
        ).toBe(true);
      });
    });

    describe('network validation', () => {
      it('rejects timeout below minimum', () => {
        const result = validateConfig({
          network: { timeout: MIN_TIMEOUT - 1 },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.timeout'))).toBe(
          true
        );
      });

      it('rejects timeout above maximum', () => {
        const result = validateConfig({
          network: { timeout: MAX_TIMEOUT + 1 },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.timeout'))).toBe(
          true
        );
      });

      it('rejects maxRetries below minimum', () => {
        const result = validateConfig({
          network: { maxRetries: MIN_RETRIES - 1 },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.maxRetries'))).toBe(
          true
        );
      });

      it('rejects maxRetries above maximum', () => {
        const result = validateConfig({
          network: { maxRetries: MAX_RETRIES + 1 },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.maxRetries'))).toBe(
          true
        );
      });

      it('rejects non-number timeout', () => {
        const result = validateConfig({
          network: { timeout: '30000' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('Must be a number'))).toBe(
          true
        );
      });
    });

    describe('telemetry validation', () => {
      it('rejects non-boolean logging', () => {
        const result = validateConfig({
          telemetry: { logging: 'true' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('telemetry.logging'))).toBe(
          true
        );
      });
    });

    describe('lsp validation', () => {
      it('rejects non-string configPath', () => {
        const result = validateConfig({
          lsp: { configPath: 123 },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('lsp.configPath'))).toBe(
          true
        );
      });
    });

    describe('output validation', () => {
      it('rejects invalid output format', () => {
        const result = validateConfig({
          output: { format: 'xml' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('output.format'))).toBe(true);
      });

      it('rejects invalid output pagination defaultCharLength', () => {
        const result = validateConfig({
          output: { pagination: { defaultCharLength: 999999 } },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e =>
            e.includes('output.pagination.defaultCharLength')
          )
        ).toBe(true);
      });

      it('accepts valid output pagination config', () => {
        const result = validateConfig({
          output: { pagination: { defaultCharLength: 12000 } },
        });
        expect(result.valid).toBe(true);
      });
    });

    describe('local validation (extended)', () => {
      it('rejects non-string items in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: [123, '/valid/path'] },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('local.allowedPaths'))).toBe(
          true
        );
      });

      it('rejects non-object local', () => {
        const result = validateConfig({ local: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('local: Must be an object'))
        ).toBe(true);
      });

      it('rejects array local', () => {
        const result = validateConfig({ local: [] });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('local: Must be an object'))
        ).toBe(true);
      });
    });

    describe('allowedPaths element validation', () => {
      it('rejects empty string in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['', '/valid/path'] },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e =>
            e.includes('local.allowedPaths[0]: empty or whitespace-only')
          )
        ).toBe(true);
      });

      it('rejects whitespace-only string in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['   '] },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e =>
            e.includes('local.allowedPaths[0]: empty or whitespace-only')
          )
        ).toBe(true);
      });

      it('rejects relative path in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['foo/bar'] },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e =>
            e.includes('must be absolute path or start with ~')
          )
        ).toBe(true);
      });

      it('rejects path traversal in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['/valid/../etc'] },
        });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('path traversal (..) not allowed'))
        ).toBe(true);
      });

      it('accepts valid absolute path in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['/Users/me/code'] },
        });
        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('accepts valid tilde path in allowedPaths', () => {
        const result = validateConfig({
          local: { allowedPaths: ['~/Documents'] },
        });
        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('reports multiple path errors at once', () => {
        const result = validateConfig({
          local: { allowedPaths: ['', 'relative', '/good/../bad'] },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.length).toBe(3);
      });

      it('accepts empty allowedPaths array', () => {
        const result = validateConfig({
          local: { allowedPaths: [] },
        });
        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });
    });

    describe('github validation (extended)', () => {
      it('rejects non-object github', () => {
        const result = validateConfig({ github: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('github: Must be an object'))
        ).toBe(true);
      });

      it('rejects array github', () => {
        const result = validateConfig({ github: [] });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('github: Must be an object'))
        ).toBe(true);
      });
    });

    describe('network validation (extended)', () => {
      it('rejects non-object network', () => {
        const result = validateConfig({ network: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('network: Must be an object'))
        ).toBe(true);
      });

      it('rejects NaN timeout', () => {
        const result = validateConfig({
          network: { timeout: NaN },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.timeout'))).toBe(
          true
        );
      });

      it('rejects NaN maxRetries', () => {
        const result = validateConfig({
          network: { maxRetries: NaN },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.some(e => e.includes('network.maxRetries'))).toBe(
          true
        );
      });

      it('accepts boundary values for timeout', () => {
        const minResult = validateConfig({
          network: { timeout: MIN_TIMEOUT },
        });
        expect(minResult.valid).toBe(true);

        const maxResult = validateConfig({
          network: { timeout: MAX_TIMEOUT },
        });
        expect(maxResult.valid).toBe(true);
      });

      it('accepts boundary values for maxRetries', () => {
        const minResult = validateConfig({
          network: { maxRetries: MIN_RETRIES },
        });
        expect(minResult.valid).toBe(true);

        const maxResult = validateConfig({
          network: { maxRetries: MAX_RETRIES },
        });
        expect(maxResult.valid).toBe(true);
      });
    });

    describe('telemetry validation (extended)', () => {
      it('rejects non-object telemetry', () => {
        const result = validateConfig({ telemetry: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('telemetry: Must be an object'))
        ).toBe(true);
      });

      it('accepts valid boolean logging', () => {
        expect(validateConfig({ telemetry: { logging: true } }).valid).toBe(
          true
        );
        expect(validateConfig({ telemetry: { logging: false } }).valid).toBe(
          true
        );
      });
    });

    describe('lsp validation (extended)', () => {
      it('accepts valid configPath string', () => {
        const result = validateConfig({
          lsp: { configPath: '/path/to/lsp.json' },
        });
        expect(result.valid).toBe(true);
      });

      it('accepts null configPath', () => {
        const result = validateConfig({
          lsp: { configPath: null },
        });
        expect(result.valid).toBe(true);
      });

      it('rejects non-object lsp', () => {
        const result = validateConfig({ lsp: 'invalid' });
        expect(result.valid).toBe(false);
        expect(
          result.errors.some(e => e.includes('lsp: Must be an object'))
        ).toBe(true);
      });
    });

    describe('$schema key', () => {
      it('allows $schema key without warning', () => {
        const result = validateConfig({
          $schema: 'https://octocode.dev/schemas/octocoderc.json',
          version: 1,
        });
        expect(result.valid).toBe(true);
        expect(result.warnings).toHaveLength(0);
      });
    });

    describe('multiple errors', () => {
      it('reports all validation errors at once', () => {
        const result = validateConfig({
          github: { apiUrl: 'not-a-url' },
          network: { timeout: -1, maxRetries: 999 },
          telemetry: { logging: 'yes' },
        });
        expect(result.valid).toBe(false);
        expect(result.errors.length).toBeGreaterThanOrEqual(4);
      });
    });

    describe('unknown keys', () => {
      it('warns about unknown top-level keys', () => {
        const result = validateConfig({
          version: 1,
          unknownKey: 'value',
        });
        expect(result.valid).toBe(true);
        expect(
          result.warnings.some(w =>
            w.includes('Unknown configuration key: unknownKey')
          )
        ).toBe(true);
      });

      it('warns about multiple unknown keys', () => {
        const result = validateConfig({
          unknownA: 1,
          unknownB: 2,
        });
        expect(result.valid).toBe(true);
        expect(result.warnings).toHaveLength(2);
      });
    });

    describe('returned config on success', () => {
      it('returns config object when valid', () => {
        const input = {
          version: 1,
          github: { apiUrl: 'https://api.github.com' },
        };
        const result = validateConfig(input);
        expect(result.valid).toBe(true);
        expect(result.config).toBeDefined();
        expect(result.config?.version).toBe(1);
      });

      it('returns undefined config when invalid', () => {
        const result = validateConfig({
          github: { apiUrl: 'not-a-url' },
        });
        expect(result.valid).toBe(false);
        expect(result.config).toBeUndefined();
      });
    });
  });
});
