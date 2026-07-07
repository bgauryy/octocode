import { describe, it, expect } from 'vitest';
import { createResponseFormat } from '../../../octocode-tools-core/src/responses.js';
import { executeBulkOperation } from '../../../octocode-tools-core/src/utils/response/bulk.js';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { maskSensitiveData } from 'octocode-security/mask';
import type { ProcessedBulkResult } from '../../../octocode-tools-core/src/types/toolResults.js';

const SECRETS = {
  AWS_KEY: 'AKIAIOSFODNN7EXAMPLE',
  STRIPE_KEY: 'sk_live_abcdefghijklmnopqrstuvwx',
  GITHUB_TOKEN: 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij',
  OPENAI_KEY: 'sk-proj-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ',
  PRIVATE_KEY:
    '-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEA0Z3VS\n-----END RSA PRIVATE KEY-----',
  SLACK_TOKEN: 'xoxb-123456789012-1234567890123-ABCDEFGHIJKLmnopqrstuvwx',
  GITLAB_TOKEN: 'glpat-abcdefghij0123456789',
  ANTHROPIC_KEY:
    'sk-ant-api03-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-ABCDEFGHIJKLMNOPQRSTUVWXYZ',
} as const;

function assertNoSecrets(obj: unknown, path = 'root'): void {
  if (obj === null || obj === undefined) return;

  if (typeof obj === 'string') {
    for (const [name, secret] of Object.entries(SECRETS)) {
      expect(obj, `Secret ${name} leaked at ${path}`).not.toContain(secret);
    }
    return;
  }

  if (Array.isArray(obj)) {
    obj.forEach((item, i) => assertNoSecrets(item, `${path}[${i}]`));
    return;
  }

  if (typeof obj === 'object') {
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      assertNoSecrets(value, `${path}.${key}`);
    }
  }
}

describe('GAP-01: structuredContent sanitization in bulk responses', () => {
  it('should sanitize secrets in structuredContent from executeBulkOperation', async () => {
    const queries = [{ id: 'q1', path: '/test' }];

    const processor = async (): Promise<ProcessedBulkResult> => ({
      content: `Config: ${SECRETS.AWS_KEY} and ${SECRETS.STRIPE_KEY}`,
      data: {
        file: 'config.ts',
        matches: [
          { line: 1, content: `const key = "${SECRETS.GITHUB_TOKEN}";` },
        ],
      },
    });

    const result = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
    });

    const textContent = result.content[0];
    expect(textContent?.type).toBe('text');
    if (textContent && 'text' in textContent) {
      assertNoSecrets(textContent.text, 'text');
    }

    if (result.structuredContent) {
      assertNoSecrets(result.structuredContent, 'structuredContent');
    }
  });

  it('should sanitize secrets in structuredContent across multiple query results', async () => {
    const queries = [
      { id: 'q1', path: '/a' },
      { id: 'q2', path: '/b' },
    ];

    const processor = async (
      _query: { id: string; path: string },
      index: number
    ): Promise<ProcessedBulkResult> => ({
      content:
        index === 0
          ? `AWS: ${SECRETS.AWS_KEY}`
          : `Stripe: ${SECRETS.STRIPE_KEY}`,
    });

    const result = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
    });

    if (result.structuredContent) {
      assertNoSecrets(result.structuredContent, 'structuredContent');
    }
  });

  it('should sanitize private keys in structuredContent', async () => {
    const queries = [{ id: 'q1' }];

    const processor = async (): Promise<ProcessedBulkResult> => ({
      content: SECRETS.PRIVATE_KEY,
    });

    const result = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
    });

    if (result.structuredContent) {
      assertNoSecrets(result.structuredContent, 'structuredContent');
    }
  });
});

