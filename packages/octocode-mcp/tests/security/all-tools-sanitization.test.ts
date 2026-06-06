import { describe, it, expect, vi } from 'vitest';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { withOutputSanitization } from '../../src/utils/secureServer.js';

const SECRETS = {
  AWS_KEY: 'AKIAIOSFODNN7EXAMPLE',
  STRIPE_KEY: 'sk_live_abcdefghijklmnopqrstuvwx',
  GITHUB_TOKEN: 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef01234567',
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

const ALL_SECRET_VALUES = Object.values(SECRETS);
function secretPayload(prefix: string): string {
  return ALL_SECRET_VALUES.map((s, i) => `${prefix}[${i}]: ${s}`).join('\n');
}

function createProxyChain() {
  const handlers = new Map<
    string,
    (...args: unknown[]) => Promise<CallToolResult>
  >();

  const mockServer = {
    registerTool: vi.fn(
      (
        name: string,
        _opts: unknown,
        cb: (...args: unknown[]) => Promise<CallToolResult>
      ) => {
        handlers.set(name, cb);
      }
    ),
  } as unknown as McpServer;

  const proxy = withOutputSanitization(mockServer);

  async function registerAndCall(
    toolName: string,
    handler: (...args: unknown[]) => Promise<CallToolResult>
  ): Promise<CallToolResult> {
    proxy.registerTool(toolName, {} as never, handler as never);
    const wrapped = handlers.get(toolName)!;
    return wrapped({});
  }

  return { proxy, mockServer, handlers, registerAndCall };
}

const TOOL_RESULT_SHAPES: Record<string, () => CallToolResult> = {
  // GitHub tools
  githubSearchCode: () => ({
    content: [
      {
        type: 'text',
        text: `Found 3 matches:\n- .env: AWS_KEY=${SECRETS.AWS_KEY}\n- config.ts: token="${SECRETS.GITHUB_TOKEN}"`,
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              matches: [
                {
                  file: '.env',
                  content: `AWS_ACCESS_KEY_ID=${SECRETS.AWS_KEY}`,
                  matchLocations: [{ start: 20, end: 40 }],
                },
                {
                  file: 'config.ts',
                  content: `const token = "${SECRETS.GITHUB_TOKEN}";`,
                },
              ],
              totalMatches: 2,
            },
          },
        ],
      },
    },
  }),

  githubGetFileContent: () => ({
    content: [
      {
        type: 'text',
        text: [
          'File: config/secrets.ts',
          `export const STRIPE = "${SECRETS.STRIPE_KEY}";`,
          `export const OPENAI = "${SECRETS.OPENAI_KEY}";`,
          `export const SLACK = "${SECRETS.SLACK_TOKEN}";`,
          SECRETS.PRIVATE_KEY,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              content: `const stripe = "${SECRETS.STRIPE_KEY}";\nconst openai = "${SECRETS.OPENAI_KEY}";`,
              path: 'config/secrets.ts',
              totalLines: 5,
            },
          },
        ],
      },
    },
  }),

  githubViewRepoStructure: () => ({
    content: [
      {
        type: 'text',
        text: `repo structure:\n  .env (contains ${SECRETS.AWS_KEY})\n  src/\n    api.ts`,
      },
    ],
    structuredContent: {
      data: {
        tree: [
          {
            path: '.env',
            content: `DB_URL=postgres://user:${SECRETS.STRIPE_KEY}@host/db`,
          },
          { path: 'src/api.ts', content: 'clean' },
        ],
      },
    },
  }),

  githubSearchRepositories: () => ({
    content: [
      {
        type: 'text',
        text: `Repo: my-project\nDescription: API key is ${SECRETS.OPENAI_KEY}`,
      },
    ],
    structuredContent: {
      data: {
        repositories: [
          {
            name: 'my-project',
            description: `Uses ${SECRETS.ANTHROPIC_KEY} for AI`,
            url: 'https://github.com/org/my-project',
          },
        ],
      },
    },
  }),

  githubSearchPullRequests: () => ({
    content: [
      {
        type: 'text',
        text: [
          'PR #42: Rotate secrets',
          `- Patch: +AWS_KEY=${SECRETS.AWS_KEY}`,
          `- Commit: "set token to ${SECRETS.GITHUB_TOKEN}"`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              number: 42,
              title: 'Rotate secrets',
              file_changes: [
                {
                  filename: '.env',
                  patch: `+AWS_KEY=${SECRETS.AWS_KEY}\n+STRIPE=${SECRETS.STRIPE_KEY}`,
                },
              ],
              commit_details: [
                {
                  sha: 'abc',
                  message: `rotate token to ${SECRETS.GITLAB_TOKEN}`,
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
      },
    },
  }),

  packageSearch: () => ({
    content: [
      {
        type: 'text',
        text: `Package: stripe-sdk\nREADME: Use your key ${SECRETS.STRIPE_KEY} to authenticate`,
      },
    ],
    structuredContent: {
      data: {
        packages: [
          {
            name: 'stripe-sdk',
            readme: `Configure with API key: ${SECRETS.STRIPE_KEY}`,
          },
        ],
      },
    },
  }),

  githubCloneRepo: () => ({
    content: [
      {
        type: 'text',
        text: `Cloned repo. Found file with secret: ${SECRETS.PRIVATE_KEY}`,
      },
    ],
    structuredContent: {
      clonePath: '/tmp/repos/my-project',
      summary: `Repository contains ${SECRETS.GITHUB_TOKEN} in .env`,
    },
  }),

  // Local tools
  localSearchCode: () => ({
    content: [
      {
        type: 'text',
        text: [
          'Search results:',
          `.env:1: API_KEY=${SECRETS.AWS_KEY}`,
          `config.ts:5: const token = "${SECRETS.SLACK_TOKEN}";`,
          `deploy.sh:3: export GITLAB="${SECRETS.GITLAB_TOKEN}"`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              matches: [
                {
                  file: '.env',
                  line: 1,
                  content: `API_KEY=${SECRETS.AWS_KEY}`,
                },
                {
                  file: 'config.ts',
                  line: 5,
                  content: `const token = "${SECRETS.SLACK_TOKEN}";`,
                },
              ],
              totalMatches: 2,
            },
          },
        ],
      },
    },
  }),

  localGetFileContent: () => ({
    content: [
      {
        type: 'text',
        text: [
          'File: /workspace/config/env.ts',
          '1| import { config } from "./base";',
          `2| export const AWS = "${SECRETS.AWS_KEY}";`,
          `3| export const STRIPE = "${SECRETS.STRIPE_KEY}";`,
          `4| export const ANTHROPIC = "${SECRETS.ANTHROPIC_KEY}";`,
          `5| ${SECRETS.PRIVATE_KEY}`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              content: secretPayload('line'),
              path: '/workspace/config/env.ts',
              totalLines: 10,
            },
          },
        ],
      },
    },
  }),

  localViewStructure: () => ({
    content: [
      {
        type: 'text',
        text: `Directory tree:\n  .env.local (${SECRETS.OPENAI_KEY})\n  src/\n    index.ts`,
      },
    ],
    structuredContent: {
      data: {
        tree: {
          name: 'project',
          children: [
            { name: '.env.local', content: `KEY=${SECRETS.OPENAI_KEY}` },
          ],
        },
      },
    },
  }),

  localFindFiles: () => ({
    content: [
      {
        type: 'text',
        text: `Found files:\n  secrets.json (size: 1024) contains ${SECRETS.GITHUB_TOKEN}\n  .env.production`,
      },
    ],
    structuredContent: {
      data: {
        files: [
          {
            path: 'secrets.json',
            preview: `{"token":"${SECRETS.GITHUB_TOKEN}"}`,
          },
          { path: '.env.production', preview: `STRIPE=${SECRETS.STRIPE_KEY}` },
        ],
      },
    },
  }),

  // LSP tools
  lspGotoDefinition: () => ({
    content: [
      {
        type: 'text',
        text: [
          'Definition found:',
          '  File: /workspace/src/config.ts:5',
          `  > export const API_KEY = "${SECRETS.OPENAI_KEY}";`,
          `  > export const DB_PASS = "${SECRETS.STRIPE_KEY}";`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              locations: [
                {
                  uri: '/workspace/src/config.ts',
                  range: { start: { line: 5 }, end: { line: 5 } },
                  content: `export const API_KEY = "${SECRETS.OPENAI_KEY}";`,
                },
              ],
            },
          },
        ],
      },
    },
  }),

  lspFindReferences: () => ({
    content: [
      {
        type: 'text',
        text: [
          'References found: 3',
          `  src/api.ts:10 — fetch(url, { headers: { Authorization: "Bearer ${SECRETS.GITHUB_TOKEN}" }})`,
          `  src/stripe.ts:5 — stripe(${SECRETS.STRIPE_KEY})`,
          `  src/aws.ts:3 — new AWS({ key: "${SECRETS.AWS_KEY}" })`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
        results: [
          {
            id: 'q1',
            data: {
              references: [
                {
                  uri: '/workspace/src/api.ts',
                  line: 10,
                  content: `fetch(url, { headers: { Authorization: "Bearer ${SECRETS.GITHUB_TOKEN}" }})`,
                },
                {
                  uri: '/workspace/src/stripe.ts',
                  line: 5,
                  content: `stripe("${SECRETS.STRIPE_KEY}")`,
                },
              ],
              totalReferences: 3,
            },
          },
        ],
      },
    },
  }),

  lspCallHierarchy: () => ({
    content: [
      {
        type: 'text',
        text: [
          'Call hierarchy for getSecret():',
          `  target: function getSecret() { return "${SECRETS.STRIPE_KEY}"; }`,
          `  ← init() calls getSecret() // ${SECRETS.AWS_KEY}`,
          `  ← connect() // token=${SECRETS.ANTHROPIC_KEY}`,
        ].join('\n'),
      },
    ],
    structuredContent: {
      data: {
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
                  content: `const key = getSecret(); // ${SECRETS.AWS_KEY}`,
                },
              ],
            },
          },
        ],
      },
    },
  }),
};

