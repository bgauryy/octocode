/* eslint-disable @typescript-eslint/no-explicit-any */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import express from 'express';
import request from 'supertest';
import { errorHandler } from '../../middleware/errorHandler.js';

vi.mock('../../mcpCache.js', () => ({
  getMcpContent: vi.fn().mockReturnValue({
    tools: {
      localSearchCode: {
        name: 'localSearchCode',
        description: 'Search local code',
        schema: { type: 'object', properties: { pattern: { type: 'string' } } },
        hints: { hasResults: ['Use lineHint'], empty: ['Try broader search'] },
      },
      githubSearchCode: {
        name: 'githubSearchCode',
        description: 'Search GitHub code',
        schema: { type: 'object', properties: { keywordsToSearch: { type: 'string' } } },
        hints: { hasResults: ['Check results'], empty: ['Try other keywords'] },
      },
    },
    prompts: {
      research: { name: 'Research', description: 'Research', args: [], content: 'test' },
      plan: { name: 'Plan', description: 'Plan', args: [], content: 'test' },
    },
    instructions: 'System instructions for agent behavior',
    baseHints: ['Always follow hints'],
    genericErrorHints: ['Check inputs'],
    baseSchema: { type: 'object' },
  }),
  initializeMcpContent: vi.fn().mockResolvedValue({}),
  isMcpInitialized: vi.fn().mockReturnValue(true),
}));

vi.mock('../../index.js', () => ({
  localSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      files: []\n      totalMatches: 0' }],
  }),
  localGetFileContent: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      path: test.ts\n      content: "hello"' }],
  }),
  localFindFiles: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      files: []' }],
  }),
  localViewStructure: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      structuredOutput: ""' }],
  }),
  githubSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      files: []' }],
  }),
  githubGetFileContent: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      content: "test"' }],
  }),
  githubSearchRepositories: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      repositories: []' }],
  }),
  githubViewRepoStructure: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      structure: {}' }],
  }),
  githubSearchPullRequests: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      pull_requests: []' }],
  }),
  lspGotoDefinition: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      definition: null' }],
  }),
  lspFindReferences: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      references: []' }],
  }),
  lspCallHierarchy: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      calls: []' }],
  }),
  packageSearch: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      packages: []' }],
  }),
  logToolCall: vi.fn().mockResolvedValue(undefined),
  logPromptCall: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/resilience.js', () => ({
  withGitHubResilience: vi.fn(async (fn: () => any) => fn()),
  withLocalResilience: vi.fn(async (fn: () => any) => fn()),
  withLspResilience: vi.fn(async (fn: () => any) => fn()),
  withPackageResilience: vi.fn(async (fn: () => any) => fn()),
}));

vi.mock('../../utils/asyncTimeout.js', () => ({
  fireAndForgetWithTimeout: vi.fn(),
  withTimeout: vi.fn(async (fn: () => any) => fn()),
}));

import { toolsRoutes } from '../../routes/tools.js';

function createApp(): any {
  const app = express();
  app.use(express.json());
  app.use('/tools', toolsRoutes);
  app.use(errorHandler);
  return app;
}

