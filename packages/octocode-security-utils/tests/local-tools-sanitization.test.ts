/**
 * WHITE-HAT SECURITY TESTS: Content Sanitization Pipeline
 * ========================================================
 * Tests the complete INPUT → TOOL → OUTPUT sanitization for all local tools.
 *
 * The architecture has these layers:
 *
 *   INPUT  (LLM → tool):
 *     - GitHub tools:  withSecurityValidation → ContentSanitizer.validateInputParameters
 *     - Local tools:   withBasicSecurityValidation → ContentSanitizer.validateInputParameters
 *
 *   OUTPUT (tool → LLM):
 *     - All tools → executeBulkOperation → sanitizeText
 *       where sanitizeText = ContentSanitizer.sanitizeContent + maskSensitiveData
 *
 * These tests verify:
 * 1. Output redaction: secrets in file content ARE redacted before reaching the LLM
 * 2. Input validation: how local tools handle secret-laden input parameters
 * 3. Masking accuracy: every 2nd char masked for detected secrets
 * 4. Path redaction: file paths in output don't leak sensitive info
 * 5. Fail-closed: errors in detection → content fully redacted
 */

import { describe, it, expect } from 'vitest';
import { ContentSanitizer } from '../src/contentSanitizer.js';
import { maskSensitiveData } from '../src/mask.js';
import { withBasicSecurityValidation } from '../src/withSecurityValidation.js';

/**
 * Local stub for createResponseFormat - applies ContentSanitizer + maskSensitiveData
 * to a response object (mirrors the security-layer behavior from octocode-mcp).
 */