describe('ALL-TOOLS: Unified output sanitization via withOutputSanitization proxy', () => {
  describe('Per-tool realistic result sanitization', () => {
    for (const [toolName, resultFactory] of Object.entries(
      TOOL_RESULT_SHAPES
    )) {
      it(`${toolName}: no secrets in content[] text after proxy sanitization`, async () => {
        const { registerAndCall } = createProxyChain();
        const handler = vi.fn().mockResolvedValue(resultFactory());
        const result = await registerAndCall(toolName, handler);

        for (const item of result.content) {
          if (item.type === 'text' && 'text' in item) {
            assertNoSecrets(item.text, `${toolName}.content[].text`);
          }
        }
      });

      it(`${toolName}: no secrets in structuredContent after proxy sanitization`, async () => {
        const { registerAndCall } = createProxyChain();
        const handler = vi.fn().mockResolvedValue(resultFactory());
        const result = await registerAndCall(toolName, handler);

        if (result.structuredContent) {
          assertNoSecrets(
            result.structuredContent,
            `${toolName}.structuredContent`
          );
        }
      });
    }
  });

  describe('Cross-cutting: every secret type through every tool', () => {
    for (const [secretName, secretValue] of Object.entries(SECRETS)) {
      it(`${secretName}: redacted in content[] across all 14 tools`, async () => {
        for (const toolName of Object.keys(TOOL_RESULT_SHAPES)) {
          const { registerAndCall } = createProxyChain();
          const handler = vi.fn().mockResolvedValue({
            content: [{ type: 'text', text: `toolOutput: ${secretValue}` }],
          } satisfies CallToolResult);

          const result = await registerAndCall(toolName, handler);
          const text = (result.content[0] as { type: 'text'; text: string })
            .text;
          expect(
            text,
            `${secretName} leaked in ${toolName} content[]`
          ).not.toContain(secretValue);
        }
      });

      it(`${secretName}: redacted in structuredContent across all 14 tools`, async () => {
        for (const toolName of Object.keys(TOOL_RESULT_SHAPES)) {
          const { registerAndCall } = createProxyChain();
          const handler = vi.fn().mockResolvedValue({
            content: [],
            structuredContent: {
              data: { nested: { deep: secretValue } },
              arrayField: [secretValue, 'safe'],
            },
          } satisfies CallToolResult);

          const result = await registerAndCall(toolName, handler);
          assertNoSecrets(
            result.structuredContent,
            `${secretName} in ${toolName}.structuredContent`
          );
        }
      });
    }
  });

  describe('Proxy chain integrity', () => {
    it('all 14 tools register through the proxy', () => {
      const { mockServer, proxy } = createProxyChain();

      for (const toolName of Object.keys(TOOL_RESULT_SHAPES)) {
        proxy.registerTool(toolName, {} as never, (() => {}) as never);
      }

      expect(mockServer.registerTool).toHaveBeenCalledTimes(14);
    });

    it('tool names are forwarded correctly to the real server', () => {
      const { mockServer, proxy } = createProxyChain();
      const toolNames = Object.keys(TOOL_RESULT_SHAPES);

      for (const toolName of toolNames) {
        proxy.registerTool(toolName, {} as never, (() => {}) as never);
      }

      const calls = (mockServer.registerTool as ReturnType<typeof vi.fn>).mock
        .calls;
      for (let i = 0; i < toolNames.length; i++) {
        expect(calls[i]?.[0]).toBe(toolNames[i]);
      }
    });

    it('original handler is called exactly once per invocation', async () => {
      const { registerAndCall } = createProxyChain();

      for (const [toolName, resultFactory] of Object.entries(
        TOOL_RESULT_SHAPES
      )) {
        const handler = vi.fn().mockResolvedValue(resultFactory());
        await registerAndCall(toolName, handler);
        expect(
          handler,
          `${toolName} handler call count`
        ).toHaveBeenCalledOnce();
      }
    });

    it('arguments are forwarded to the original handler', async () => {
      const { proxy, handlers } = createProxyChain();

      const handler = vi.fn().mockResolvedValue({
        content: [{ type: 'text', text: 'ok' }],
      } satisfies CallToolResult);

      proxy.registerTool('testTool', {} as never, handler as never);
      const wrapped = handlers.get('testTool')!;

      const args = { query: 'test', owner: 'org' };
      const extra = { authInfo: { token: 'tok' }, sessionId: 'sid-1' };
      await wrapped(args, extra);

      expect(handler).toHaveBeenCalledWith(args, extra);
    });
  });

  describe('Error results with secrets', () => {
    for (const [toolName] of Object.entries(TOOL_RESULT_SHAPES)) {
      it(`${toolName}: error isError=true with secrets is sanitized`, async () => {
        const { registerAndCall } = createProxyChain();
        const handler = vi.fn().mockResolvedValue({
          content: [
            {
              type: 'text',
              text: `Error: Auth failed with token ${SECRETS.GITHUB_TOKEN} for key ${SECRETS.AWS_KEY}`,
            },
          ],
          isError: true,
        } satisfies CallToolResult);

        const result = await registerAndCall(toolName, handler);
        expect(result.isError).toBe(true);
        const text = (result.content[0] as { type: 'text'; text: string }).text;
        assertNoSecrets(text, `${toolName}.error.content`);
      });
    }
  });

  describe('Mixed content types', () => {
    it('text items sanitized, non-text items preserved', async () => {
      const { registerAndCall } = createProxyChain();
      const imageItem = {
        type: 'image' as const,
        data: 'base64data',
        mimeType: 'image/png',
      };

      const handler = vi.fn().mockResolvedValue({
        content: [
          { type: 'text', text: `secret: ${SECRETS.OPENAI_KEY}` },
          imageItem,
          { type: 'text', text: `another: ${SECRETS.STRIPE_KEY}` },
        ],
      } satisfies CallToolResult);

      const result = await registerAndCall('githubSearchCode', handler);
      assertNoSecrets(
        (result.content[0] as { type: 'text'; text: string }).text,
        'first text'
      );
      expect(result.content[1]).toBe(imageItem);
      assertNoSecrets(
        (result.content[2] as { type: 'text'; text: string }).text,
        'second text'
      );
    });
  });

  describe('Clean content preservation', () => {
    for (const toolName of Object.keys(TOOL_RESULT_SHAPES)) {
      it(`${toolName}: clean results pass through unmodified`, async () => {
        const { registerAndCall } = createProxyChain();
        const cleanResult: CallToolResult = {
          content: [
            {
              type: 'text',
              text: `function calculateTotal(items) {\n  return items.reduce((sum, i) => sum + i.price, 0);\n}`,
            },
          ],
          structuredContent: {
            data: {
              count: 42,
              name: 'test-project',
              tags: ['typescript', 'clean'],
            },
          },
        };

        const handler = vi.fn().mockResolvedValue(cleanResult);
        const result = await registerAndCall(toolName, handler);

        const text = (result.content[0] as { type: 'text'; text: string }).text;
        expect(text).toContain('calculateTotal');
        expect(text).toContain('items');

        const sc = result.structuredContent as Record<string, unknown>;
        expect((sc.data as Record<string, unknown>).count).toBe(42);
        expect((sc.data as Record<string, unknown>).name).toBe('test-project');
      });
    }
  });
});
