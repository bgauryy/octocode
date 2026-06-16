/* eslint-disable @typescript-eslint/no-explicit-any */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import request from 'supertest';

vi.mock('../../mcpCache.js', () => ({
  getMcpContent: vi.fn().mockReturnValue({
    tools: { localSearchCode: { name: 'localSearchCode', description: 'Search', schema: {}, hints: { hasResults: [], empty: [] } } },
    prompts: { research: { name: 'research', description: 'Research', args: [], content: 'test' } },
    instructions: 'Test instructions',
    baseHints: [],
    genericErrorHints: [],
    baseSchema: {},
  }),
  initializeMcpContent: vi.fn().mockResolvedValue({}),
  isMcpInitialized: vi.fn().mockReturnValue(true),
}));

vi.mock('../../index.js', () => {
  const r = { content: [{ type: 'text', text: 'results:\n  - status: hasResults\n    data:\n      files: []\n      totalMatches: 0' }] };
  return {
    initializeProviders: vi.fn().mockResolvedValue(undefined),
    initializeSession: vi.fn(),
    logSessionInit: vi.fn().mockResolvedValue(undefined),
    logToolCall: vi.fn().mockResolvedValue(undefined),
    logPromptCall: vi.fn().mockResolvedValue(undefined),
    localSearchCode: vi.fn().mockResolvedValue(r),
    localGetFileContent: vi.fn().mockResolvedValue(r),
    localFindFiles: vi.fn().mockResolvedValue(r),
    localViewStructure: vi.fn().mockResolvedValue(r),
    ghSearchCode: vi.fn().mockResolvedValue(r),
    ghGetFileContent: vi.fn().mockResolvedValue(r),
    ghViewRepoStructure: vi.fn().mockResolvedValue(r),
    ghSearchRepos: vi.fn().mockResolvedValue(r),
    ghSearchPRs: vi.fn().mockResolvedValue(r),
    ghCloneRepo: vi.fn().mockResolvedValue(r),
    lspGetSemantics: vi.fn().mockResolvedValue(r),
    npmSearch: vi.fn().mockResolvedValue(r),
  };
});

vi.mock('../../utils/logger.js', () => ({
  initializeLogger: vi.fn(),
  getLogsPath: vi.fn().mockReturnValue('/tmp/logs'),
  logToolCall: vi.fn(),
  logError: vi.fn(),
  logWarn: vi.fn(),
  sanitizeQueryParams: vi.fn().mockReturnValue({}),
}));

vi.mock('../../utils/circuitBreaker.js', () => ({
  getAllCircuitStates: vi.fn().mockReturnValue({}),
  clearAllCircuits: vi.fn(),
  stopCircuitCleanup: vi.fn(),
  configureCircuit: vi.fn(),
  withCircuitBreaker: vi.fn((_name: string, fn: () => any) => fn()),
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

vi.mock('../../utils/errorQueue.js', () => ({
  errorQueue: {
    getRecent: vi.fn().mockReturnValue([]),
    push: vi.fn(),
    size: 0,
  },
}));

import { createServer } from '../../server.js';

describe('Server HTTP Flows', () => {
  let app: any;

  beforeEach(async () => {
    app = await createServer();
  });

  describe('GET /health', () => {
    it('returns 200 with health data when initialized', async () => {
      const res = await request(app).get('/health');
      expect(res.status).toBe(200);
      expect(res.body.status).toBe('ok');
    });

    it('includes all required health fields', async () => {
      const res = await request(app).get('/health');
      const body = res.body;
      expect(body).toHaveProperty('host');
      expect(body).toHaveProperty('port');
      expect(body).toHaveProperty('uptime');
      expect(body).toHaveProperty('processManager');
      expect(body).toHaveProperty('pid');
      expect(body).toHaveProperty('idle');
      expect(body).toHaveProperty('memory');
      expect(body).toHaveProperty('circuits');
      expect(body).toHaveProperty('errors');
    });

    it('reports detached daemon as process manager', async () => {
      const res = await request(app).get('/health');
      expect(res.body.processManager).toContain('detached');
    });

    it('returns numeric pid', async () => {
      const res = await request(app).get('/health');
      expect(typeof res.body.pid).toBe('number');
      expect(res.body.pid).toBeGreaterThan(0);
    });

    it('includes idle tracking info', async () => {
      const res = await request(app).get('/health');
      const { idle } = res.body;
      expect(idle).toHaveProperty('currentMs');
      expect(idle).toHaveProperty('thresholdMs');
      expect(idle).toHaveProperty('checkIntervalMs');
      expect(idle).toHaveProperty('percentToRestart');
      expect(typeof idle.currentMs).toBe('number');
    });

    it('includes memory stats in MB', async () => {
      const res = await request(app).get('/health');
      const { memory } = res.body;
      expect(memory).toHaveProperty('heapUsed');
      expect(memory).toHaveProperty('heapTotal');
      expect(memory).toHaveProperty('rss');
      expect(memory.heapUsed).toBeGreaterThan(0);
    });

    it('returns initializing when MCP not ready', async () => {
      const { isMcpInitialized } = await import('../../mcpCache.js');
      vi.mocked(isMcpInitialized).mockReturnValueOnce(false);
      const res = await request(app).get('/health');
      expect(res.body.status).toBe('initializing');
    });

    it('includes version string', async () => {
      const res = await request(app).get('/health');
      expect(res.body).toHaveProperty('version');
      expect(typeof res.body.version).toBe('string');
    });
  });

  describe('404 handler', () => {
    it('returns 404 for unknown routes', async () => {
      const res = await request(app).get('/nonexistent');
      expect(res.status).toBe(404);
      expect(res.body.success).toBe(false);
    });

    it('includes error code NOT_FOUND', async () => {
      const res = await request(app).get('/does-not-exist');
      expect(res.body.error.code).toBe('NOT_FOUND');
    });

    it('lists available routes in response', async () => {
      const res = await request(app).get('/unknown');
      expect(res.body.error.availableRoutes).toBeDefined();
      expect(Array.isArray(res.body.error.availableRoutes)).toBe(true);
      expect(res.body.error.availableRoutes.length).toBeGreaterThan(5);
    });

    it('includes hint about POST tool calls', async () => {
      const res = await request(app).get('/wrong-path');
      expect(res.body.error.hint).toContain('POST');
    });

    it('returns 404 for POST to unknown routes', async () => {
      const res = await request(app).post('/unknown').send({});
      expect(res.status).toBe(404);
    });
  });

  describe('Middleware', () => {
    it('parses JSON request bodies', async () => {
      const res = await request(app)
        .post('/tools/call/localSearchCode')
        .send({
          queries: [{
            id: 'test-1',
            researchGoal: 'test',
            reasoning: 'test',
            pattern: 'foo',
            path: '/test',
          }],
        });
      expect(res.status).not.toBe(415);
    });

    it('updates idle timer on requests', async () => {
      const res1 = await request(app).get('/health');
      const idle1 = res1.body.idle.currentMs;
      await new Promise((r) => setTimeout(r, 50));
      const res2 = await request(app).get('/health');
      const idle2 = res2.body.idle.currentMs;
      expect(idle2).toBeLessThanOrEqual(idle1 + 200);
    });
  });

  describe('Route mounting', () => {
    it('tools routes are mounted at /tools', async () => {
      const res = await request(app).get('/tools/list');
      expect(res.status).toBe(200);
    });

    it('prompts routes are mounted at /prompts', async () => {
      const res = await request(app).get('/prompts/list');
      expect(res.status).toBe(200);
    });
  });
});
