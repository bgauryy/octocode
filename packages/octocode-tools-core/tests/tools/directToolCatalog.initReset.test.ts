/**
 * TDD proof for Bug #5: module-level serverRuntimeInitPromise in
 * directToolCatalog.exec.ts is never cleared when initialize() rejects.
 * A rejected promise stays cached; subsequent calls re-await it instead of
 * retrying.
 *
 * Fix: self-healing .catch() that clears serverRuntimeInitPromise on
 * rejection, so the next call retries.  The _overrideInitialize /
 * _resetInitialize test hooks (same _ convention as serverConfig's
 * _setTokenResolvers) allow injecting a failing stub without module mocking.
 *
 * RED before fix  → initCallCount stays at 1 on the 2nd call.
 * GREEN after fix → initCallCount becomes 2 (self-healed, retried).
 *
 * PACKAGE_SEARCH is used because requiresServerRuntime:true with no
 * requiresProviders, so only the serverRuntimeInitPromise path is exercised.
 * Its input schema requires `packageName` (not `keywords`).
 */
import { afterEach, beforeEach, describe, it, expect } from 'vitest';

import {
  executeDirectTool,
  _overrideInitialize,
  _resetInitialize,
} from '../../src/tools/directToolCatalog.exec.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';

const PACKAGE_QUERY = {
  packageName: 'vitest',
  mainResearchGoal: 'retry test',
  researchGoal: 'verify retry',
  reasoning: 'bug-5 regression',
} as const;

beforeEach(() => {
  _resetInitialize(); // clear any cached promise state before each test
});

afterEach(() => {
  _resetInitialize(); // restore real initialize() and clear cached promises
});

describe('executeDirectTool – Bug #5: failed init promise is cleared on rejection', () => {
  it('retries initialize() after a transient failure (does not cache the rejected promise)', async () => {
    let initCallCount = 0;

    _overrideInitialize(async () => {
      initCallCount++;
      if (initCallCount === 1) {
        throw new Error('transient init failure');
      }
      // 2nd call: succeed silently.
    });

    // First call — injected initialize() throws on call #1.
    const firstResult = await executeDirectTool(
      STATIC_TOOL_NAMES.PACKAGE_SEARCH,
      { queries: [PACKAGE_QUERY] }
    );

    expect(firstResult.isError).toBe(true);
    expect(initCallCount).toBe(1);

    // Second call — Bug (before fix): serverRuntimeInitPromise caches the
    // rejected promise → initialize() is NOT called again → initCallCount stays 1.
    // Fix: promise cleared on rejection → initialize() retried → count = 2.
    await executeDirectTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      queries: [{ ...PACKAGE_QUERY, researchGoal: 'verify retry 2nd' }],
    });

    expect(initCallCount).toBe(2);
  });

  it('_resetInitialize() clears cached promises so the next call re-initializes', async () => {
    let reinitCount = 0;

    // First override: succeeds.
    _overrideInitialize(async () => undefined);

    // First call sets serverRuntimeInitPromise to a resolved promise.
    await executeDirectTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      queries: [{ ...PACKAGE_QUERY, researchGoal: 'initial call' }],
    });

    // Now reset: clears the cached resolved promise.
    _resetInitialize();

    // Second override: tracks calls.
    _overrideInitialize(async () => {
      reinitCount++;
    });

    // Second call — because _resetInitialize cleared the promise, initialize()
    // must be called again.
    await executeDirectTool(STATIC_TOOL_NAMES.PACKAGE_SEARCH, {
      queries: [{ ...PACKAGE_QUERY, researchGoal: 'after reset' }],
    });

    expect(reinitCount).toBe(1);
  });
});
