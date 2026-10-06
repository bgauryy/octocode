import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const startNativeMcp = vi.hoisted(() => vi.fn(async () => ({})));

vi.mock('../src/native/index.js', () => ({ startNativeMcp }));

type Handler = (chunk?: unknown) => void;

/** stdin/stdout/exit doubles for a failed start. */
function failedStart(failure: unknown) {
  startNativeMcp.mockRejectedValueOnce(failure);
  const stderr = vi
    .spyOn(process.stderr, 'write')
    .mockImplementation(() => true);
  const exit = vi
    .spyOn(process, 'exit')
    .mockImplementation((() => undefined) as never);
  const stdin = new Map<string, Handler>();
  const listen = ((event: string, handler: Handler) => {
    stdin.set(event, handler);
    return process.stdin;
  }) as never;
  vi.spyOn(process.stdin, 'setEncoding').mockImplementation(
    () => process.stdin
  );
  vi.spyOn(process.stdin, 'on').mockImplementation(listen);
  vi.spyOn(process.stdin, 'once').mockImplementation(listen);
  const stdout = vi.spyOn(process.stdout, 'write').mockImplementation(((
    _text: string,
    written?: () => void
  ) => {
    written?.();
    return true;
  }) as never);
  return { stderr, exit, stdin, stdout };
}

describe('native MCP process entry', () => {
  beforeEach(() => {
    vi.resetModules();
    startNativeMcp.mockReset();
    startNativeMcp.mockResolvedValue({});
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('starts the native adapter directly', async () => {
    await import('../src/index.js');
    await vi.waitFor(() => expect(startNativeMcp).toHaveBeenCalledOnce());
  });

  it.each([
    [new Error('native boom'), 'native boom'],
    ['native string failure', 'native string failure'],
    [null, 'unknown'],
  ])(
    'reports native startup failure %p and exits when stdin ends',
    async (failure, message) => {
      const { stderr, exit, stdin } = failedStart(failure);

      await import('../src/index.js');
      await vi.waitFor(() => expect(stdin.has('end')).toBe(true));
      expect(stderr).toHaveBeenCalledWith(
        `Server initialization failed: ${message}\n`
      );
      expect(exit).not.toHaveBeenCalled();
      stdin.get('end')!();
      expect(exit).toHaveBeenCalledWith(1);
    }
  );

  it('answers the client initialize with the failure reason before exiting', async () => {
    const reason =
      'Contract drift (fingerprint mismatch): rebuild native (yarn contracts:regen)';
    const { exit, stdin, stdout } = failedStart(new Error(reason));

    await import('../src/index.js');
    await vi.waitFor(() => expect(stdin.has('data')).toBe(true));
    stdin.get('data')!(
      '{"jsonrpc":"2.0","method":"notifications/x"}\n{"jsonrpc":'
    );
    expect(stdout).not.toHaveBeenCalled();
    stdin.get('data')!('"2.0","id":0,"method":"initialize","params":{}}\n');

    expect(stdout).toHaveBeenCalledOnce();
    expect(JSON.parse(String(stdout.mock.calls[0]![0]))).toEqual({
      jsonrpc: '2.0',
      id: 0,
      error: {
        code: -32603,
        message: `Server initialization failed: ${reason}`,
      },
    });
    expect(exit).toHaveBeenCalledOnce();
    expect(exit).toHaveBeenCalledWith(1);
  });

  it('exits when no client request arrives in time', async () => {
    vi.useFakeTimers();
    const { exit, stdin } = failedStart(new Error('native boom'));

    await import('../src/index.js');
    await vi.waitFor(() => expect(stdin.has('data')).toBe(true));
    vi.advanceTimersByTime(10_000);
    expect(exit).toHaveBeenCalledWith(1);
  });
});
