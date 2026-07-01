/**
 * Provider auth/rate failures must surface as typed diagnostics, not as the
 * misleading providerUnindexed ("repo may not be indexed"). Finalized tools
 * (ghSearchCode) strip errored results into top-level `errors[]` — before
 * this plumbing a 403/429 fired no status diagnostic at all.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));

vi.mock('../../src/oql/adapters/runner.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  runDirect,
}));

import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';
import { collectFlatErrors } from '../../src/utils/response/groupedFinalizer.js';

async function runCode(mockResult: Record<string, unknown>) {
  runDirect.mockResolvedValue(mockResult);
  const env = await runOqlSearch({
    target: 'code',
    from: { kind: 'github', repo: 'facebook/react' },
    where: { kind: 'text', value: 'useEffect' },
  } as never);
  if (isBatchEnvelope(env)) throw new Error('expected single envelope');
  return env;
}

describe('GitHub provider error diagnostics', () => {
  beforeEach(() => {
    runDirect.mockReset();
  });

  it('finalized rate-limit shape -> rateLimited (blocking warning), no providerUnindexed', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [],
        errors: [
          {
            id: 'q1',
            error: 'API rate limit exceeded for user (HTTP 403)',
            status: 403,
            rateLimitRemaining: 0,
            retryAfterSeconds: 30,
          },
        ],
      },
    });
    const rateLimited = env.diagnostics.find(d => d.code === 'rateLimited');
    expect(rateLimited).toBeDefined();
    expect(rateLimited?.severity).toBe('warning');
    expect(rateLimited?.blocksAnswer).toBe(true);
    expect(rateLimited?.message).toMatch(/~30s/);
    expect(rateLimited?.repair?.message).toMatch(/authenticate|token/i);
    expect(env.diagnostics.some(d => d.code === 'providerUnindexed')).toBe(
      false
    );
    expect(env.evidence.kind).not.toBe('proof');
    expect(env.evidence.answerReady).toBe(false);
  });

  it('429 without markers -> rateLimited', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [],
        errors: [{ id: 'q1', error: 'Too many requests', status: 429 }],
      },
    });
    expect(env.diagnostics.some(d => d.code === 'rateLimited')).toBe(true);
  });

  it('bulk-shape 401 structured error -> authRequired (blocking error)', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [
          {
            id: 'q1',
            status: 'error',
            data: {
              error: { error: 'Bad credentials', type: 'http', status: 401 },
            },
          },
        ],
      },
    });
    const auth = env.diagnostics.find(d => d.code === 'authRequired');
    expect(auth).toBeDefined();
    expect(auth?.severity).toBe('error');
    expect(auth?.blocksAnswer).toBe(true);
    expect(auth?.repair?.message).toMatch(/token/i);
    expect(env.diagnostics.some(d => d.code === 'providerUnindexed')).toBe(
      false
    );
  });

  it('403 with SAML enforcement message -> authRequired, not rateLimited', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [],
        errors: [
          {
            id: 'q1',
            error:
              'Resource protected by organization SAML enforcement (HTTP 403)',
            status: 403,
          },
        ],
      },
    });
    expect(env.diagnostics.some(d => d.code === 'authRequired')).toBe(true);
    expect(env.diagnostics.some(d => d.code === 'rateLimited')).toBe(false);
  });

  it('plain string error falls back to invalidQuery', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [
          {
            id: 'q1',
            status: 'error',
            data: { error: 'ValidationError: q cannot be empty' },
          },
        ],
      },
    });
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
    expect(env.diagnostics.some(d => d.code === 'rateLimited')).toBe(false);
    expect(env.diagnostics.some(d => d.code === 'authRequired')).toBe(false);
  });

  it('clean zero-hit result still emits providerUnindexed (no provider failure)', async () => {
    const env = await runCode({
      content: [],
      structuredContent: {
        results: [{ id: 'q1', status: 'hasResults', data: { files: [] } }],
      },
    });
    expect(env.diagnostics.some(d => d.code === 'providerUnindexed')).toBe(
      true
    );
  });
});

describe('collectFlatErrors structured fields', () => {
  it('keeps the human text unchanged and adds structured fields', () => {
    const errors = collectFlatErrors([
      {
        id: 'q1',
        status: 'error',
        data: {
          error: {
            error: 'API rate limit exceeded',
            type: 'http',
            status: 403,
            rateLimitRemaining: 0,
            retryAfter: 30,
            rateLimitReset: 1750000000000,
          },
        },
      } as never,
    ]);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toMatchObject({
      id: 'q1',
      error: 'API rate limit exceeded (HTTP 403)',
      status: 403,
      retryAfterSeconds: 30,
      rateLimitRemaining: 0,
      rateLimitReset: 1750000000000,
    });
  });

  it('string errors stay message-only', () => {
    const errors = collectFlatErrors([
      { id: 'q1', status: 'error', data: { error: 'boom' } } as never,
    ]);
    expect(errors[0]).toEqual({ id: 'q1', error: 'boom' });
  });
});
