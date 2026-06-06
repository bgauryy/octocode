import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EventEmitter } from 'node:events';
import https from 'node:https';
import { getToken, getTokenType } from '../../../src/features/github-oauth.js';

const sharedMocks = vi.hoisted(() => ({
  maskToken: vi.fn((t: string) => `masked(${t})`),
  safeTokenOutput: vi.fn((t: string) => `safe(${t})`),
  formatTokenSource: vi.fn((s: string) => `source:${s}`),
  printLoginHint: vi.fn(() => console.log('LOGIN_HINT')),
}));

const spinnerMocks = vi.hoisted(() => {
  const stop = vi.fn();
  const start = vi.fn();
  class FakeSpinner {
    start(..._a: unknown[]) {
      start(..._a);
      return this;
    }
    stop(..._a: unknown[]) {
      stop(..._a);
      return this;
    }
  }
  return {
    stop,
    start,
    SpinnerCtor: FakeSpinner,
  };
});

vi.mock('node:https', () => ({
  default: {
    request: vi.fn(),
  },
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_tag: string, text: string) => text,
  bold: (text: string) => text,
  dim: (text: string) => text,
}));

vi.mock('../../../src/features/github-oauth.js', () => ({
  getToken: vi.fn(),
  getTokenType: vi.fn().mockReturnValue('octocode'),
}));

vi.mock('../../../src/cli/commands/shared.js', () => ({
  maskToken: sharedMocks.maskToken,
  safeTokenOutput: sharedMocks.safeTokenOutput,
  formatTokenSource: sharedMocks.formatTokenSource,
  printLoginHint: sharedMocks.printLoginHint,
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: spinnerMocks.SpinnerCtor,
}));

type PingScenario =
  | { kind: 'status'; statusCode: number }
  | {
      kind: 'json';
      body: string;
      headers?: Record<string, string>;
    }
  | { kind: 'error'; message: string }
  | { kind: 'timeout' };

function installHttpsMock(scenario: PingScenario): void {
  vi.mocked(https.request).mockImplementation(((
    _opts: unknown,
    cb: (res: EventEmitter & { statusCode?: number; headers?: unknown }) => void
  ) => {
    const req = new EventEmitter() as EventEmitter & {
      end: () => void;
      destroy: () => void;
    };
    req.end = vi.fn(() => {
      if (scenario.kind === 'error') {
        process.nextTick(() => req.emit('error', new Error(scenario.message)));
        return;
      }
      if (scenario.kind === 'timeout') {
        process.nextTick(() => req.emit('timeout'));
        return;
      }
      const res = new EventEmitter() as EventEmitter & {
        statusCode?: number;
        headers?: Record<string, string>;
      };
      if (scenario.kind === 'status') {
        res.statusCode = scenario.statusCode;
        res.headers = {};
      } else {
        res.statusCode = 200;
        res.headers = scenario.headers ?? {};
      }
      process.nextTick(() => {
        cb(res);
        if (scenario.kind === 'json') {
          res.emit('data', Buffer.from(scenario.body));
        }
        res.emit('end');
      });
    }) as () => void;
    req.destroy = vi.fn();
    return req;
  }) as unknown as typeof https.request);
}

async function loadCommand() {
  const mod = await import('../../../src/cli/commands/token.js');
  return mod.tokenCommand;
}