describe('GAP-02: Local file content sanitization', () => {
  it('should sanitize file content in bulk result before reaching createResponseFormat', () => {
    const fileContent = [
      'export const config = {',
      `  awsKey: "${SECRETS.AWS_KEY}",`,
      `  stripeKey: "${SECRETS.STRIPE_KEY}",`,
      `  ghToken: "${SECRETS.GITHUB_TOKEN}",`,
      '  host: "localhost",',
      '  port: 3000,',
      '};',
    ].join('\n');

    const response = {
      results: [
        {
          id: 'q1',
          data: {
            content: fileContent,
            path: '/workspace/config.ts',
            totalLines: 7,
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
    expect(text).toContain('localhost');
    expect(text).toContain('3000');
  });

  it('should sanitize matchString results containing secrets', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            content: `const secret = "${SECRETS.OPENAI_KEY}";`,
            path: '/workspace/src/api.ts',
            startLine: 10,
            endLine: 12,
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });
});

describe('GAP-03: Ripgrep output sanitization', () => {
  it('should sanitize secrets in ripgrep match results', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            matches: [
              {
                file: '.env',
                line: 1,
                content: `AWS_ACCESS_KEY_ID=${SECRETS.AWS_KEY}`,
              },
              {
                file: '.env',
                line: 2,
                content: `STRIPE_SECRET_KEY=${SECRETS.STRIPE_KEY}`,
              },
              {
                file: 'config.js',
                line: 5,
                content: `const token = "${SECRETS.GITHUB_TOKEN}";`,
              },
            ],
            totalMatches: 3,
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
    expect(text).toContain('.env');
    expect(text).toContain('config.js');
  });

  it('should sanitize secrets in ripgrep context lines', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            matches: [
              {
                file: 'deploy.sh',
                content: [
                  '#!/bin/bash',
                  `export AWS_KEY="${SECRETS.AWS_KEY}"`,
                  `export SLACK="${SECRETS.SLACK_TOKEN}"`,
                  'echo "deploying..."',
                ].join('\n'),
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
    expect(text).toContain('deploying');
  });
});

describe('GAP-04: LSP tool output sanitization', () => {
  it('should sanitize secrets in goto definition code snippets', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            locations: [
              {
                uri: '/workspace/src/config.ts',
                range: {
                  start: { line: 5, character: 0 },
                  end: { line: 5, character: 20 },
                },
                content: [
                  '  4| import { config } from "./base";',
                  `> 5| export const API_KEY = "${SECRETS.OPENAI_KEY}";`,
                  '  6| export const HOST = "localhost";',
                ].join('\n'),
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
    expect(text).toContain('localhost');
  });

  it('should sanitize secrets in find references results', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            references: [
              {
                uri: '/workspace/src/api.ts',
                line: 10,
                content: `  fetch(url, { headers: { Authorization: "Bearer ${SECRETS.GITHUB_TOKEN}" } })`,
              },
            ],
            totalReferences: 1,
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });

  it('should sanitize secrets in call hierarchy context', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            target: {
              name: 'getSecret',
              uri: '/workspace/src/secrets.ts',
              content: `function getSecret() { return "${SECRETS.STRIPE_KEY}"; }`,
            },
            incomingCalls: [
              {
                from: { name: 'init', uri: '/workspace/src/app.ts' },
                content: `  const key = getSecret(); // ${SECRETS.AWS_KEY}`,
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });
});

describe('GAP-05: PR patch content sanitization', () => {
  it('should sanitize secrets in PR file change patches', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            number: 42,
            title: 'Update config',
            file_changes: [
              {
                filename: '.env.production',
                status: 'modified',
                additions: 1,
                deletions: 1,
                patch: [
                  '@@ -1,3 +1,3 @@',
                  `-AWS_KEY=old_key`,
                  `+AWS_KEY=${SECRETS.AWS_KEY}`,
                  ` DB_HOST=localhost`,
                ].join('\n'),
              },
              {
                filename: 'config/stripe.ts',
                status: 'modified',
                additions: 1,
                deletions: 0,
                patch: `+export const STRIPE_KEY = "${SECRETS.STRIPE_KEY}";`,
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
    expect(text).toContain('.env.production');
    expect(text).toContain('config/stripe.ts');
  });

  it('should sanitize secrets in PR commit details', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            number: 99,
            title: 'Fix auth',
            commit_details: [
              {
                sha: 'abc123',
                message: `fix: rotate API key to ${SECRETS.GITHUB_TOKEN}`,
                files: [
                  {
                    filename: 'auth.ts',
                    patch: `+const token = "${SECRETS.OPENAI_KEY}";`,
                  },
                ],
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });
});

describe('GAP-06: Error message masking', () => {
  it('should mask secrets in error messages via createResponseFormat', () => {
    const response = {
      results: [
        {
          id: 'q1',
          status: 'error' as const,
          data: {
            error: `Authentication failed with token ${SECRETS.GITHUB_TOKEN}`,
            _sanitization_warnings: [
              `Failed to connect with key ${SECRETS.AWS_KEY}`,
              `Retry with token ${SECRETS.STRIPE_KEY}`,
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });

  it('should mask secrets in provider error messages', () => {
    const response = {
      data: {
        error: `Request to https://api.example.com failed: invalid token ${SECRETS.GITLAB_TOKEN}`,
      },
    };

    const text = createResponseFormat(response);
    assertNoSecrets(text, 'text');
  });
});

describe('CROSS: Every secret type through bulk pipeline', () => {
  for (const [secretName, secretValue] of Object.entries(SECRETS)) {
    it(`should sanitize ${secretName} in both text and structuredContent`, async () => {
      const queries = [{ id: 'q1' }];

      const processor = async (): Promise<ProcessedBulkResult> => ({
        content: `secret: ${secretValue}`,
        nested: { deep: { value: secretValue } },
      });

      const result = await executeBulkOperation(queries, processor, {
        toolName: 'testTool',
      });

      const textContent = result.content[0];
      if (textContent && 'text' in textContent) {
        expect(textContent.text, `${secretName} leaked in text`).not.toContain(
          secretValue
        );
      }

      if (result.structuredContent) {
        assertNoSecrets(
          result.structuredContent,
          `structuredContent[${secretName}]`
        );
      }
    });
  }
});

describe('SAFE: Clean content preserved through all paths', () => {
  it('should preserve normal code through bulk pipeline text', async () => {
    const queries = [{ id: 'q1' }];
    const cleanCode =
      'function calculateTotal(items) { return items.reduce((sum, i) => sum + i.price, 0); }';

    const processor = async (): Promise<ProcessedBulkResult> => ({
      content: cleanCode,
    });

    const result = await executeBulkOperation(queries, processor, {
      toolName: 'testTool',
    });

    const textContent = result.content[0];
    if (textContent && 'text' in textContent) {
      expect(textContent.text).toContain('calculateTotal');
      expect(textContent.text).toContain('items');
    }
  });

  it('should preserve normal code through createResponseFormat', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            content: 'const greeting = "Hello, World!";',
            path: '/workspace/src/hello.ts',
            line: 1,
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    expect(text).toContain('Hello, World');
    expect(text).toContain('hello.ts');
  });

  it('should preserve file paths and line numbers through sanitization', () => {
    const response = {
      results: [
        {
          id: 'q1',
          data: {
            locations: [
              {
                uri: '/workspace/src/components/Button.tsx',
                range: { start: { line: 42, character: 0 } },
              },
            ],
          },
        },
      ],
    };

    const text = createResponseFormat(response);
    expect(text).toContain('Button.tsx');
    expect(text).toContain('42');
  });
});

describe('INPUT: All secret types blocked in tool input', () => {
  for (const [secretName, secretValue] of Object.entries(SECRETS)) {
    it(`should redact ${secretName} from input parameters`, () => {
      const result = ContentSanitizer.validateInputParameters({
        query: `search for ${secretValue} in code`,
      });

      expect(result.hasSecrets, `${secretName} not detected in input`).toBe(
        true
      );
      const sanitizedQuery = result.sanitizedParams.query as string;
      expect(
        sanitizedQuery,
        `${secretName} leaked in sanitized input`
      ).not.toContain(secretValue);
    });
  }

  it('should redact secrets in nested query objects', () => {
    const result = ContentSanitizer.validateInputParameters({
      queries: [
        {
          pattern: SECRETS.AWS_KEY,
          context: { token: SECRETS.GITHUB_TOKEN },
        },
      ],
    });

    expect(result.hasSecrets).toBe(true);
    assertNoSecrets(result.sanitizedParams, 'sanitizedParams');
  });

  it('should reject prototype pollution attempts', () => {
    const params = { normal: 'value' };
    Object.defineProperty(params, '__proto__', {
      value: { admin: true },
      enumerable: true,
    });

    const result = ContentSanitizer.validateInputParameters(params);
    expect(result.isValid).toBe(false);
  });
});

describe('DOUBLE-LAYER: ContentSanitizer + maskSensitiveData combined', () => {
  for (const [secretName, secretValue] of Object.entries(SECRETS)) {
    it(`should catch ${secretName} through both sanitization layers`, () => {
      const input = `data: ${secretValue}`;
      const step1 = ContentSanitizer.sanitizeContent(input);
      const step2 = maskSensitiveData(step1.content);
      expect(step2, `${secretName} survived both layers`).not.toContain(
        secretValue
      );
    });
  }
});
