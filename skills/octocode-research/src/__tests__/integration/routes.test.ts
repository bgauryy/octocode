/* eslint-disable @typescript-eslint/no-explicit-any */

import { describe, it, expect, vi } from 'vitest';
import express from 'express';
import request from 'supertest';
import { localRoutes } from '../../routes/local.js';
import { githubRoutes } from '../../routes/github.js';
import { lspRoutes } from '../../routes/lsp.js';
import { packageRoutes } from '../../routes/package.js';

vi.mock('../../mcpCache.js', () => ({
  getMcpContent: vi.fn().mockReturnValue({
    tools: {},
    prompts: {},
    instructions: 'Test instructions',
    baseHints: [],
    genericErrorHints: [],
  }),
  initializeMcpContent: vi.fn().mockResolvedValue({}),
  isMcpInitialized: vi.fn().mockReturnValue(true),
}));

vi.mock('../../index.js', () => ({
  localSearchCode: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      files: []\n      totalMatches: 0' }],
  }),
  localGetFileContent: vi.fn().mockResolvedValue({
    content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      path: test.ts\n      content: "test"' }],
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
}));

function createApp(): any {
  const app = express();
  app.use(express.json());
  app.use('/', localRoutes);
  app.use('/', githubRoutes);
  app.use('/', lspRoutes);
  app.use('/', packageRoutes);
  return app;
}

describe('Route Validation', () => {
  describe('Local Routes', () => {
    describe('GET /localSearchCode', () => {
      it('validates required pattern parameter', async () => {
        const res = await request(createApp()).get('/localSearchCode');
        expect(res.status).toBe(400);
      });

      it('accepts valid search request', async () => {
        const res = await request(createApp())
          .get('/localSearchCode')
          .query({ pattern: 'test', path: '/test' });
        expect(res.status).toBe(200);
        expect(res.body).toHaveProperty('content');
      });

      it('returns proper response structure', async () => {
        const res = await request(createApp())
          .get('/localSearchCode')
          .query({ pattern: 'test', path: '/test' });
        expect(res.body).toHaveProperty('content');
        expect(Array.isArray(res.body.content)).toBe(true);
      });
    });

    describe('GET /localGetFileContent', () => {
      it('validates required path parameter', async () => {
        const res = await request(createApp()).get('/localGetFileContent');
        expect(res.status).toBe(400);
      });

      it('accepts valid content request', async () => {
        const res = await request(createApp())
          .get('/localGetFileContent')
          .query({ path: '/test/file.ts' });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /localFindFiles', () => {
      it('validates required path parameter', async () => {
        const res = await request(createApp()).get('/localFindFiles');
        expect(res.status).toBe(400);
      });

      it('accepts valid find request', async () => {
        const res = await request(createApp())
          .get('/localFindFiles')
          .query({ path: '/test' });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /localViewStructure', () => {
      it('validates required path parameter', async () => {
        const res = await request(createApp()).get('/localViewStructure');
        expect(res.status).toBe(400);
      });

      it('accepts valid structure request', async () => {
        const res = await request(createApp())
          .get('/localViewStructure')
          .query({ path: '/test' });
        expect(res.status).toBe(200);
      });
    });
  });

  describe('GitHub Routes', () => {
    describe('GET /githubSearchCode', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/githubSearchCode');
        expect(res.status).toBe(400);
      });

      it('accepts valid search request', async () => {
        const res = await request(createApp())
          .get('/githubSearchCode')
          .query({
            keywordsToSearch: 'test',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /githubGetFileContent', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/githubGetFileContent');
        expect(res.status).toBe(400);
      });

      it('accepts valid content request', async () => {
        const res = await request(createApp())
          .get('/githubGetFileContent')
          .query({
            owner: 'test',
            repo: 'test',
            path: 'test.ts',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /githubSearchRepositories', () => {
      it('validates required keywords or topics', async () => {
        const res = await request(createApp())
          .get('/githubSearchRepositories')
          .query({
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(400);
      });

      it('accepts valid repos request with keywords', async () => {
        const res = await request(createApp())
          .get('/githubSearchRepositories')
          .query({
            keywordsToSearch: 'test-keyword',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });

      it('accepts valid repos request with topics', async () => {
        const res = await request(createApp())
          .get('/githubSearchRepositories')
          .query({
            topicsToSearch: 'test-topic',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /githubViewRepoStructure', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/githubViewRepoStructure');
        expect(res.status).toBe(400);
      });

      it('accepts valid structure request', async () => {
        const res = await request(createApp())
          .get('/githubViewRepoStructure')
          .query({
            owner: 'test',
            repo: 'test',
            branch: 'main',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /githubSearchPullRequests', () => {
      it('accepts valid PRs request', async () => {
        const res = await request(createApp())
          .get('/githubSearchPullRequests')
          .query({
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });
  });

  describe('LSP Routes', () => {
    describe('GET /lspGotoDefinition', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/lspGotoDefinition');
        expect(res.status).toBe(400);
      });

      it('accepts valid definition request', async () => {
        const res = await request(createApp())
          .get('/lspGotoDefinition')
          .query({
            uri: 'file:///test.ts',
            symbolName: 'test',
            lineHint: 1,
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /lspFindReferences', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/lspFindReferences');
        expect(res.status).toBe(400);
      });

      it('accepts valid references request', async () => {
        const res = await request(createApp())
          .get('/lspFindReferences')
          .query({
            uri: 'file:///test.ts',
            symbolName: 'test',
            lineHint: 1,
          });
        expect(res.status).toBe(200);
      });
    });

    describe('GET /lspCallHierarchy', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/lspCallHierarchy');
        expect(res.status).toBe(400);
      });

      it('accepts valid calls request', async () => {
        const res = await request(createApp())
          .get('/lspCallHierarchy')
          .query({
            uri: 'file:///test.ts',
            symbolName: 'test',
            lineHint: 1,
            direction: 'incoming',
          });
        expect(res.status).toBe(200);
      });
    });
  });

  describe('Package Routes', () => {
    describe('GET /packageSearch', () => {
      it('validates required parameters', async () => {
        const res = await request(createApp()).get('/packageSearch');
        expect(res.status).toBe(400);
      });

      it('accepts valid npm package search', async () => {
        const res = await request(createApp())
          .get('/packageSearch')
          .query({
            name: 'express',
            ecosystem: 'npm',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });

      it('accepts valid python package search', async () => {
        const res = await request(createApp())
          .get('/packageSearch')
          .query({
            name: 'requests',
            ecosystem: 'python',
            mainResearchGoal: 'test',
            researchGoal: 'test',
            reasoning: 'test',
          });
        expect(res.status).toBe(200);
      });
    });
  });

  describe('Response Structure', () => {
    it('includes content array in all responses', async () => {
      const res = await request(createApp())
        .get('/localSearchCode')
        .query({ pattern: 'test', path: '/test' });
      expect(res.body).toHaveProperty('content');
      expect(Array.isArray(res.body.content)).toBe(true);
    });

    it('includes structuredContent in responses', async () => {
      const res = await request(createApp())
        .get('/localSearchCode')
        .query({ pattern: 'test', path: '/test' });
      expect(res.body).toHaveProperty('structuredContent');
    });
  });
});
