import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  withSecurityValidation,
  withBasicSecurityValidation,
  configureSecurity,
} from '../src/withSecurityValidation.js';

function makeDeepObject(depth: number): Record<string, unknown> {
  return depth <= 0 ? {} : { x: makeDeepObject(depth - 1) };
}

function makeCircularObject(): Record<string, unknown> {
  const obj: Record<string, unknown> = { a: 1 };
  obj.self = obj;
  return obj;
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
    sanitizer: undefined,
  });
}

describe('CORE-01: Input validation — both wrappers reject invalid params identically', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('full wrapper: returns error result when validation fails (dangerous key)', async () => {
    const handler = vi.fn();
    const wrapped = withSecurityValidation('tool', handler);
    const result = await wrapped({ constructor: 'evil' }, {});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/Security validation failed/);
    expect(result.content[0]?.text).toContain(
      'Dangerous parameter key blocked'
    );
    expect(handler).not.toHaveBeenCalled();
  });

  it('basic wrapper: returns error result when validation fails (circular ref)', async () => {
    const handler = vi.fn();
    const wrapped = withBasicSecurityValidation(handler, 'tool');
    const result = await wrapped(makeCircularObject());
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/Security validation failed/);
    expect(result.content[0]?.text).toContain('Circular reference detected');
    expect(handler).not.toHaveBeenCalled();
  });

  it('both produce the same error prefix on the same input', async () => {
    const handler = vi.fn();
    const input = { constructor: 'evil' };

    const full = withSecurityValidation('t', handler);
    const fullResult = await full(input, {});

    const basic = withBasicSecurityValidation(handler, 't');
    const basicResult = await basic(input);

    expect(fullResult.content[0]?.text).toBe(basicResult.content[0]?.text);
    expect(fullResult.isError).toBe(basicResult.isError);
  });

  it('handler is not invoked on validation failure in either wrapper', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const input = { prototype: 'evil' };

    await withSecurityValidation('t', handler)(input, {});
    await withBasicSecurityValidation(handler, 't')(input);

    expect(handler).not.toHaveBeenCalled();
  });

  it('both wrappers accept deeply nested valid objects', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const deepInput = makeDeepObject(19);

    const r1 = await withSecurityValidation('t', handler)(deepInput, {});
    const r2 = await withBasicSecurityValidation(handler, 't')(deepInput);

    expect(r1.isError).toBe(false);
    expect(r2.isError).toBe(false);
  });

  it('both wrappers reject objects exceeding max nesting depth', async () => {
    const handler = vi.fn();
    const input = makeDeepObject(22);

    const r1 = await withSecurityValidation('t', handler)(input, {});
    const r2 = await withBasicSecurityValidation(handler, 't')(input);

    expect(r1.isError).toBe(true);
    expect(r2.isError).toBe(true);
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('CORE-02: Success result — both wrappers forward handler output unchanged', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('full wrapper forwards handler result as-is', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withSecurityValidation('t', handler)(
      { key: 'val' },
      {}
    );
    expect(result).toEqual(SUCCESS_RESULT);
  });

  it('basic wrapper forwards handler result as-is', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(
      handler,
      't'
    )({ key: 'val' });
    expect(result).toEqual(SUCCESS_RESULT);
  });

  it('both wrappers pass sanitizedParams to the handler', async () => {
    const params = { query: 'clean input' };

    const fullHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation<typeof params>('t', fullHandler)(params, {});
    expect(fullHandler).toHaveBeenCalledWith(params, undefined, undefined);

    const basicHandler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation<typeof params>(basicHandler, 't')(params);
    expect(basicHandler).toHaveBeenCalledWith(params);
  });
});

