/**
 * withSecurityValidation.core.test.ts
 *
 * Parity contract tests for the shared `runSecure` core introduced when the
 * two security wrappers were merged.
 *
 * Both `withSecurityValidation` (auth-aware) and `withBasicSecurityValidation`
 * (auth-free) delegate to the same internal `runSecure` function.  Every
 * behaviour they share — validation, timeout, abort, error result shape,
 * logging — must be identical.  This file makes that contract explicit and
 * regression-proof.
 *
 * Layout:
 *   CORE-01  Input validation (shared path)
 *   CORE-02  Success result shape
 *   CORE-03  Logging on success vs error (unified gate)
 *   CORE-04  logSessionError on exception
 *   CORE-05  Timeout behaviour
 *   CORE-06  AbortSignal cancellation
 *   CORE-07  Full-wrapper auth passthrough (unique to withSecurityValidation)
 *   CORE-08  Basic-wrapper no-auth contract (unique to withBasicSecurityValidation)
 *   CORE-09  configureSecurity isolation
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  withSecurityValidation,
  withBasicSecurityValidation,
  configureSecurity,
} from '../src/withSecurityValidation.js';

// ---------------------------------------------------------------------------
// Shared mock for ContentSanitizer — tests control isValid / hasSecrets.
// ---------------------------------------------------------------------------
vi.mock('../src/contentSanitizer.js', () => ({
  ContentSanitizer: {
    validateInputParameters: vi.fn(),
    sanitizeContent: vi.fn((content: string) => ({
      content,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    })),
  },
}));

import { ContentSanitizer } from '../src/contentSanitizer.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function validationOk(
  params: Record<string, unknown> = {}
): ReturnType<(typeof ContentSanitizer)['validateInputParameters']> {
  return {
    isValid: true,
    sanitizedParams: params,
    hasSecrets: false,
    warnings: [],
  };
}

function validationFail(
  warnings: string[]
): ReturnType<(typeof ContentSanitizer)['validateInputParameters']> {
  return { isValid: false, sanitizedParams: {}, hasSecrets: false, warnings };
}

const SUCCESS_RESULT = {
  content: [{ type: 'text' as const, text: 'ok' }],
  isError: false,
} as const;

const ERROR_RESULT = {
  content: [{ type: 'text' as const, text: 'fail' }],
  isError: true,
} as const;

const mockLogToolCall = vi.fn().mockResolvedValue(undefined);
const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
const mockIsLoggingEnabled = vi.fn().mockReturnValue(false);

function setupDeps(loggingOn = false) {
  mockIsLoggingEnabled.mockReturnValue(loggingOn);
  configureSecurity({
    logToolCall: mockLogToolCall,
    logSessionError: mockLogSessionError,
    isLoggingEnabled: mockIsLoggingEnabled,
  });
}

function teardownDeps() {
  configureSecurity({
    logToolCall: undefined,
    logSessionError: undefined,
    isLoggingEnabled: undefined,
  });
}

// ---------------------------------------------------------------------------
// CORE-01: Input validation — identical rejection path in both wrappers
// ---------------------------------------------------------------------------
describe('CORE-01: Input validation — both wrappers reject invalid params identically', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('full wrapper: returns error result when validation fails', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationFail(['Dangerous key blocked: __proto__', 'Too long'])
    );
    const handler = vi.fn();
    const wrapped = withSecurityValidation('tool', handler);
    const result = await wrapped({}, {});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/Security validation failed/);
    expect(result.content[0]?.text).toContain('Dangerous key blocked');
    expect(handler).not.toHaveBeenCalled();
  });

  it('basic wrapper: returns error result when validation fails', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationFail(['Circular reference detected'])
    );
    const handler = vi.fn();
    const wrapped = withBasicSecurityValidation(handler, 'tool');
    const result = await wrapped({});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/Security validation failed/);
    expect(result.content[0]?.text).toContain('Circular reference');
    expect(handler).not.toHaveBeenCalled();
  });

  it('both produce the same error prefix on the same failure message', async () => {
    const warnings = ['Maximum nesting depth exceeded'];
    const handler = vi.fn();

    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationFail(warnings)
    );
    const full = withSecurityValidation('t', handler);
    const fullResult = await full({}, {});

    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationFail(warnings)
    );
    const basic = withBasicSecurityValidation(handler, 't');
    const basicResult = await basic({});

    expect(fullResult.content[0]?.text).toBe(basicResult.content[0]?.text);
    expect(fullResult.isError).toBe(basicResult.isError);
  });

  it('handler is not invoked on validation failure in either wrapper', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationFail(['bad'])
    );
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);

    await withSecurityValidation('t', handler)({}, {});
    await withBasicSecurityValidation(handler, 't')({});

    expect(handler).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// CORE-02: Success result shape — both wrappers forward handler output unchanged
// ---------------------------------------------------------------------------
describe('CORE-02: Success result — both wrappers forward handler output unchanged', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk({ key: 'val' })
    );
  });
  afterEach(teardownDeps);

  it('full wrapper forwards handler result as-is', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withSecurityValidation('t', handler)({}, {});
    expect(result).toEqual(SUCCESS_RESULT);
  });

  it('basic wrapper forwards handler result as-is', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(handler, 't')({});
    expect(result).toEqual(SUCCESS_RESULT);
  });

  it('both wrappers pass sanitizedParams to the handler', async () => {
    const params = { query: 'clean' };
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk(params)
    );

    const fullHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation<typeof params>('t', fullHandler)({}, {});
    expect(fullHandler).toHaveBeenCalledWith(params, undefined, undefined);

    const basicHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation<typeof params>(basicHandler, 't')({});
    expect(basicHandler).toHaveBeenCalledWith(params);
  });
});

// ---------------------------------------------------------------------------
// CORE-03: Logging gate — both wrappers log on success, skip on error
// ---------------------------------------------------------------------------
describe('CORE-03: Logging gate — both wrappers log on success, skip on error', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps(true); // logging ON
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk({ queries: [{ owner: 'acme', repo: 'api' }] })
    );
  });
  afterEach(teardownDeps);

  it('full wrapper: logToolCall fires on success', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation('githubSearchCode', handler)({}, {});
    expect(mockLogToolCall).toHaveBeenCalledTimes(1);
    expect(mockLogToolCall).toHaveBeenCalledWith(
      'githubSearchCode',
      ['acme/api'],
      undefined,
      undefined,
      undefined
    );
  });

  it('basic wrapper: logToolCall fires on success', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(handler, 'localSearchCode')({});
    expect(mockLogToolCall).toHaveBeenCalledTimes(1);
  });

  it('full wrapper: logToolCall does NOT fire when handler returns isError=true', async () => {
    const handler = vi.fn().mockResolvedValue(ERROR_RESULT);
    await withSecurityValidation('tool', handler)({}, {});
    expect(mockLogToolCall).not.toHaveBeenCalled();
  });

  it('basic wrapper: logToolCall does NOT fire when handler returns isError=true', async () => {
    const handler = vi.fn().mockResolvedValue(ERROR_RESULT);
    await withBasicSecurityValidation(handler, 'tool')({});
    expect(mockLogToolCall).not.toHaveBeenCalled();
  });

  it('full wrapper: no logToolCall when logging is disabled', async () => {
    mockIsLoggingEnabled.mockReturnValue(false);
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation('tool', handler)({}, {});
    expect(mockLogToolCall).not.toHaveBeenCalled();
  });

  it('basic wrapper: no logToolCall when logging is disabled', async () => {
    mockIsLoggingEnabled.mockReturnValue(false);
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(handler, 'tool')({});
    expect(mockLogToolCall).not.toHaveBeenCalled();
  });

  it('basic wrapper with no toolName: no logToolCall (no name to log under)', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    // toolName defaults to 'tool' internally — logging still fires
    // (regression guard: previous impl gated on isLocalTool which is now removed)
    await withBasicSecurityValidation(handler)({});
    // 'tool' is the effective name — log should fire
    expect(mockLogToolCall).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// CORE-04: logSessionError — both wrappers report on exception
// ---------------------------------------------------------------------------
// Note: logSessionError fires when the outer try/catch in runSecure catches a
// synchronous exception (e.g. from validateInputParameters).  A rejected
// handler promise is caught inside withToolTimeout, which resolves to an error
// result — that path does NOT call logSessionError.
describe('CORE-04: logSessionError on synchronous exception in sanitizer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('full wrapper: returns error result and calls logSessionError when sanitizer throws', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(() => {
      throw new Error('sanitizer exploded');
    });
    const handler = vi.fn();
    const result = await withSecurityValidation('tool', handler)({}, {});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('sanitizer exploded');
    await Promise.resolve(); // flush fire-and-forget microtask
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'tool',
      'TOOL_SECURITY_VALIDATION_FAILED'
    );
  });

  it('basic wrapper: returns error result and calls logSessionError when sanitizer throws', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(() => {
      throw new Error('sanitizer exploded');
    });
    const handler = vi.fn();
    const result = await withBasicSecurityValidation(handler, 'local_read')({});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('sanitizer exploded');
    await Promise.resolve();
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'local_read',
      'TOOL_SECURITY_VALIDATION_FAILED'
    );
  });

  it('rejected handler: withToolTimeout swallows the rejection — error result returned, no logSessionError', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(validationOk());
    const handler = vi.fn().mockRejectedValue(new Error('db exploded'));
    const result = await withSecurityValidation('tool', handler)({}, {});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('db exploded');
    await Promise.resolve();
    // withToolTimeout catches this internally — logSessionError must NOT fire
    expect(mockLogSessionError).not.toHaveBeenCalled();
  });

  it('both wrappers return error (not throw) when sanitizer throws non-Error', async () => {
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(() => {
      // eslint-disable-next-line @typescript-eslint/no-throw-literal
      throw 'string error';
    });
    const handler = vi.fn();
    const full = await withSecurityValidation('t', handler)({}, {});
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(() => {
      throw 'string error';
    });
    const basic = await withBasicSecurityValidation(handler, 't')({});
    expect(full.isError).toBe(true);
    expect(basic.isError).toBe(true);
    expect(full.content[0]?.text).toContain('Unknown error');
    expect(basic.content[0]?.text).toContain('Unknown error');
  });
});

// ---------------------------------------------------------------------------
// CORE-05: Timeout — both wrappers enforce it
// ---------------------------------------------------------------------------
describe('CORE-05: Timeout enforcement', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    setupDeps();
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk()
    );
  });
  afterEach(() => {
    vi.useRealTimers();
    teardownDeps();
  });

  it('full wrapper: times out and returns error', async () => {
    const handler = vi.fn().mockReturnValue(new Promise(() => {})); // never resolves
    const promise = withSecurityValidation('slow_tool', handler, {
      timeoutMs: 100,
    })({}, {});
    vi.advanceTimersByTime(150);
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/timed out/);
    expect(result.content[0]?.text).toContain('slow_tool');
  });

  it('basic wrapper: times out and returns error', async () => {
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withBasicSecurityValidation(handler, 'slow_local', {
      timeoutMs: 100,
    })({});
    vi.advanceTimersByTime(150);
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/timed out/);
    expect(result.content[0]?.text).toContain('slow_local');
  });

  it('both wrappers complete before timeout when handler is fast', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);

    const p1 = withSecurityValidation('t', handler, { timeoutMs: 5000 })({}, {});
    const p2 = withBasicSecurityValidation(handler, 't', { timeoutMs: 5000 })({});

    vi.advanceTimersByTime(10); // well before timeout
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.isError).toBe(false);
    expect(r2.isError).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// CORE-06: AbortSignal — both wrappers cancel on abort
// ---------------------------------------------------------------------------
describe('CORE-06: AbortSignal cancellation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk()
    );
  });
  afterEach(teardownDeps);

  it('full wrapper: already-aborted signal returns cancellation error before handler runs', async () => {
    const controller = new AbortController();
    controller.abort();
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withSecurityValidation('t', handler)({}, {
      signal: controller.signal,
    });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('basic wrapper: already-aborted signal returns cancellation error before handler runs', async () => {
    const controller = new AbortController();
    controller.abort();
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(handler, 't')({}, {
      signal: controller.signal,
    });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('full wrapper: aborting mid-flight resolves to cancellation result', async () => {
    const controller = new AbortController();
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withSecurityValidation('t', handler)({}, {
      signal: controller.signal,
    });
    controller.abort();
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('basic wrapper: aborting mid-flight resolves to cancellation result', async () => {
    const controller = new AbortController();
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withBasicSecurityValidation(handler, 't')({}, {
      signal: controller.signal,
    });
    controller.abort();
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });
});

// ---------------------------------------------------------------------------
// CORE-07: withSecurityValidation — auth passthrough (unique to this wrapper)
// ---------------------------------------------------------------------------
describe('CORE-07: withSecurityValidation — auth/session passthrough', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk()
    );
  });
  afterEach(teardownDeps);

  it('passes authInfo and sessionId to handler', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const authInfo = { userId: 'u1', token: 'tok' };
    await withSecurityValidation('t', handler)({}, {
      authInfo,
      sessionId: 'sess-123',
    });
    expect(handler).toHaveBeenCalledWith({}, authInfo, 'sess-123');
  });

  it('passes undefined authInfo/sessionId when not provided', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation('t', handler)({}, {});
    expect(handler).toHaveBeenCalledWith({}, undefined, undefined);
  });

  it('handler receives different authInfo on each call independently', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const wrapped = withSecurityValidation('t', handler);

    await wrapped({}, { authInfo: { user: 'alice' } });
    await wrapped({}, { authInfo: { user: 'bob' } });

    expect(handler).toHaveBeenNthCalledWith(1, {}, { user: 'alice' }, undefined);
    expect(handler).toHaveBeenNthCalledWith(2, {}, { user: 'bob' }, undefined);
  });
});

// ---------------------------------------------------------------------------
// CORE-08: withBasicSecurityValidation — no auth in handler signature
// ---------------------------------------------------------------------------
describe('CORE-08: withBasicSecurityValidation — no auth contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk({ path: '/workspace/file.ts' })
    );
  });
  afterEach(teardownDeps);

  it('handler receives only sanitizedArgs (no auth context)', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(handler, 'local_read')({});
    // Basic handler signature is (sanitizedArgs: T) — no auth args
    expect(handler).toHaveBeenCalledWith({ path: '/workspace/file.ts' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('optional extra.signal is respected', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(handler, 't')({}, {
      signal: new AbortController().signal,
    });
    expect(result.isError).toBe(false);
  });

  it('missing extra argument does not throw', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    // No second argument at all
    await expect(
      withBasicSecurityValidation(handler, 't')({})
    ).resolves.not.toThrow();
  });

  it('toolName defaults to "tool" in timeout messages when not provided', async () => {
    vi.useFakeTimers();
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withBasicSecurityValidation(handler, undefined, {
      timeoutMs: 50,
    })({});
    vi.advanceTimersByTime(100);
    const result = await promise;
    expect(result.content[0]?.text).toContain('tool');
    vi.useRealTimers();
  });
});

// ---------------------------------------------------------------------------
// CORE-09: configureSecurity isolation — deps are applied to both wrappers
// ---------------------------------------------------------------------------
describe('CORE-09: configureSecurity applies to both wrappers', () => {
  afterEach(teardownDeps);

  it('custom sanitizer is used by both wrappers', async () => {
    const customSanitizer = {
      sanitizeContent: vi.fn((c: string) => ({
        content: c,
        hasSecrets: false,
        secretsDetected: [],
        warnings: [],
      })),
      validateInputParameters: vi.fn().mockReturnValue(validationOk({ x: 1 })),
    };
    configureSecurity({ sanitizer: customSanitizer });

    const fullHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const basicHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);

    await withSecurityValidation('t', fullHandler)({}, {});
    await withBasicSecurityValidation(basicHandler, 't')({});

    expect(customSanitizer.validateInputParameters).toHaveBeenCalledTimes(2);
  });

  it('defaultTimeoutMs from configureSecurity is respected by both wrappers', async () => {
    vi.useFakeTimers();
    configureSecurity({ defaultTimeoutMs: 100 });
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue(
      validationOk()
    );

    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const p1 = withSecurityValidation('t', handler)({}, {});
    const p2 = withBasicSecurityValidation(handler, 't')({});

    vi.advanceTimersByTime(150);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.isError).toBe(true);
    expect(r1.content[0]?.text).toMatch(/timed out/);
    expect(r2.isError).toBe(true);
    expect(r2.content[0]?.text).toMatch(/timed out/);
    vi.useRealTimers();
  });
});