describe('Tools Routes', () => {
  let app: any;

  beforeEach(() => {
    app = createApp();
    vi.clearAllMocks();
  });

  describe('GET /tools/list', () => {
    it('returns 200 with success true', async () => {
      const res = await request(app).get('/tools/list');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
    });

    it('returns all 13 tools', async () => {
      const res = await request(app).get('/tools/list');
      expect(res.body.data.tools).toHaveLength(13);
    });

    it('each tool has name and description', async () => {
      const res = await request(app).get('/tools/list');
      for (const tool of res.body.data.tools) {
        expect(tool).toHaveProperty('name');
        expect(tool).toHaveProperty('description');
        expect(typeof tool.name).toBe('string');
        expect(typeof tool.description).toBe('string');
      }
    });

    it('includes all tool categories', async () => {
      const res = await request(app).get('/tools/list');
      const names = res.body.data.tools.map((t: any) => t.name);
      expect(names).toContain('localSearchCode');
      expect(names).toContain('githubSearchCode');
      expect(names).toContain('lspGotoDefinition');
      expect(names).toContain('packageSearch');
    });

    it('includes hints', async () => {
      const res = await request(app).get('/tools/list');
      expect(res.body.hints).toBeDefined();
      expect(Array.isArray(res.body.hints)).toBe(true);
    });
  });

  describe('GET /tools/info', () => {
    it('returns all tools from MCP content', async () => {
      const res = await request(app).get('/tools/info');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data.toolNames).toContain('localSearchCode');
    });

    it('includes total count', async () => {
      const res = await request(app).get('/tools/info');
      expect(res.body.data.totalTools).toBeGreaterThan(0);
    });

    it('excludes schema by default', async () => {
      const res = await request(app).get('/tools/info');
      for (const tool of res.body.data.tools) {
        expect(tool).not.toHaveProperty('schema');
      }
    });

    it('includes schema when schema=true', async () => {
      const res = await request(app).get('/tools/info').query({ schema: 'true' });
      for (const tool of res.body.data.tools) {
        expect(tool).toHaveProperty('schema');
      }
    });

    it('includes hints when hints=true', async () => {
      const res = await request(app).get('/tools/info').query({ hints: 'true' });
      expect(res.body.data.baseHints).toBeDefined();
      for (const tool of res.body.data.tools) {
        expect(tool).toHaveProperty('hints');
      }
    });
  });

  describe('GET /tools/info/:toolName', () => {
    it('returns tool info for valid tool', async () => {
      const res = await request(app).get('/tools/info/localSearchCode');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data.name).toBe('localSearchCode');
    });

    it('includes inputSchema by default', async () => {
      const res = await request(app).get('/tools/info/localSearchCode');
      expect(res.body.data).toHaveProperty('inputSchema');
    });

    it('includes schema source indicator', async () => {
      const res = await request(app).get('/tools/info/localSearchCode');
      expect(res.body.data._schemaSource).toBeDefined();
    });

    it('includes tool hints by default', async () => {
      const res = await request(app).get('/tools/info/localSearchCode');
      expect(res.body.data).toHaveProperty('toolHints');
    });

    it('excludes schema when schema=false', async () => {
      const res = await request(app).get('/tools/info/localSearchCode').query({ schema: 'false' });
      expect(res.body.data).not.toHaveProperty('inputSchema');
    });

    it('excludes hints when hints=false', async () => {
      const res = await request(app).get('/tools/info/localSearchCode').query({ hints: 'false' });
      expect(res.body.data).not.toHaveProperty('toolHints');
    });

    it('returns 404 for unknown tool', async () => {
      const res = await request(app).get('/tools/info/nonExistentTool');
      expect(res.status).toBe(404);
      expect(res.body.success).toBe(false);
    });

    it('provides helpful hints for unknown tool', async () => {
      const res = await request(app).get('/tools/info/badTool');
      expect(res.body.hints).toBeDefined();
      expect(res.body.hints.some((h: string) => h.includes('badTool'))).toBe(true);
      expect(res.body.hints.some((h: string) => h.includes('Available tools'))).toBe(true);
    });
  });

  describe('GET /tools/metadata', () => {
    it('returns metadata summary', async () => {
      const res = await request(app).get('/tools/metadata');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data).toHaveProperty('instructions');
      expect(res.body.data).toHaveProperty('toolCount');
      expect(res.body.data).toHaveProperty('promptCount');
      expect(res.body.data).toHaveProperty('hasBaseSchema');
    });

    it('returns correct counts', async () => {
      const res = await request(app).get('/tools/metadata');
      expect(res.body.data.toolCount).toBe(2);
      expect(res.body.data.promptCount).toBe(2);
    });
  });

  describe('GET /tools/schemas', () => {
    it('returns all tool schemas', async () => {
      const res = await request(app).get('/tools/schemas');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data).toHaveProperty('totalTools');
      expect(res.body.data).toHaveProperty('schemas');
    });

    it('has 13 schemas (one per tool)', async () => {
      const res = await request(app).get('/tools/schemas');
      expect(res.body.data.totalTools).toBe(13);
    });

    it('each schema is a valid JSON Schema (has properties or anyOf)', async () => {
      const res = await request(app).get('/tools/schemas');
      const schemas = res.body.data.schemas;
      for (const [, schema] of Object.entries(schemas) as [string, any][]) {
        expect(schema).toHaveProperty('$schema');
        const hasProperties = schema.properties && Object.keys(schema.properties).length > 0;
        const hasAnyOf = Array.isArray(schema.anyOf) && schema.anyOf.length > 0;
        expect(hasProperties || hasAnyOf).toBe(true);
      }
    });
  });

  describe('GET /tools/system', () => {
    it('returns system instructions', async () => {
      const res = await request(app).get('/tools/system');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data.instructions).toBe('System instructions for agent behavior');
    });

    it('includes char count and version', async () => {
      const res = await request(app).get('/tools/system');
      expect(res.body.data).toHaveProperty('charCount');
      expect(res.body.data).toHaveProperty('version');
      expect(res.body.data.charCount).toBe('System instructions for agent behavior'.length);
    });
  });

  describe('GET /tools/initContext', () => {
    it('returns combined system prompt and schemas', async () => {
      const res = await request(app).get('/tools/initContext');
      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body).toHaveProperty('system_prompt');
      expect(res.body).toHaveProperty('tools_schema');
      expect(res.body).toHaveProperty('_meta');
    });

    it('system_prompt matches /tools/system content', async () => {
      const res = await request(app).get('/tools/initContext');
      expect(res.body.system_prompt).toBe('System instructions for agent behavior');
    });

    it('tools_schema has all tool schemas', async () => {
      const res = await request(app).get('/tools/initContext');
      const schemaNames = Object.keys(res.body.tools_schema);
      expect(schemaNames.length).toBe(13);
    });

    it('_meta includes prompt char count and tools count', async () => {
      const res = await request(app).get('/tools/initContext');
      expect(res.body._meta.promptCharCount).toBeGreaterThan(0);
      expect(res.body._meta.toolsCount).toBe(13);
      expect(res.body._meta).toHaveProperty('version');
    });
  });

  describe('POST /tools/call/:toolName', () => {
    const validLocalQuery = {
      queries: [{
        id: 'q1',
        researchGoal: 'test search',
        reasoning: 'testing',
        pattern: 'hello',
        path: '/test/project',
      }],
    };

    describe('success path', () => {
      it('executes tool and returns parsed result', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send(validLocalQuery);
        expect(res.status).toBe(200);
        expect(res.body.tool).toBe('localSearchCode');
        expect(res.body.success).toBe(true);
        expect(res.body).toHaveProperty('data');
        expect(res.body).toHaveProperty('hints');
      });

      it('returns research context from query', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send(validLocalQuery);
        expect(res.body).toHaveProperty('research');
      });
    });

    describe('error handling', () => {
      it('returns 404 for unknown tool', async () => {
        const res = await request(app)
          .post('/tools/call/nonExistentTool')
          .send(validLocalQuery);
        expect(res.status).toBe(404);
        expect(res.body.success).toBe(false);
        expect(res.body.hints.some((h: string) => h.includes('nonExistentTool'))).toBe(true);
      });

      it('returns 400 for missing body', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send({});
        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
      });

      it('returns 400 for empty queries array', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send({ queries: [] });
        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
      });

      it('returns 400 for invalid queries format', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send({ queries: 'not-an-array' });
        expect(res.status).toBe(400);
      });

      it('returns 400 for too many queries', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send({
            queries: [
              { id: 'q1', researchGoal: 't', reasoning: 't', pattern: 'a', path: '/p' },
              { id: 'q2', researchGoal: 't', reasoning: 't', pattern: 'b', path: '/p' },
              { id: 'q3', researchGoal: 't', reasoning: 't', pattern: 'c', path: '/p' },
              { id: 'q4', researchGoal: 't', reasoning: 't', pattern: 'd', path: '/p' },
            ],
          });
        expect(res.status).toBe(400);
      });

      it('lists available tools in 404 response', async () => {
        const res = await request(app)
          .post('/tools/call/badTool')
          .send(validLocalQuery);
        expect(res.body.hints.some((h: string) => h.includes('localSearchCode'))).toBe(true);
      });
    });

    describe('bulk queries', () => {
      it('returns bulk format for multiple queries', async () => {
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send({
            queries: [
              { id: 'q1', researchGoal: 't', reasoning: 't', pattern: 'a', path: '/p' },
              { id: 'q2', researchGoal: 't', reasoning: 't', pattern: 'b', path: '/p' },
            ],
          });
        expect(res.status).toBe(200);
        expect(res.body.bulk).toBe(true);
        expect(res.body).toHaveProperty('results');
        expect(res.body).toHaveProperty('counts');
        expect(res.body).toHaveProperty('hints');
      });
    });

    describe('error response from tool', () => {
      it('returns 500 when tool reports error status', async () => {
        const { localSearchCode } = await import('../../index.js');
        vi.mocked(localSearchCode).mockResolvedValueOnce({
          content: [{ type: 'text', text: 'results:\n  - status: error\n    data:\n      message: "Rate limited"' }],
        });
        const res = await request(app)
          .post('/tools/call/localSearchCode')
          .send(validLocalQuery);
        expect(res.status).toBe(500);
        expect(res.body.success).toBe(false);
      });
    });
  });

  describe('Readiness gate', () => {
    it('returns 503 for /tools/list when not initialized', async () => {
      const { isMcpInitialized } = await import('../../mcpCache.js');
      vi.mocked(isMcpInitialized).mockReturnValue(false);

      const res = await request(app).get('/tools/list');
      expect(res.status).toBe(503);
      expect(res.body.error.code).toBe('SERVER_INITIALIZING');

      vi.mocked(isMcpInitialized).mockReturnValue(true);
    });

    it('returns 503 for POST /tools/call when not initialized', async () => {
      const { isMcpInitialized } = await import('../../mcpCache.js');
      vi.mocked(isMcpInitialized).mockReturnValue(false);

      const res = await request(app)
        .post('/tools/call/localSearchCode')
        .send({ queries: [{ id: 'q1', researchGoal: 't', reasoning: 't', pattern: 'a', path: '/p' }] });
      expect(res.status).toBe(503);

      vi.mocked(isMcpInitialized).mockReturnValue(true);
    });

    it('returns 503 for /tools/schemas when not initialized', async () => {
      const { isMcpInitialized } = await import('../../mcpCache.js');
      vi.mocked(isMcpInitialized).mockReturnValue(false);

      const res = await request(app).get('/tools/schemas');
      expect(res.status).toBe(503);

      vi.mocked(isMcpInitialized).mockReturnValue(true);
    });
  });
});
