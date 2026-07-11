import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  configureSecurity,
  withSecurityValidation,
  withBasicSecurityValidation,
} from '../../src/security/withSecurityValidation.js';
import type {
  ISanitizer,
  ToolResult,
  ValidationResult,
} from '../../src/security/types.js';

function okResult(text = 'ok'): ToolResult {
  return { content: [{ type: 'text', text }] };
}

// A sanitizer that passes params straight through, marked valid. Individual
// tests override validateInputParameters to simulate rejection.
function passthroughSanitizer(): ISanitizer {
  return {
    sanitizeContent: (content: string) => ({
      content,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    }),
    validateInputParameters: (params): ValidationResult => ({
      sanitizedParams: params,
      isValid: true,
      hasSecrets: false,
      warnings: [],
    }),
  };
}

function errorText(r: ToolResult): string {
  return r.content[0]?.text ?? '';
}

describe('withSecurityValidation', () => {
  afterEach(() => {
    // Reset the module-level dependency overrides between tests.
    configureSecurity({ sanitizer: undefined, defaultTimeoutMs: undefined });
    vi.useRealTimers();
  });

  it('sanitizes args and forwards them (plus authInfo/sessionId) to the handler', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const handler = vi.fn(async () => okResult('done'));
    const wrapped = withSecurityValidation<{ q: string }, { user: string }>(
      'myTool',
      handler
    );

    const result = await wrapped(
      { q: 'hello' },
      { authInfo: { user: 'alice' }, sessionId: 'sess-1' }
    );

    expect(result.isError).toBeFalsy();
    expect(errorText(result)).toBe('done');
    expect(handler).toHaveBeenCalledWith(
      { q: 'hello' },
      { user: 'alice' },
      'sess-1'
    );
  });

  it('returns a security-validation error when the sanitizer rejects input', async () => {
    const sanitizer = passthroughSanitizer();
    sanitizer.validateInputParameters = () => ({
      sanitizedParams: {},
      isValid: false,
      hasSecrets: true,
      warnings: ['dangerous key detected', 'secret found'],
    });
    configureSecurity({ sanitizer });
    const handler = vi.fn(async () => okResult());
    const wrapped = withSecurityValidation('myTool', handler);

    const result = await wrapped({ bad: true }, {});

    expect(result.isError).toBe(true);
    expect(errorText(result)).toContain('Security validation failed');
    expect(errorText(result)).toContain('dangerous key detected; secret found');
    expect(handler).not.toHaveBeenCalled();
  });

  it('converts a thrown handler error into an error result instead of rejecting', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const wrapped = withSecurityValidation('myTool', async () => {
      throw new Error('boom');
    });

    const result = await wrapped({ q: 'x' }, {});

    expect(result.isError).toBe(true);
    expect(errorText(result)).toContain("Tool 'myTool' failed: boom");
  });

  it('wraps a non-Error handler rejection with an Unknown error message', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const wrapped = withSecurityValidation('myTool', async () => {
      throw 'string failure';
    });

    const result = await wrapped({ q: 'x' }, {});
    expect(result.isError).toBe(true);
    expect(errorText(result)).toContain('Unknown error');
  });

  it('times out a slow handler and returns a timeout error', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const wrapped = withSecurityValidation(
      'slowTool',
      () => new Promise<ToolResult>(() => {}), // never resolves
      { timeoutMs: 20 }
    );

    const result = await wrapped({ q: 'x' }, {});
    expect(result.isError).toBe(true);
    expect(errorText(result)).toMatch(/timed out after/);
  });

  it('returns "cancelled before execution" when the signal is already aborted', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const handler = vi.fn(() => new Promise<ToolResult>(() => {}));
    const wrapped = withSecurityValidation('myTool', handler);

    const result = await wrapped({ q: 'x' }, { signal: AbortSignal.abort() });

    expect(result.isError).toBe(true);
    expect(errorText(result)).toContain('cancelled before execution');
  });

  it('resolves with a client-cancellation error when the signal aborts mid-flight', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const controller = new AbortController();
    const wrapped = withSecurityValidation(
      'myTool',
      () => new Promise<ToolResult>(() => {}), // never resolves on its own
      { timeoutMs: 10_000 }
    );

    const pending = wrapped({ q: 'x' }, { signal: controller.signal });
    controller.abort();
    const result = await pending;

    expect(result.isError).toBe(true);
    expect(errorText(result)).toContain('cancelled by the client');
  });

  it('honors a defaultTimeoutMs set via configureSecurity', async () => {
    configureSecurity({
      sanitizer: passthroughSanitizer(),
      defaultTimeoutMs: 15,
    });
    const wrapped = withSecurityValidation(
      'slowTool',
      () => new Promise<ToolResult>(() => {})
    );

    const result = await wrapped({ q: 'x' }, {});
    expect(errorText(result)).toMatch(/timed out after/);
  });

  it('works when the extra argument is omitted entirely', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const wrapped = withSecurityValidation('myTool', async () =>
      okResult('bare')
    );
    const result = await wrapped({ q: 'x' });
    expect(errorText(result)).toBe('bare');
  });
});

describe('withBasicSecurityValidation', () => {
  afterEach(() => {
    configureSecurity({ sanitizer: undefined, defaultTimeoutMs: undefined });
  });

  it('runs the handler with sanitized args', async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const handler = vi.fn(async () => okResult('basic-done'));
    const wrapped = withBasicSecurityValidation(handler, 'basicTool');

    const result = await wrapped({ q: 'hi' });
    expect(errorText(result)).toBe('basic-done');
    expect(handler).toHaveBeenCalledWith({ q: 'hi' });
  });

  it("defaults the tool name to 'tool' when none is provided", async () => {
    configureSecurity({ sanitizer: passthroughSanitizer() });
    const wrapped = withBasicSecurityValidation(
      () => new Promise<ToolResult>(() => {}),
      undefined,
      { timeoutMs: 15 }
    );

    const result = await wrapped({ q: 'x' });
    expect(errorText(result)).toContain("Tool 'tool' timed out");
  });

  it('rejects invalid input before invoking the handler', async () => {
    const sanitizer = passthroughSanitizer();
    sanitizer.validateInputParameters = () => ({
      sanitizedParams: {},
      isValid: false,
      hasSecrets: false,
      warnings: ['nope'],
    });
    configureSecurity({ sanitizer });
    const handler = vi.fn(async () => okResult());
    const wrapped = withBasicSecurityValidation(handler, 'basicTool');

    const result = await wrapped({ bad: 1 });
    expect(result.isError).toBe(true);
    expect(handler).not.toHaveBeenCalled();
  });
});
