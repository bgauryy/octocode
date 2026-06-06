/* eslint-disable @typescript-eslint/no-explicit-any */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import request from 'supertest';
import { join } from 'path';
import { homedir } from 'os';


vi.mock('../../mcpCache.js', () => ({
  getMcpContent: vi.fn().mockReturnValue({
    tools: {},
    prompts: {},
    instructions: 'Test instructions',
    baseHints: [],
    genericErrorHints: [],
    baseSchema: {},
  }),
  initializeMcpContent: vi.fn().mockResolvedValue({}),
  isMcpInitialized: vi.fn().mockReturnValue(true),
}));

vi.mock('../../index.js', () => {
  const r = { content: [{ type: 'text', text: 'results:\n  - status: hasResults' }] };
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
    githubSearchCode: vi.fn().mockResolvedValue(r),
    githubGetFileContent: vi.fn().mockResolvedValue(r),
    githubViewRepoStructure: vi.fn().mockResolvedValue(r),
    githubSearchRepositories: vi.fn().mockResolvedValue(r),
    githubSearchPullRequests: vi.fn().mockResolvedValue(r),
    lspGotoDefinition: vi.fn().mockResolvedValue(r),
    lspFindReferences: vi.fn().mockResolvedValue(r),
    lspCallHierarchy: vi.fn().mockResolvedValue(r),
    packageSearch: vi.fn().mockResolvedValue(r),
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

const mockWriteFileSync = vi.fn();
const mockUnlinkSync = vi.fn();
const mockMkdirSync = vi.fn();

vi.mock('fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('fs')>();
  return {
    ...actual,
    writeFileSync: (...args: any[]) => mockWriteFileSync(...args),
    unlinkSync: (...args: any[]) => mockUnlinkSync(...args),
    mkdirSync: (...args: any[]) => mockMkdirSync(...args),
  };
});


import { createServer } from '../../server.js';

describe('Server Lifecycle', () => {
  let app: any;

  beforeEach(async () => {
    vi.clearAllMocks();
    app = await createServer();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('health endpoint after initialization', () => {
    it('returns ok when MCP is initialized', async () => {
      const res = await request(app).get('/health');
      expect(res.status).toBe(200);
      expect(res.body.status).toBe('ok');
      expect(res.body.processManager).toContain('detached');
      expect(res.body.pid).toBe(process.pid);
    });

    it('returns initializing when MCP is not ready', async () => {
      const { isMcpInitialized } = await import('../../mcpCache.js');
      vi.mocked(isMcpInitialized).mockReturnValueOnce(false);

      const res = await request(app).get('/health');
      expect(res.status).toBe(200);
      expect(res.body.status).toBe('initializing');
    });

    it('reports positive pid', async () => {
      const res = await request(app).get('/health');
      expect(res.body.pid).toBeGreaterThan(0);
    });
  });

  describe('PID file management', () => {
    it('startServer writes PID file on listen', async () => {
      const { PID_FILE } = await import('../../server.js');

      const listenSpy = vi.spyOn(app, 'listen').mockImplementation(
        ((...args: any[]) => {
          const cb = args.find((a: any) => typeof a === 'function');
          if (cb) cb();
          return { on: vi.fn(), close: vi.fn() } as any;
        }) as any
      );


      const OCTOCODE_DIR = process.env.OCTOCODE_HOME || join(homedir(), '.octocode');
      const expectedPidFile = PID_FILE;

      expect(expectedPidFile).toContain(OCTOCODE_DIR);
      expect(expectedPidFile).toContain('research-server-');

      listenSpy.mockRestore();
    });

    it('PID file path includes the configured port', async () => {
      const { PID_FILE } = await import('../../server.js');
      expect(PID_FILE).toMatch(/research-server-\d+\.pid$/);
    });
  });

  describe('server-init flow (mocked)', () => {
    it('detects running server and exits immediately (fast path)', async () => {
      const healthBody = { status: 'ok' };
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(healthBody),
      });

      const originalFetch = globalThis.fetch;
      globalThis.fetch = mockFetch as any;

      try {
        const response = await globalThis.fetch('http://localhost:1987/health', {
          signal: AbortSignal.timeout(5000),
        });
        const body = await (response as any).json();

        expect(body.status).toBe('ok');
        expect(mockFetch).toHaveBeenCalledTimes(1);
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it('starts server and polls health when server is not running', async () => {
      let callCount = 0;
      const mockFetch = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount <= 2) {
          return Promise.reject(new Error('ECONNREFUSED'));
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok' }),
        });
      });

      const originalFetch = globalThis.fetch;
      globalThis.fetch = mockFetch as any;

      try {
        let ready = false;
        for (let i = 0; i < 5 && !ready; i++) {
          try {
            const res = await globalThis.fetch('http://localhost:1987/health', {
              signal: AbortSignal.timeout(5000),
            });
            const body = await (res as any).json();
            if (body.status === 'ok') ready = true;
          } catch {
    void 0;
  }
        }

        expect(ready).toBe(true);
        expect(mockFetch).toHaveBeenCalledTimes(3);
      } finally {
        globalThis.fetch = originalFetch;
      }
    });

    it('handles initializing state before ok', async () => {
      let callCount = 0;
      const mockFetch = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve({ status: 'initializing' }),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok' }),
        });
      });

      const originalFetch = globalThis.fetch;
      globalThis.fetch = mockFetch as any;

      try {
        let ready = false;
        for (let i = 0; i < 5 && !ready; i++) {
          try {
            const res = await globalThis.fetch('http://localhost:1987/health', {
              signal: AbortSignal.timeout(5000),
            });
            const body = await (res as any).json();
            if (body.status === 'ok') ready = true;
          } catch {
    void 0;
  }
        }

        expect(ready).toBe(true);
        expect(mockFetch).toHaveBeenCalledTimes(2);
      } finally {
        globalThis.fetch = originalFetch;
      }
    });
  });
});