describe('tokenCommand', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    sharedMocks.maskToken.mockImplementation((t: string) => `masked(${t})`);
    sharedMocks.safeTokenOutput.mockImplementation((t: string) => `safe(${t})`);
    sharedMocks.formatTokenSource.mockImplementation(
      (s: string) => `source:${s}`
    );
    sharedMocks.printLoginHint.mockImplementation(() =>
      console.log('LOGIN_HINT')
    );
    vi.mocked(getTokenType).mockReturnValue('octocode');
  });

  afterEach(() => {
    logSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  const out = (needle: string) =>
    logSpy.mock.calls.some((call: unknown[]) =>
      String(call.join(' ')).includes(needle)
    );

  it('exports the expected command metadata', async () => {
    const cmd = await loadCommand();
    expect(cmd.name).toBe('token');
    expect(cmd.aliases).toContain('t');
    expect(cmd.options?.length).toBeGreaterThan(0);
  });

  it.each([['octocode'], ['octocode-cli'], ['o']])(
    'maps type "%s" to octocode source',
    async typeArg => {
      vi.mocked(getToken).mockResolvedValue({
        token: 'tok-octocode',
        source: 'octocode',
      } as never);
      const cmd = await loadCommand();
      await cmd.handler({
        command: 'token',
        args: [],
        options: { type: typeArg },
      });
      expect(getToken).toHaveBeenCalledWith('github.com', 'octocode');
    }
  );

  it.each([['gh'], ['gh-cli'], ['g']])(
    'maps type "%s" to gh source',
    async typeArg => {
      vi.mocked(getToken).mockResolvedValue({
        token: 'tok-gh',
        source: 'gh-cli',
      } as never);
      const cmd = await loadCommand();
      await cmd.handler({
        command: 'token',
        args: [],
        options: { type: typeArg },
      });
      expect(getToken).toHaveBeenCalledWith('github.com', 'gh');
    }
  );

  it.each([['auto'], ['a']])('maps type "%s" to auto source', async typeArg => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'tok-auto',
      source: 'octocode',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { type: typeArg },
    });
    expect(getToken).toHaveBeenCalledWith('github.com', 'auto');
  });

  it('uses -t alias for type and -H alias for hostname', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'tok',
      source: 'gh-cli',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { t: 'gh', H: 'ghe.example.com' },
    });
    expect(getToken).toHaveBeenCalledWith('ghe.example.com', 'gh');
  });

  it('invalid type (non-json) prints error and sets exitCode 1', async () => {
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { type: 'bogus' },
    });
    expect(process.exitCode).toBe(1);
    expect(out('Invalid token type: bogus')).toBe(true);
    expect(getToken).not.toHaveBeenCalled();
  });

  it('invalid type with --json prints json none and sets exitCode 1', async () => {
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { type: 'bogus', json: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('"type":"none"')).toBe(true);
  });

  it('--json with no token prints null token and exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: null,
      source: 'none',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { json: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('"valid":false')).toBe(true);
  });

  it('--json without validate prints token and type', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    vi.mocked(getTokenType).mockReturnValue('octocode');
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { json: true },
    });
    expect(process.exitCode).toBeUndefined();
    expect(out('"token":"abc123"')).toBe(true);
    expect(out('"type":"octocode"')).toBe(true);
  });

  it('--json -j alias is honored', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { j: true },
    });
    expect(out('"token":"abc123"')).toBe(true);
  });

  it('--json --validate valid token prints valid:true with login', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({
      kind: 'json',
      body: JSON.stringify({ login: 'octocat' }),
      headers: {
        'x-ratelimit-remaining': '4999',
        'x-ratelimit-limit': '5000',
        'x-ratelimit-reset': '1700000000',
      },
    });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { json: true, validate: true },
    });
    expect(process.exitCode).toBeUndefined();
    expect(out('"valid":true')).toBe(true);
    expect(out('octocat')).toBe(true);
  });

  it('--json --validate invalid token sets exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'status', statusCode: 401 });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { json: true, validate: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('"valid":false')).toBe(true);
  });

  it('no octocode token prints octocode login hint and exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: null,
      source: 'none',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { type: 'octocode' },
    });
    expect(process.exitCode).toBe(1);
    expect(out('No Octocode token found')).toBe(true);
    expect(out('octocode login')).toBe(true);
  });

  it('no gh token prints gh login hint and exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: null,
      source: 'none',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { type: 'gh' },
    });
    expect(process.exitCode).toBe(1);
    expect(out('No gh CLI token found')).toBe(true);
    expect(out('gh auth login')).toBe(true);
  });

  it('no auto token prints generic login hint and exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: null,
      source: 'none',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(out('Not authenticated')).toBe(true);
    expect(sharedMocks.printLoginHint).toHaveBeenCalled();
  });

  it('--validate valid token with rate limit prints success', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({
      kind: 'json',
      body: JSON.stringify({ login: 'octocat' }),
      headers: {
        'x-ratelimit-remaining': '10',
        'x-ratelimit-limit': '5000',
        'x-ratelimit-reset': '1700000000',
      },
    });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(process.exitCode).toBeUndefined();
    expect(out('Token is valid')).toBe(true);
    expect(out('Rate limit:')).toBe(true);
    expect(spinnerMocks.stop).toHaveBeenCalled();
  });

  it('--validate valid token without login uses unknown', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({
      kind: 'json',
      body: JSON.stringify({ login: 'someone' }),
      headers: {},
    });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(out('Token is valid')).toBe(true);
  });

  it('--validate invalid token (request error) prints failure and exitCode 1', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'error', message: 'ECONNREFUSED' });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('Token validation failed')).toBe(true);
    expect(out('ECONNREFUSED')).toBe(true);
  });

  it('--validate timeout prints timed out failure', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'timeout' });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('Request timed out')).toBe(true);
  });

  it('--validate invalid API response (bad json) prints failure', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'json', body: 'not-json{' });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('Invalid API response')).toBe(true);
  });

  it('--validate 200 but no login field is treated invalid (default error)', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'json', body: JSON.stringify({ message: 'x' }) });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true },
    });
    expect(process.exitCode).toBe(1);
    expect(out('unknown error')).toBe(true);
  });

  it('uses GitHub Enterprise api path for non-github.com hostname', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    installHttpsMock({ kind: 'json', body: JSON.stringify({ login: 'ent' }) });
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { validate: true, hostname: 'ghe.corp.com' },
    });
    const opts = vi.mocked(https.request).mock.calls[0][0] as unknown as {
      hostname: string;
      path: string;
    };
    expect(opts.hostname).toBe('ghe.corp.com');
    expect(opts.path).toContain('/api/v3/user');
  });

  it('--source prints token source and user', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
      username: 'me',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { source: true },
    });
    expect(out('Token found')).toBe(true);
    expect(out('@me')).toBe(true);
    expect(sharedMocks.maskToken).toHaveBeenCalledWith('abc123');
    expect(sharedMocks.formatTokenSource).toHaveBeenCalled();
  });

  it('--source without username omits user line', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: { s: true },
    });
    expect(out('Token found')).toBe(true);
    expect(out('@')).toBe(false);
    expect(sharedMocks.maskToken).toHaveBeenCalledWith('abc123');
  });

  it('default output prints safe token', async () => {
    vi.mocked(getToken).mockResolvedValue({
      token: 'abc123',
      source: 'octocode',
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'token',
      args: [],
      options: {},
    });
    expect(sharedMocks.safeTokenOutput).toHaveBeenCalledWith('abc123');
    expect(out('safe(abc123)')).toBe(true);
  });
});