describe('CORE-03: Logging gate — both wrappers log on success, skip on error', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps(true); // logging ON
  });
  afterEach(teardownDeps);

  it('full wrapper: logToolCall fires on success', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withSecurityValidation('ghSearchCode', handler)(
      { queries: [{ owner: 'acme', repo: 'api' }] },
      {}
    );
    expect(mockLogToolCall).toHaveBeenCalledTimes(1);
    expect(mockLogToolCall).toHaveBeenCalledWith(
      'ghSearchCode',
      ['acme/api'],
      undefined,
      undefined,
      undefined
    );
  });

  it('basic wrapper: logToolCall fires on success', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(
      handler,
      'localSearchCode'
    )({ path: '/src' });
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

  it('basic wrapper with no toolName: logToolCall still fires', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(handler)({});
    expect(mockLogToolCall).toHaveBeenCalledTimes(1);
  });
});

describe('CORE-05: Timeout enforcement', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    setupDeps();
  });
  afterEach(() => {
    vi.useRealTimers();
    teardownDeps();
  });

  it('full wrapper: times out and returns error', async () => {
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
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

    const p1 = withSecurityValidation('t', handler, { timeoutMs: 5000 })(
      {},
      {}
    );
    const p2 = withBasicSecurityValidation(handler, 't', { timeoutMs: 5000 })(
      {}
    );

    vi.advanceTimersByTime(10);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.isError).toBe(false);
    expect(r2.isError).toBe(false);
  });
});

describe('CORE-06: AbortSignal cancellation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('full wrapper: already-aborted signal returns cancellation error before handler runs', async () => {
    const controller = new AbortController();
    controller.abort();
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withSecurityValidation('t', handler)(
      {},
      { signal: controller.signal }
    );
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('basic wrapper: already-aborted signal returns cancellation error before handler runs', async () => {
    const controller = new AbortController();
    controller.abort();
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(handler, 't')(
      {},
      { signal: controller.signal }
    );
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('full wrapper: aborting mid-flight resolves to cancellation result', async () => {
    const controller = new AbortController();
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withSecurityValidation('t', handler)(
      {},
      { signal: controller.signal }
    );
    controller.abort();
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });

  it('basic wrapper: aborting mid-flight resolves to cancellation result', async () => {
    const controller = new AbortController();
    const handler = vi.fn().mockReturnValue(new Promise(() => {}));
    const promise = withBasicSecurityValidation(handler, 't')(
      {},
      { signal: controller.signal }
    );
    controller.abort();
    const result = await promise;
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/cancelled/);
  });
});

describe('CORE-07: withSecurityValidation — auth/session passthrough', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('passes authInfo and sessionId to handler', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const authInfo = { userId: 'u1', token: 'tok' };
    await withSecurityValidation('t', handler)(
      {},
      { authInfo, sessionId: 'sess-123' }
    );
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

    expect(handler).toHaveBeenNthCalledWith(
      1,
      {},
      { user: 'alice' },
      undefined
    );
    expect(handler).toHaveBeenNthCalledWith(2, {}, { user: 'bob' }, undefined);
  });
});

describe('CORE-08: withBasicSecurityValidation — no auth contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupDeps();
  });
  afterEach(teardownDeps);

  it('handler receives only sanitizedArgs (no auth context)', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    await withBasicSecurityValidation(
      handler,
      'local_read'
    )({ path: '/workspace/file.ts' });
    expect(handler).toHaveBeenCalledWith({ path: '/workspace/file.ts' });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('optional extra.signal is respected', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
    const result = await withBasicSecurityValidation(handler, 't')(
      {},
      { signal: new AbortController().signal }
    );
    expect(result.isError).toBe(false);
  });

  it('missing extra argument does not throw', async () => {
    const handler = vi.fn().mockResolvedValue(SUCCESS_RESULT);
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

describe('CORE-09: configureSecurity applies to both wrappers', () => {
  afterEach(teardownDeps);

  it('defaultTimeoutMs from configureSecurity is respected by both wrappers', async () => {
    vi.useFakeTimers();
    configureSecurity({ defaultTimeoutMs: 100 });

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

  it('rejected handler returns error result without calling logSessionError', async () => {
    const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
    configureSecurity({ logSessionError: mockLogSessionError });
    const handler = vi.fn().mockRejectedValue(new Error('db exploded'));
    const result = await withSecurityValidation('tool', handler)({}, {});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('db exploded');
    await Promise.resolve();
    expect(mockLogSessionError).not.toHaveBeenCalled();
  });
});