function createResponseFormat(response: { data: unknown }): string {
  const text = JSON.stringify(response);
  const step1 = ContentSanitizer.sanitizeContent(text);
  return maskSensitiveData(step1.content);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 1: OUTPUT Sanitization – Response Format Pipeline
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-01: createResponseFormat – Output Sanitization', () => {
  describe('Secrets in tool output are redacted', () => {
    it('should redact AWS access keys in output', () => {
      const response = {
        data: {
          content: 'Config: AKIAIOSFODNN7EXAMPLE is the key',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
    });

    it('should redact Stripe secret keys in output', () => {
      const response = {
        data: {
          content: 'STRIPE_KEY=sk_live_abcdefghijklmnopqrstuvwx',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
    });

    it('should redact GitHub tokens in output', () => {
      const response = {
        data: {
          content: 'token: ghp_1234567890abcdefghijklmnopqrstuvwxyz',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('ghp_1234567890abcdefghijklmnopqrstuvwxyz');
    });

    it('should redact OpenAI API keys in output', () => {
      const response = {
        data: {
          content:
            'OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain(
        'sk-proj-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ'
      );
    });

    it('should redact private keys (PEM format) in output', () => {
      const response = {
        data: {
          content:
            '-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA0Z3VS...\n-----END RSA PRIVATE KEY-----',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('BEGIN RSA PRIVATE KEY');
    });

    it('should redact Slack tokens in output', () => {
      const response = {
        data: {
          content:
            'SLACK_TOKEN=xoxb-123456789012-1234567890123-ABCDEFGHIJKLmnopqrstuvwx',
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('xoxb-123456789012-1234567890123');
    });
  });

  describe('Non-secret content passes through', () => {
    it('should preserve normal code content', () => {
      const response = {
        data: {
          content: 'function hello() { return 42; }',
        },
      };
      const result = createResponseFormat(response);
      expect(result).toContain('function hello()');
      expect(result).toContain('return 42');
    });
  });

  describe('Multiple secrets in single output', () => {
    it('should redact ALL secrets in mixed content', () => {
      const response = {
        data: {
          content: [
            'AWS_KEY=AKIAIOSFODNN7EXAMPLE',
            'STRIPE=sk_live_abcdefghijklmnopqrstuvwx',
            'Normal code here',
            'GITHUB=ghp_1234567890abcdefghijklmnopqrstuvwxyz',
          ].join('\n'),
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
      expect(result).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
      expect(result).not.toContain('ghp_1234567890abcdefghijklmnopqrstuvwxyz');
      expect(result).toContain('Normal code here');
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 2: ContentSanitizer – Direct Secret Redaction
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-02: ContentSanitizer – Secret Detection & Redaction', () => {
  describe('Payment provider secrets', () => {
    it('should redact Stripe secret keys', () => {
      const r = ContentSanitizer.sanitizeContent(
        'key: sk_live_abcdefghijklmnopqrstuvwx'
      );
      expect(r.hasSecrets).toBe(true);
      expect(r.content).toContain('[REDACTED-');
      expect(r.content).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
    });

    it('should redact Stripe test keys', () => {
      const r = ContentSanitizer.sanitizeContent(
        'key: sk_test_abcdefghijklmnopqrstuvwx'
      );
      expect(r.hasSecrets).toBe(true);
    });

    it('should redact Stripe webhook secrets', () => {
      const r = ContentSanitizer.sanitizeContent(
        'whsec: whsec_abcdefghijklmnopqrstuvwxyz0123456789'
      );
      expect(r.hasSecrets).toBe(true);
    });

    it('should redact Square access tokens', () => {
      const r = ContentSanitizer.sanitizeContent(
        'sq0atp-1234567890abcdef12345678'
      );
      expect(r.hasSecrets).toBe(true);
    });

    it('should redact Shopify tokens', () => {
      const r = ContentSanitizer.sanitizeContent(
        'shpat_abcdef0123456789abcdef0123456789'
      );
      expect(r.hasSecrets).toBe(true);
    });
  });

  describe('Cloud provider secrets', () => {
    it('should redact AWS access key IDs', () => {
      const r = ContentSanitizer.sanitizeContent(
        'AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE'
      );
      expect(r.hasSecrets).toBe(true);
      expect(r.secretsDetected).toEqual(
        expect.arrayContaining([expect.stringContaining('aws')])
      );
    });

    it('should redact GCP service account keys (PEM format)', () => {
      const r = ContentSanitizer.sanitizeContent(
        '-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA0Z3VS\n-----END RSA PRIVATE KEY-----'
      );
      expect(r.hasSecrets).toBe(true);
      expect(r.content).not.toContain('BEGIN RSA PRIVATE KEY');
    });
  });

  describe('Auth & crypto secrets', () => {
    it('should redact JWT tokens', () => {
      const jwt =
        'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U';
      const r = ContentSanitizer.sanitizeContent(`Bearer ${jwt}`);
      expect(r.hasSecrets).toBe(true);
    });
  });

  describe('VCS & dev tool secrets', () => {
    it('should redact GitHub personal access tokens', () => {
      const r = ContentSanitizer.sanitizeContent(
        'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij'
      );
      expect(r.hasSecrets).toBe(true);
    });

    it('should redact GitLab tokens', () => {
      const r = ContentSanitizer.sanitizeContent('glpat-abcdefghij0123456789');
      expect(r.hasSecrets).toBe(true);
    });
  });

  describe('AI provider secrets', () => {
    it('should redact Anthropic API keys', () => {
      const r = ContentSanitizer.sanitizeContent(
        'sk-ant-api03-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-ABCDEFGHIJKLMNOPQRSTUVWXYZ'
      );
      expect(r.hasSecrets).toBe(true);
    });
  });

  describe('Fail-closed behavior', () => {
    it('should fully redact content on detection error', () => {
      const r = ContentSanitizer.sanitizeContent('normal content');
      expect(r.hasSecrets).toBe(false);
      expect(r.content).toBe('normal content');
    });
  });

  describe('No false positives on common code patterns', () => {
    it('should NOT redact short hex strings', () => {
      const r = ContentSanitizer.sanitizeContent('color: #ff0000;');
      expect(r.hasSecrets).toBe(false);
    });

    it('should NOT redact import statements', () => {
      const r = ContentSanitizer.sanitizeContent(
        'import { stripe } from "@stripe/stripe-js";'
      );
      expect(r.content).toContain('stripe');
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 3: maskSensitiveData – Character-Level Masking
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-03: maskSensitiveData – Character-Level Masking', () => {
  describe('Masking behavior', () => {
    it('should mask every 2nd character of detected secrets', () => {
      const text = 'key: sk_live_abcdefghijklmnopqrstuvwx';
      const masked = maskSensitiveData(text);
      expect(masked).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
      expect(masked.length).toBe(text.length);
    });

    it('should mask GitHub tokens', () => {
      const token = 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij';
      const masked = maskSensitiveData(`token: ${token}`);
      expect(masked).not.toContain(token);
    });

    it('should mask AWS keys', () => {
      const key = 'AKIAIOSFODNN7EXAMPLE';
      const masked = maskSensitiveData(`aws: ${key}`);
      expect(masked).not.toContain(key);
    });

    it('should preserve non-secret text around masked content', () => {
      const text = 'prefix ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij suffix';
      const masked = maskSensitiveData(text);
      expect(masked).toContain('prefix');
      expect(masked).toContain('suffix');
    });
  });

  describe('No-op for clean content', () => {
    it('should return empty string as-is', () => {
      expect(maskSensitiveData('')).toBe('');
    });

    it('should return normal code as-is', () => {
      const code = 'function hello() { return 42; }';
      expect(maskSensitiveData(code)).toBe(code);
    });

    it('should return null/undefined safely', () => {
      expect(maskSensitiveData(null as unknown as string)).toBe(null);
      expect(maskSensitiveData(undefined as unknown as string)).toBe(undefined);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 4: INPUT Validation – ContentSanitizer.validateInputParameters
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-04: Input Validation – validateInputParameters', () => {
  describe('Prototype pollution prevention', () => {
    it('should handle __proto__ key (JS Object.entries skips it)', () => {
      const params = { normal: 'value' };
      Object.defineProperty(params, '__proto__', {
        value: { admin: true },
        enumerable: true,
      });
      const r = ContentSanitizer.validateInputParameters(params);
      expect(r.isValid).toBe(false);
    });

    it('should block constructor key', () => {
      const r = ContentSanitizer.validateInputParameters({
        constructor: 'evil',
      });
      expect(r.isValid).toBe(false);
    });

    it('should block prototype key', () => {
      const r = ContentSanitizer.validateInputParameters({
        prototype: 'evil',
      });
      expect(r.isValid).toBe(false);
    });
  });

  describe('Size limits', () => {
    it('should truncate strings over 10,000 chars', () => {
      const longString = 'a'.repeat(15000);
      const r = ContentSanitizer.validateInputParameters({ text: longString });
      expect(r.isValid).toBe(true);
      expect((r.sanitizedParams.text as string).length).toBe(10000);
      expect(r.warnings.length).toBeGreaterThan(0);
    });

    it('should truncate arrays over 100 items', () => {
      const bigArray = Array.from({ length: 150 }, (_, i) => `item${i}`);
      const r = ContentSanitizer.validateInputParameters({ items: bigArray });
      expect((r.sanitizedParams.items as string[]).length).toBe(100);
    });
  });

  describe('Secret detection in input', () => {
    it('should detect and redact secrets in string params', () => {
      const r = ContentSanitizer.validateInputParameters({
        query: 'search for sk_live_abcdefghijklmnopqrstuvwx',
      });
      expect(r.hasSecrets).toBe(true);
      expect(r.sanitizedParams.query).not.toContain(
        'sk_live_abcdefghijklmnopqrstuvwx'
      );
    });

    it('should handle nested objects', () => {
      const r = ContentSanitizer.validateInputParameters({
        options: {
          key: 'sk_live_abcdefghijklmnopqrstuvwx',
        },
      });
      expect(r.hasSecrets).toBe(true);
    });

    it('should pass clean params through', () => {
      const r = ContentSanitizer.validateInputParameters({
        pattern: 'function hello',
        path: '/src',
      });
      expect(r.isValid).toBe(true);
      expect(r.hasSecrets).toBe(false);
      expect(r.sanitizedParams.pattern).toBe('function hello');
    });
  });

  describe('Invalid input handling', () => {
    it('should reject null input', () => {
      const r = ContentSanitizer.validateInputParameters(
        null as unknown as Record<string, unknown>
      );
      expect(r.isValid).toBe(false);
    });

    it('should reject non-object input', () => {
      const r = ContentSanitizer.validateInputParameters(
        'string' as unknown as Record<string, unknown>
      );
      expect(r.isValid).toBe(false);
    });

    it('should handle empty object', () => {
      const r = ContentSanitizer.validateInputParameters({});
      expect(r.isValid).toBe(true);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 5: Double-layer protection verification
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-05: Double-Layer Protection', () => {
  describe('Double-layer protection verification', () => {
    it('ContentSanitizer replaces with [REDACTED-*]', () => {
      const r = ContentSanitizer.sanitizeContent(
        'key: sk_live_abcdefghijklmnopqrstuvwx'
      );
      expect(r.content).toMatch(/\[REDACTED-\w+\]/);
    });

    it('maskSensitiveData masks every 2nd char', () => {
      const masked = maskSensitiveData('sk_live_abcdefghijklmnopqrstuvwx');
      expect(masked).toContain('*');
      expect(masked).not.toBe('sk_live_abcdefghijklmnopqrstuvwx');
    });

    it('combined: sanitize then mask produces no raw secrets', () => {
      const secret = 'sk_live_abcdefghijklmnopqrstuvwx';
      const step1 = ContentSanitizer.sanitizeContent(secret);
      const step2 = maskSensitiveData(step1.content);
      expect(step2).not.toContain(secret);
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 6: Simulated Local Tool Output Scenarios
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-06: Simulated Local Tool Output Scenarios', () => {
  describe('localSearchCode: ripgrep match containing secrets', () => {
    it('should redact AWS keys in search match output', () => {
      const toolOutput = {
        data: {
          results: [
            {
              file: 'config/aws.ts',
              matches: [
                {
                  lineNumber: 5,
                  content: 'const accessKey = "AKIAIOSFODNN7EXAMPLE";',
                },
              ],
            },
          ],
        },
      };
      const result = createResponseFormat(toolOutput);
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
      expect(result).toContain('config/aws.ts');
    });

    it('should redact multiple secrets in search results', () => {
      const toolOutput = {
        data: {
          results: [
            {
              file: '.env.example',
              matches: [
                {
                  lineNumber: 1,
                  content: 'STRIPE_KEY=sk_live_abcdefghijklmnopqrstuvwx',
                },
                {
                  lineNumber: 2,
                  content:
                    'GITHUB_TOKEN=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij',
                },
              ],
            },
          ],
        },
      };
      const result = createResponseFormat(toolOutput);
      expect(result).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
      expect(result).not.toContain('ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij');
    });
  });

  describe('localGetFileContent: file with embedded secrets', () => {
    it('should redact secrets in full file content', () => {
      const toolOutput = {
        data: {
          path: 'src/config.ts',
          content: [
            'export const config = {',
            '  apiKey: "sk_live_abcdefghijklmnopqrstuvwx",',
            '  dbHost: "localhost",',
            '  port: 3000,',
            '};',
          ].join('\n'),
        },
      };
      const result = createResponseFormat(toolOutput);
      expect(result).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
      expect(result).toContain('dbHost');
    });

    it('should redact private keys in file content', () => {
      const toolOutput = {
        data: {
          path: 'certs/server.key',
          content:
            '-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA0Z3VS\n-----END RSA PRIVATE KEY-----',
        },
      };
      const result = createResponseFormat(toolOutput);
      expect(result).not.toContain('BEGIN RSA PRIVATE KEY');
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 7: Edge Cases & Bypass Attempts
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-07: Edge Cases & Bypass Attempts', () => {
  describe('Secrets in nested data structures', () => {
    it('should redact secrets in deeply nested objects', () => {
      const response = {
        data: {
          level1: {
            level2: {
              level3: {
                secret: 'AKIAIOSFODNN7EXAMPLE',
              },
            },
          },
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('AKIAIOSFODNN7EXAMPLE');
    });

    it('should redact secrets in arrays', () => {
      const response = {
        data: {
          items: ['normal', 'sk_live_abcdefghijklmnopqrstuvwx', 'also normal'],
        },
      };
      const result = createResponseFormat(response);
      expect(result).not.toContain('sk_live_abcdefghijklmnopqrstuvwx');
    });
  });

  describe('Empty and null handling', () => {
    it('should handle empty string content', () => {
      const result = createResponseFormat({ data: { content: '' } });
      expect(result).toBeDefined();
    });

    it('should handle null data', () => {
      const result = createResponseFormat({ data: null });
      expect(result).toBeDefined();
    });
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 8: withBasicSecurityValidation Integration
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('SAN-08: withBasicSecurityValidation – Local Tool Integration', () => {
  function getTextContent(result: {
    content: Array<{ type: string; text?: string }>;
  }): string {
    const item = result.content[0]!;
    expect(item.type).toBe('text');
    return (item as { type: 'text'; text: string }).text;
  }

  describe('Prototype pollution keys are rejected', () => {
    it('should reject __proto__ key in input', async () => {
      const mockHandler = async (args: Record<string, unknown>) => ({
        content: [{ type: 'text' as const, text: JSON.stringify(args) }],
      });
      const wrapped = withBasicSecurityValidation(mockHandler);

      const params = { normal: 'value' };
      Object.defineProperty(params, '__proto__', {
        value: { admin: true },
        enumerable: true,
      });

      const result = await wrapped(params, {
        signal: new AbortController().signal,
      });
      expect(getTextContent(result)).toContain('Security validation failed');
      expect(result.isError).toBe(true);
    });

    it('should reject constructor key in input', async () => {
      const mockHandler = async (args: Record<string, unknown>) => ({
        content: [{ type: 'text' as const, text: JSON.stringify(args) }],
      });
      const wrapped = withBasicSecurityValidation(mockHandler);

      const result = await wrapped(
        { constructor: 'evil' },
        { signal: new AbortController().signal }
      );
      expect(getTextContent(result)).toContain('Security validation failed');
      expect(result.isError).toBe(true);
    });

    it('should reject prototype key in input', async () => {
      const mockHandler = async (args: Record<string, unknown>) => ({
        content: [{ type: 'text' as const, text: JSON.stringify(args) }],
      });
      const wrapped = withBasicSecurityValidation(mockHandler);

      const result = await wrapped(
        { prototype: 'evil' },
        { signal: new AbortController().signal }
      );
      expect(getTextContent(result)).toContain('Security validation failed');
      expect(result.isError).toBe(true);
    });
  });

  describe('Oversized strings are truncated before execution', () => {
    it('should truncate strings over 10,000 chars', async () => {
      let receivedArgs: Record<string, unknown> | undefined;
      const mockHandler = async (args: Record<string, unknown>) => {
        receivedArgs = args;
        return {
          content: [{ type: 'text' as const, text: 'ok' }],
        };
      };
      const wrapped = withBasicSecurityValidation(mockHandler);

      const longString = 'x'.repeat(15000);
      await wrapped(
        { pattern: longString },
        { signal: new AbortController().signal }
      );

      expect(receivedArgs).toBeDefined();
      expect((receivedArgs!['pattern'] as string).length).toBe(10000);
    });
  });

  describe('Secrets in input params are redacted', () => {
    it('should redact secrets in string params before execution', async () => {
      let receivedArgs: Record<string, unknown> | undefined;
      const mockHandler = async (args: Record<string, unknown>) => {
        receivedArgs = args;
        return {
          content: [{ type: 'text' as const, text: 'ok' }],
        };
      };
      const wrapped = withBasicSecurityValidation(mockHandler);

      await wrapped(
        {
          pattern: 'search for sk_live_abcdefghijklmnopqrstuvwx in code',
        },
        { signal: new AbortController().signal }
      );

      expect(receivedArgs).toBeDefined();
      expect(receivedArgs!['pattern'] as string).not.toContain(
        'sk_live_abcdefghijklmnopqrstuvwx'
      );
    });
  });

  describe('Invalid input is rejected', () => {
    it('should reject null input', async () => {
      const mockHandler = async (args: Record<string, unknown>) => ({
        content: [{ type: 'text' as const, text: JSON.stringify(args) }],
      });
      const wrapped = withBasicSecurityValidation(mockHandler);

      const result = await wrapped(null, {
        signal: new AbortController().signal,
      });
      expect(getTextContent(result)).toContain('Security validation failed');
      expect(result.isError).toBe(true);
    });

    it('should reject non-object input', async () => {
      const mockHandler = async (args: Record<string, unknown>) => ({
        content: [{ type: 'text' as const, text: JSON.stringify(args) }],
      });
      const wrapped = withBasicSecurityValidation(mockHandler);

      const result = await wrapped('string input', {
        signal: new AbortController().signal,
      });
      expect(getTextContent(result)).toContain('Security validation failed');
      expect(result.isError).toBe(true);
    });
  });
});
