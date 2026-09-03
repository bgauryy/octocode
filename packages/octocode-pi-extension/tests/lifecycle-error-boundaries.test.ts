/**
 * Lifecycle error-boundary regression tests.
 *
 * Gap 1b — session_compact and session_before_compact pi.on handlers have no
 *           outer try/catch; unexpected errors propagate uncaught to Pi's raw
 *           event system.
 * Gap 2  — session_shutdown calls clearCurrentContextSources() with no ctx,
 *           which clears ALL sessions' sources.  If Pi races session_start with
 *           session_shutdown the new session's early-registered sources are wiped.
 *
 * Tests are written to FAIL before the fixes and PASS after.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
import type { PiInstance } from '../src/types.js';
import {
  registerCompactionHooks,
  resetCompactionCheckpointDedupe,
} from '../src/tools/compaction-hooks.js';
import {
  captureCurrentContextSources,
  clearCurrentContextSources,
  registerCurrentContextSource,
} from '../src/tools/context-source-registry.js';
import type { PiContext } from '../src/types.js';

// ─── Minimal Pi + harness helpers ─────────────────────────────────────────────

type Handler = (event: unknown, ctx: unknown) => unknown | Promise<unknown>;

interface CompactionHarness {
  handlers: Map<string, Handler[]>;
  notes: Array<{ msg: string; level?: string }>;
  fire: (event: string, evt: unknown, ctx: unknown) => Promise<unknown[]>;
}

function makeCompactionHarness(): CompactionHarness {
  const handlers = new Map<string, Handler[]>();
  const notes: Array<{ msg: string; level?: string }> = [];
  const pi = {
    registerTool: () => undefined,
    registerCommand: () => undefined,
    sendUserMessage: () => undefined,
    sendMessage: () => undefined,
    appendEntry: (_customType: string, _data?: unknown) => undefined,
    on: (event: string, handler: Handler) => {
      const current = handlers.get(event) ?? [];
      current.push(handler);
      handlers.set(event, current);
    },
  } as unknown as PiInstance;
  const notify = (_ctx: unknown, msg: string, level?: string) =>
    notes.push({ msg, level });
  registerCompactionHooks(pi, notify as never);
  return {
    handlers,
    notes,
    fire: (event, evt, ctx) =>
      Promise.all((handlers.get(event) ?? []).map((h) => h(evt, ctx))),
  };
}

function makeCtx(sessionId = 'test-session', cwd?: string) {
  return {
    hasUI: false,
    cwd: cwd ?? process.cwd(),
    compact: () => undefined,
    getContextUsage: () => ({ tokens: 50, contextWindow: 100 }),
    sessionManager: {
      getBranch: () => [] as unknown[],
      getSessionId: () => sessionId,
      getSessionFile: () => undefined,
    },
  } as unknown as PiContext;
}

// ─── Environment setup ────────────────────────────────────────────────────────

let previousHome: string | undefined;
let testHome: string;

beforeEach(() => {
  previousHome = process.env['OCTOCODE_HOME'];
  testHome = fs.mkdtempSync(
    path.join(os.tmpdir(), 'octocode-error-bounds-'),
  );
  process.env['OCTOCODE_HOME'] = testHome;
  resetCompactionCheckpointDedupe();
  clearCurrentContextSources(); // reset module-level ownersBySession map
});

afterEach(() => {
  if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = previousHome;
  fs.rmSync(testHome, { recursive: true, force: true });
  clearCurrentContextSources();
  vi.restoreAllMocks();
});

// ─── Gap 1b: session_compact outer error boundary ─────────────────────────────

test(
  'session_compact does not propagate errors from clearAllReadStates — outer handler must catch',
  async () => {
    const harness = makeCompactionHarness();
    const ctx = makeCtx('compact-error-session', testHome);

    // Make clearAllReadStates throw to simulate an unexpected disk error
    // that occurs OUTSIDE the existing inner try/catch in the handler.
    const fileState = await import('../src/tools/file-state.js');
    vi.spyOn(fileState, 'clearAllReadStates').mockImplementation(() => {
      throw new Error('simulated disk error in clearAllReadStates');
    });

    let threw = false;
    try {
      await harness.fire(
        'session_compact',
        {
          compactionEntry: { id: 'cmp-outer-error-boundary' },
          reason: 'threshold',
          willRetry: false,
        },
        ctx,
      );
    } catch {
      threw = true;
    }

    // BEFORE FIX: threw = true (error propagates to Pi's event system).
    // AFTER FIX:  threw = false (outer try/catch swallows and notifies).
    assert.equal(
      threw,
      false,
      'session_compact must not propagate errors from clearAllReadStates to Pi',
    );
    assert.ok(
      harness.notes.some((n) => /compaction|checkpoint/i.test(n.msg)),
      'a warning notification must replace the propagated throw',
    );
  },
);

// ─── Gap 1b: session_before_compact outer error boundary ──────────────────────

test(
  'session_before_compact does not propagate unexpected errors — handler must return undefined',
  async () => {
    const harness = makeCompactionHarness();
    const ctx = makeCtx('before-compact-error-session', testHome);

    // Make activePlanScope throw.  It is called at the top of the overflow path
    // BEFORE any try/catch in the handler.
    const activePlanModule = await import('../src/tools/active-plan.js');
    vi.spyOn(activePlanModule, 'activePlanScope').mockImplementation(() => {
      throw new Error('simulated plan-scope error in session_before_compact');
    });

    let threw = false;
    let result: unknown;
    try {
      [result] = await harness.fire(
        'session_before_compact',
        {
          preparation: {
            isSplitTurn: true,
            turnPrefixMessages: [{ role: 'assistant', content: '' }],
            firstKeptEntryId: 'keep-1',
            tokensBefore: 120_000,
          },
          reason: 'overflow',
          willRetry: false,
        },
        ctx,
      );
    } catch {
      threw = true;
    }

    // BEFORE FIX: threw = true.
    // AFTER FIX:  threw = false; result = undefined (Pi uses its default summarizer).
    assert.equal(
      threw,
      false,
      'session_before_compact must not propagate unexpected errors to Pi',
    );
    assert.equal(
      result,
      undefined,
      'on unexpected error the handler must return undefined so Pi uses its default path',
    );
  },
);

// ─── Gap 2: session_shutdown must NOT clear other sessions' context sources ────

test(
  'session_shutdown no longer wipes context sources from a concurrently active session',
  async () => {
    const harness = makeCompactionHarness();

    // Two distinct sessions — different sessionIds ensure different registry keys.
    const cwdA = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-gap2-a-'));
    const cwdB = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-gap2-b-'));
    const ctxA = makeCtx('session-a', cwdA);
    const ctxB = makeCtx('session-b', cwdB);

    try {
      // Register a context source for session B (the "new" session).
      registerCurrentContextSource(ctxB, {
        version: 1,
        id: 'session-b-tool-result',
        kind: 'tool-result',
        origin: 'tool:some-tool',
        authority: 'external-data',
        scope: 'task',
        visibility: 'inspectable',
        rehydrate: 'always',
        readCurrent: () => 'session b tool result content',
      });

      const captureBefore = captureCurrentContextSources(ctxB);
      assert.equal(
        captureBefore.sources.length,
        1,
        'precondition: session B source must be registered',
      );

      // Session A shuts down (simulates /new – the old session ends).
      await harness.fire('session_shutdown', { reason: 'new' }, ctxA);

      // BEFORE FIX: captureCurrentContextSources() no-ctx cleared ALL sessions,
      //             so session B's source is gone → sources.length === 0 → FAILS.
      // AFTER FIX:  clearCurrentContextSources() is removed from session_shutdown;
      //             session B's source survives → sources.length === 1 → PASSES.
      const captureAfter = captureCurrentContextSources(ctxB);
      assert.equal(
        captureAfter.sources.length,
        1,
        'session B context sources must survive session A shutdown',
      );
    } finally {
      fs.rmSync(cwdA, { recursive: true, force: true });
      fs.rmSync(cwdB, { recursive: true, force: true });
    }
  },
);

// ─── Gap 2 (complementary): session A sources ARE cleared after fix ────────────

test(
  'after fix, disposeSessionResources still clears session-specific context sources on shutdown',
  async () => {
    // This test verifies the replacement mechanism works: clearCurrentContextSources(ctx)
    // in disposeSessionResources removes only the shutting-down session's sources.
    // We test this at the context-source-registry level: register for ctxA, manually
    // call clearCurrentContextSources(ctxA), verify ctxA's sources are gone while ctxB's remain.
    const cwdA = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-gap2c-a-'));
    const cwdB = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-gap2c-b-'));
    const ctxA = makeCtx('session-a-clear', cwdA);
    const ctxB = makeCtx('session-b-clear', cwdB);

    try {
      registerCurrentContextSource(ctxA, {
        version: 1,
        id: 'a-source',
        kind: 'tool-result',
        origin: 'tool:a',
        authority: 'external-data',
        scope: 'task',
        visibility: 'inspectable',
        rehydrate: 'always',
        readCurrent: () => 'a content',
      });
      registerCurrentContextSource(ctxB, {
        version: 1,
        id: 'b-source',
        kind: 'tool-result',
        origin: 'tool:b',
        authority: 'external-data',
        scope: 'task',
        visibility: 'inspectable',
        rehydrate: 'always',
        readCurrent: () => 'b content',
      });

      // Simulate what disposeSessionResources will do after the fix.
      clearCurrentContextSources(ctxA);

      assert.equal(
        captureCurrentContextSources(ctxA).sources.length,
        0,
        'session A sources must be cleared by ctx-specific clear',
      );
      assert.equal(
        captureCurrentContextSources(ctxB).sources.length,
        1,
        'session B sources must survive session A ctx-specific clear',
      );
    } finally {
      fs.rmSync(cwdA, { recursive: true, force: true });
      fs.rmSync(cwdB, { recursive: true, force: true });
    }
  },
);
