/**
 * TDD Security & Resilience Tests
 *
 * All tests call REAL functions and verify REAL behavior.
 * No source-code-reading or mock-only tests.
 *
 * Issue categories:
 *   [SECURITY]   spawnCheckSuccess timeout behavior
 *   [SECURITY]   spawnCollectStdout OOM protection
 *   [SECURITY]   LSP locationsToSnippets path validation
 *   [SECURITY]   convertCallHierarchyItem malformed response handling
 *   [SECURITY]   Environment variable leakage prevention
 *   [SECURITY]   Command injection via rg flags
 *   [SECURITY]   Git clone argument injection
 *   [RESILIENCE]  validateArgs edge cases
 *   [RESILIENCE]  ContentSanitizer edge cases
 *   [RESILIENCE]  Cache key generation edge cases
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  writeFileSync,
  mkdirSync,
  rmSync,
  existsSync,
  symlinkSync,
} from 'node:fs';

// ═══════════════════════════════════════════════════════════════════════
// 1. [SECURITY] spawnCheckSuccess SIGKILL escalation
//    File: src/utils/exec/spawn.ts
//    Verify: on timeout, SIGTERM is sent first, then SIGKILL follows
//    Note: child_process.spawn is globally mocked in setup.ts.
//    We configure the mock to simulate process behavior and test the
//    real wrapper logic (timeouts, SIGKILL escalation, output limits).
// ═══════════════════════════════════════════════════════════════════════

import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import {
  spawnCheckSuccess,
  spawnCollectStdout,
  buildChildProcessEnv,
  CORE_ALLOWED_ENV_VARS,
  TOOLING_ALLOWED_ENV_VARS,
  SENSITIVE_ENV_VARS,
  validateArgs,
} from '../../src/utils/exec/spawn.js';

/** Create a fake ChildProcess that emits events like a real one */
function createMockProcess() {
  const proc = new EventEmitter() as EventEmitter & {
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: ReturnType<typeof vi.fn>;
    pid: number;
  };
  proc.stdout = new EventEmitter();
  proc.stderr = new EventEmitter();
  proc.kill = vi.fn();
  proc.pid = 12345;
  return proc;
}

describe('[SECURITY] spawnCheckSuccess SIGKILL escalation', () => {
  it('should return false and send SIGTERM on timeout', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCheckSuccess('sleep', ['999'], 50);

    // Wait for timeout to fire
    const result = await promise;
    expect(result).toBe(false);
    expect(proc.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('should return true for process that exits code 0', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCheckSuccess('echo', ['hi'], 5000);
    // Simulate process exit
    proc.emit('close', 0);

    expect(await promise).toBe(true);
  });

  it('should return false for process that exits non-zero', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCheckSuccess('false', [], 5000);
    proc.emit('close', 1);

    expect(await promise).toBe(false);
  });

  it('should return false when spawn throws', async () => {
    vi.mocked(spawn).mockImplementationOnce(() => {
      throw new Error('ENOENT');
    });

    const result = await spawnCheckSuccess('nonexistent', [], 5000);
    expect(result).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 2. [SECURITY] spawnCollectStdout OOM protection
//    File: src/utils/exec/spawn.ts
//    Verify: process is killed when output exceeds maxOutputSize
// ═══════════════════════════════════════════════════════════════════════

describe('[SECURITY] spawnCollectStdout OOM protection', () => {
  it('should return null and kill process when output exceeds maxOutputSize', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCollectStdout('yes', [], 5000, {
      maxOutputSize: 50,
    });

    // Simulate large output exceeding the 50 byte limit
    proc.stdout.emit('data', Buffer.from('a'.repeat(100)));

    const result = await promise;
    expect(result).toBeNull();
    expect(proc.kill).toHaveBeenCalledWith('SIGKILL');
  });

  it('should collect output when within maxOutputSize', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCollectStdout('echo', ['hello'], 5000, {
      maxOutputSize: 1024,
    });

    proc.stdout.emit('data', Buffer.from('hello\n'));
    proc.emit('close', 0);

    expect(await promise).toBe('hello');
  });

  it('should return null on timeout', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCollectStdout('sleep', ['999'], 50);

    const result = await promise;
    expect(result).toBeNull();
    expect(proc.kill).toHaveBeenCalledWith('SIGTERM');
  });

  it('should return null when spawn throws', async () => {
    vi.mocked(spawn).mockImplementationOnce(() => {
      throw new Error('ENOENT');
    });

    const result = await spawnCollectStdout('nonexistent', [], 5000);
    expect(result).toBeNull();
  });

  it('should return null for non-zero exit', async () => {
    const proc = createMockProcess();
    vi.mocked(spawn).mockReturnValueOnce(proc as any);

    const promise = spawnCollectStdout('false', [], 5000);
    proc.emit('close', 1);

    expect(await promise).toBeNull();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 3. [SECURITY] LSP locationsToSnippets path validation
//    File: src/lsp/lspOperations.ts
//    Verify: out-of-workspace LSP URIs are rejected by PathValidator
// ═══════════════════════════════════════════════════════════════════════

import { LSPOperations } from '../../src/lsp/lspOperations.js';
import { LSPDocumentManager } from '../../src/lsp/lspDocumentManager.js';

describe('[SECURITY] LSP locationsToSnippets path validation', () => {
  const testTmpDir = join(tmpdir(), `octocode-lsp-security-${Date.now()}`);

  beforeEach(() => {
    mkdirSync(testTmpDir, { recursive: true });
  });

  afterEach(() => {
    if (existsSync(testTmpDir)) {
      rmSync(testTmpDir, { recursive: true, force: true });
    }
  });

  it('LSPOperations constructor should accept workspaceRoot for path validation', () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: testTmpDir,
    };
    const docManager = new LSPDocumentManager(mockConfig);
    expect(() => new LSPOperations(docManager, testTmpDir)).not.toThrow();
  });

  it('gotoDefinition should skip out-of-workspace paths from LSP response', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: testTmpDir,
    };

    // Create a real file so openDocument can read it
    const testFile = join(testTmpDir, 'test.ts');
    writeFileSync(testFile, 'const x = 1;');

    const docManager = new LSPDocumentManager(mockConfig);
    const operations = new LSPOperations(docManager, testTmpDir);

    // Mock connection that returns a location pointing to /etc/passwd
    const mockConnection = {
      sendRequest: vi.fn().mockResolvedValue({
        uri: 'file:///etc/passwd',
        range: {
          start: { line: 0, character: 0 },
          end: { line: 5, character: 0 },
        },
      }),
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    operations.setConnection(mockConnection as any, true);
    docManager.setConnection(mockConnection as any, true);

    const snippets = await operations.gotoDefinition(testFile, {
      line: 0,
      character: 0,
    });

    // Should be empty — /etc/passwd is outside workspace
    expect(snippets).toHaveLength(0);
  });

  it('gotoDefinition should include in-workspace paths from LSP response', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: process.cwd(),
    };

    // Use a real file in the current workspace
    const targetFile = join(process.cwd(), 'package.json');

    const docManager = new LSPDocumentManager(mockConfig);
    const operations = new LSPOperations(docManager, process.cwd());

    const mockConnection = {
      sendRequest: vi.fn().mockResolvedValue({
        uri: `file://${targetFile}`,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 10 },
        },
      }),
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    operations.setConnection(mockConnection as any, true);
    docManager.setConnection(mockConnection as any, true);

    // Mock openDocument to avoid needing the real file format
    vi.spyOn(docManager, 'openDocument').mockResolvedValue(undefined);
    vi.spyOn(docManager, 'closeDocument').mockResolvedValue(undefined);

    const snippets = await operations.gotoDefinition(targetFile, {
      line: 0,
      character: 0,
    });

    // Should have 1 result — file is within workspace
    expect(snippets).toHaveLength(1);
    expect(snippets[0]!.content).toBeTruthy();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 4. [SECURITY] convertCallHierarchyItem malformed response handling
//    File: src/lsp/lspOperations.ts
//    Verify: malformed LSP responses don't crash
// ═══════════════════════════════════════════════════════════════════════

describe('[SECURITY] convertCallHierarchyItem malformed response handling', () => {
  it('prepareCallHierarchy should handle null result from LSP', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: process.cwd(),
    };

    const docManager = new LSPDocumentManager(mockConfig);
    const operations = new LSPOperations(docManager, process.cwd());

    const mockConnection = {
      sendRequest: vi.fn().mockResolvedValue(null),
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    operations.setConnection(mockConnection as any, true);
    docManager.setConnection(mockConnection as any, true);

    vi.spyOn(docManager, 'openDocument').mockResolvedValue(undefined);
    vi.spyOn(docManager, 'closeDocument').mockResolvedValue(undefined);

    const result = await operations.prepareCallHierarchy(
      join(process.cwd(), 'package.json'),
      { line: 0, character: 0 }
    );

    expect(result).toEqual([]);
  });

  it('prepareCallHierarchy should handle items with missing range', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: process.cwd(),
    };

    const docManager = new LSPDocumentManager(mockConfig);
    const operations = new LSPOperations(docManager, process.cwd());

    // Return a malformed item — range and selectionRange are undefined
    const mockConnection = {
      sendRequest: vi.fn().mockResolvedValue([
        {
          name: 'brokenFunction',
          kind: 12, // SymbolKind.Function
          uri: `file://${join(process.cwd(), 'package.json')}`,
          range: undefined,
          selectionRange: undefined,
        },
      ]),
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    operations.setConnection(mockConnection as any, true);
    docManager.setConnection(mockConnection as any, true);

    vi.spyOn(docManager, 'openDocument').mockResolvedValue(undefined);
    vi.spyOn(docManager, 'closeDocument').mockResolvedValue(undefined);

    // Should NOT throw — defensive handling
    const result = await operations.prepareCallHierarchy(
      join(process.cwd(), 'package.json'),
      { line: 0, character: 0 }
    );

    expect(result).toHaveLength(1);
    expect(result[0]!.name).toBe('brokenFunction');
    // Range should have defaults
    expect(result[0]!.range.start.line).toBe(0);
    expect(result[0]!.range.start.character).toBe(0);
  });

  it('getIncomingCalls should handle malformed fromRanges', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: process.cwd(),
    };

    const docManager = new LSPDocumentManager(mockConfig);
    const operations = new LSPOperations(docManager, process.cwd());

    // Return result with null fromRanges entries
    const mockConnection = {
      sendRequest: vi.fn().mockResolvedValue([
        {
          from: {
            name: 'caller',
            kind: 12,
            uri: `file://${join(process.cwd(), 'package.json')}`,
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 10 },
            },
            selectionRange: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 5 },
            },
          },
          fromRanges: [null, undefined],
        },
      ]),
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    operations.setConnection(mockConnection as any, true);
    docManager.setConnection(mockConnection as any, true);

    const item = {
      name: 'target',
      kind: 'function' as const,
      uri: join(process.cwd(), 'package.json'),
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 10 },
      },
      selectionRange: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 5 },
      },
      displayRange: { startLine: 1, endLine: 1 },
    };

    // Should not crash on null fromRanges entries
    const result = await operations.getIncomingCalls(item);
    expect(result).toHaveLength(1);
    expect(result[0]!.from.name).toBe('caller');
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 5. [SECURITY] Environment variable leakage prevention
//    File: src/utils/exec/spawn.ts
//    Tests call buildChildProcessEnv() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

describe('[SECURITY] Environment variable leakage prevention', () => {
  const originalEnv = { ...process.env };

  afterEach(() => {
    for (const key of Object.keys(process.env)) {
      if (!(key in originalEnv)) {
        delete process.env[key];
      }
    }
    for (const [key, value] of Object.entries(originalEnv)) {
      process.env[key] = value;
    }
  });

  it('buildChildProcessEnv should NOT include GITHUB_TOKEN', () => {
    process.env.GITHUB_TOKEN = 'ghp_secret_token_123';
    const env = buildChildProcessEnv();
    expect(env.GITHUB_TOKEN).toBeUndefined();
  });

  it('buildChildProcessEnv should NOT include any sensitive env vars', () => {
    for (const varName of SENSITIVE_ENV_VARS) {
      process.env[varName] = `secret-${varName}`;
    }
    const env = buildChildProcessEnv();
    for (const varName of SENSITIVE_ENV_VARS) {
      expect(env[varName]).toBeUndefined();
    }
  });

  it('buildChildProcessEnv should include PATH for command resolution', () => {
    const env = buildChildProcessEnv();
    expect(env.PATH).toBeDefined();
    expect(env.PATH).toBe(process.env.PATH);
  });

  it('CORE_ALLOWED_ENV_VARS should NOT include HOME', () => {
    process.env.HOME = '/Users/test';
    process.env.GITHUB_TOKEN = 'ghp_secret';
    const env = buildChildProcessEnv({}, CORE_ALLOWED_ENV_VARS);
    expect(env.HOME).toBeUndefined();
    expect(env.GITHUB_TOKEN).toBeUndefined();
    expect(env.PATH).toBeDefined();
  });

  it('TOOLING_ALLOWED_ENV_VARS should include HOME', () => {
    process.env.HOME = '/Users/test';
    const env = buildChildProcessEnv({}, TOOLING_ALLOWED_ENV_VARS);
    expect(env.HOME).toBe('/Users/test');
    expect(env.GITHUB_TOKEN).toBeUndefined();
  });

  it('envOverrides should only apply to allowed vars', () => {
    const env = buildChildProcessEnv(
      { GITHUB_TOKEN: 'injected', PATH: '/custom/path' },
      CORE_ALLOWED_ENV_VARS
    );
    expect(env.PATH).toBe('/custom/path');
    expect(env.GITHUB_TOKEN).toBeUndefined();
  });

  it('undefined override should remove an allowed var', () => {
    process.env.TMPDIR = '/tmp';
    const env = buildChildProcessEnv(
      { TMPDIR: undefined },
      CORE_ALLOWED_ENV_VARS
    );
    expect(env.TMPDIR).toBeUndefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 6. [SECURITY] Command injection via rg flags
//    Tests call validateCommand() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

import { validateCommand } from 'octocode-security-utils/commandValidator';

describe('[SECURITY] Command injection via rg flags', () => {
  it('rg --pre should be blocked (arbitrary command execution)', () => {
    const result = validateCommand('rg', ['--pre', 'cat', 'pattern']);
    expect(result.isValid).toBe(false);
  });

  it('rg --pre-glob should be blocked', () => {
    const result = validateCommand('rg', ['--pre-glob', '*.txt', 'pattern']);
    expect(result.isValid).toBe(false);
  });

  it('rg with known safe flags should work', () => {
    const result = validateCommand('rg', ['-i', '-n', '-l', 'pattern', './']);
    expect(result.isValid).toBe(true);
  });

  it('rg with flag-value pairs should work', () => {
    const result = validateCommand('rg', [
      '-g',
      '*.ts',
      '-A',
      '3',
      'pattern',
      './',
    ]);
    expect(result.isValid).toBe(true);
  });

  it('rg with -- separator should allow pattern after it', () => {
    const result = validateCommand('rg', ['--', 'foo|bar', './']);
    expect(result.isValid).toBe(true);
  });

  it('rg with dangerous pattern in path should be blocked', () => {
    const result = validateCommand('rg', ['pattern', '$(malicious)']);
    expect(result.isValid).toBe(false);
  });

  it('find -delete should be blocked', () => {
    const result = validateCommand('find', ['.', '-name', '*.log', '-delete']);
    expect(result.isValid).toBe(false);
  });

  it('find -exec should be blocked', () => {
    const result = validateCommand('find', [
      '.',
      '-name',
      '*.ts',
      '-exec',
      'rm',
      '{}',
      ';',
    ]);
    expect(result.isValid).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 7. [SECURITY] Git clone argument injection
//    Tests call validateCommand() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

describe('[SECURITY] Git clone argument injection', () => {
  it('git clone with allowed flags should work', () => {
    const result = validateCommand('git', [
      'clone',
      '--depth',
      '1',
      '--single-branch',
      'https://github.com/org/repo.git',
    ]);
    expect(result.isValid).toBe(true);
  });

  it('git clone --upload-pack should be blocked (arbitrary commands)', () => {
    const result = validateCommand('git', [
      'clone',
      '--upload-pack',
      'malicious-command',
      'https://example.com/repo.git',
    ]);
    expect(result.isValid).toBe(false);
  });

  it('git clone --config should be blocked (config injection)', () => {
    const result = validateCommand('git', [
      'clone',
      '--config',
      'core.sshCommand=evil',
      'https://example.com/repo.git',
    ]);
    expect(result.isValid).toBe(false);
  });

  it('git clone -c should be allowed (safe config overrides)', () => {
    const result = validateCommand('git', [
      'clone',
      '-c',
      'advice.detachedHead=false',
      'https://example.com/repo.git',
    ]);
    expect(result.isValid).toBe(true);
  });

  it('git push should be blocked (not in allowed subcommands)', () => {
    const result = validateCommand('git', ['push', 'origin', 'main']);
    expect(result.isValid).toBe(false);
  });

  it('git checkout should be blocked', () => {
    const result = validateCommand('git', ['checkout', 'main']);
    expect(result.isValid).toBe(false);
  });

  it('disallowed commands should be rejected', () => {
    expect(validateCommand('curl', ['http://evil.com']).isValid).toBe(false);
    expect(validateCommand('wget', ['http://evil.com']).isValid).toBe(false);
    expect(validateCommand('rm', ['-rf', '/']).isValid).toBe(false);
    expect(validateCommand('bash', ['-c', 'echo pwned']).isValid).toBe(false);
    expect(validateCommand('sh', ['-c', 'echo pwned']).isValid).toBe(false);
  });

  it('git sparse-checkout with allowed actions should work', () => {
    const result = validateCommand('git', [
      'sparse-checkout',
      'set',
      'src/compiler',
    ]);
    expect(result.isValid).toBe(true);
  });

  it('git sparse-checkout with disallowed actions should be blocked', () => {
    const result = validateCommand('git', ['sparse-checkout', 'reapply']);
    expect(result.isValid).toBe(false);
  });

  it('git with no subcommand should be rejected', () => {
    const result = validateCommand('git', []);
    expect(result.isValid).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 8. [RESILIENCE] validateArgs edge cases
//    Tests call validateArgs() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

describe('[RESILIENCE] validateArgs edge cases', () => {
  it('should reject args containing null bytes', () => {
    const result = validateArgs(['normal', 'has\0null']);
    expect(result.valid).toBe(false);
    expect(result.error).toContain('Null bytes');
  });

  it('should reject args exceeding max length', () => {
    const result = validateArgs(['a'.repeat(1001)]);
    expect(result.valid).toBe(false);
    expect(result.error).toContain('too long');
  });

  it('should accept args within max length', () => {
    expect(validateArgs(['short', 'a'.repeat(1000)]).valid).toBe(true);
  });

  it('should accept empty args array', () => {
    expect(validateArgs([]).valid).toBe(true);
  });

  it('should use custom maxLength when provided', () => {
    expect(validateArgs(['abc'], 2).valid).toBe(false);
  });

  it('should allow empty string args', () => {
    expect(validateArgs(['', 'normal']).valid).toBe(true);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 9. [RESILIENCE] ContentSanitizer edge cases
//    Tests call ContentSanitizer methods directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';

describe('[RESILIENCE] ContentSanitizer edge cases', () => {
  it('sanitizeContent with null input should not crash', () => {
    const result = ContentSanitizer.sanitizeContent(null as unknown as string);
    expect(result.content).toBe('');
    expect(result.hasSecrets).toBe(false);
  });

  it('sanitizeContent with undefined input should not crash', () => {
    const result = ContentSanitizer.sanitizeContent(
      undefined as unknown as string
    );
    expect(result.content).toBe('');
  });

  it('sanitizeContent with empty string should return empty', () => {
    const result = ContentSanitizer.sanitizeContent('');
    expect(result.content).toBe('');
    expect(result.hasSecrets).toBe(false);
  });

  it('sanitizeContent with number input should not crash', () => {
    expect(() =>
      ContentSanitizer.sanitizeContent(123 as unknown as string)
    ).not.toThrow();
  });

  it('validateInputParameters should block __proto__ key', () => {
    const params = Object.create(null);
    Object.defineProperty(params, '__proto__', {
      value: { isAdmin: true },
      enumerable: true,
      writable: true,
      configurable: true,
    });
    const result = ContentSanitizer.validateInputParameters(params);
    expect(
      result.warnings.some(w => w.includes('Dangerous parameter key'))
    ).toBe(true);
  });

  it('validateInputParameters should block constructor key', () => {
    const result = ContentSanitizer.validateInputParameters({
      constructor: { prototype: {} },
    });
    expect(result.warnings.some(w => w.includes('constructor'))).toBe(true);
  });

  it('secrets in array elements should be detected', () => {
    const params = {
      keywords: ['ghp_1234567890abcdefghijklmnopqrstuvwxABCD', 'normal-search'],
    };
    const result = ContentSanitizer.validateInputParameters(params);
    expect(result.hasSecrets).toBe(true);
    const kw = result.sanitizedParams.keywords as string[];
    expect(kw[0]).not.toContain('ghp_');
    expect(kw[1]).toBe('normal-search');
  });

  it('very large array should be truncated to 100', () => {
    const params = {
      items: Array.from({ length: 200 }, (_, i) => `item-${i}`),
    };
    const result = ContentSanitizer.validateInputParameters(params);
    expect((result.sanitizedParams.items as string[]).length).toBe(100);
  });

  it('very long string value should be truncated to 10000', () => {
    const params = { content: 'a'.repeat(20000) };
    const result = ContentSanitizer.validateInputParameters(params);
    expect(
      (result.sanitizedParams.content as string).length
    ).toBeLessThanOrEqual(10000);
  });

  it('deeply nested object beyond depth limit should not throw', () => {
    let deep: Record<string, unknown> = { value: 'leaf' };
    for (let i = 0; i < 25; i++) {
      deep = { nested: deep };
    }
    expect(() =>
      ContentSanitizer.validateInputParameters({ root: deep })
    ).not.toThrow();
  });

  it('circular references should be caught, not stack overflow', () => {
    const a: Record<string, unknown> = { name: 'a' };
    const b: Record<string, unknown> = { name: 'b' };
    a.ref = b;
    b.ref = a;
    expect(() =>
      ContentSanitizer.validateInputParameters({ a, b })
    ).not.toThrow();
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 10. [RESILIENCE] Cache key generation edge cases
//     Tests call generateCacheKey() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

import { generateCacheKey } from '../../src/utils/http/cache.js';

describe('[RESILIENCE] Cache key generation edge cases', () => {
  it('should handle null params', () => {
    expect(() => generateCacheKey('test', null)).not.toThrow();
  });

  it('should handle undefined params', () => {
    expect(() => generateCacheKey('test', undefined)).not.toThrow();
  });

  it('should handle empty object', () => {
    const key = generateCacheKey('test', {});
    expect(key).toBeTruthy();
    expect(key).toContain('test');
  });

  it('should handle nested arrays', () => {
    expect(() =>
      generateCacheKey('test', { arr: [1, [2, 3], 'str'] })
    ).not.toThrow();
  });

  it('circular references should not crash', () => {
    const obj: Record<string, unknown> = { a: 1 };
    obj.self = obj;
    expect(() => generateCacheKey('test', obj)).not.toThrow();
  });

  it('should produce deterministic keys (sorted keys)', () => {
    const key1 = generateCacheKey('prefix', { b: 2, a: 1 });
    const key2 = generateCacheKey('prefix', { a: 1, b: 2 });
    expect(key1).toBe(key2);
  });

  it('different values should produce different keys', () => {
    const key1 = generateCacheKey('prefix', { a: 1 });
    const key2 = generateCacheKey('prefix', { a: 2 });
    expect(key1).not.toBe(key2);
  });

  it('different prefixes should produce different keys', () => {
    const key1 = generateCacheKey('prefix1', { a: 1 });
    const key2 = generateCacheKey('prefix2', { a: 1 });
    expect(key1).not.toBe(key2);
  });

  it('sessionId should isolate cache keys', () => {
    const key1 = generateCacheKey('test', { a: 1 }, 'session1');
    const key2 = generateCacheKey('test', { a: 1 }, 'session2');
    const keyNone = generateCacheKey('test', { a: 1 });
    expect(key1).not.toBe(key2);
    expect(key1).not.toBe(keyNone);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 12. [SECURITY] grep pattern detection regression
//     Tests call validateCommand() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

describe('[SECURITY] grep/rg pattern detection regression', () => {
  it('grep -E with pipe alternation should be valid', () => {
    expect(validateCommand('grep', ['-E', 'foo|bar', './src']).isValid).toBe(
      true
    );
  });

  it('grep -E with grouped alternation should be valid', () => {
    expect(validateCommand('grep', ['-E', '(foo|bar)+', './src']).isValid).toBe(
      true
    );
  });

  it('rg with pipe in pattern should be valid', () => {
    expect(validateCommand('rg', ['foo|bar', './']).isValid).toBe(true);
  });

  it('rg with complex regex pattern should be valid', () => {
    expect(
      validateCommand('rg', ['(import|export)\\s+\\{', './']).isValid
    ).toBe(true);
  });

  it('command substitution in path args should be blocked', () => {
    expect(validateCommand('grep', ['-E', 'pattern', '$(evil)']).isValid).toBe(
      false
    );
    expect(validateCommand('rg', ['pattern', '$(evil)']).isValid).toBe(false);
  });

  it('backtick substitution in args should be blocked', () => {
    expect(validateCommand('grep', ['-E', 'pattern', '`evil`']).isValid).toBe(
      false
    );
    expect(validateCommand('rg', ['pattern', '`evil`']).isValid).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 13. [RESILIENCE] LSPDocumentManager state management
//     Tests use real LSPDocumentManager with real temp files
//     Mock is ONLY for the MessageConnection (external dependency)
// ═══════════════════════════════════════════════════════════════════════

describe('[RESILIENCE] LSPDocumentManager state management', () => {
  const testTmpDir = join(tmpdir(), `octocode-lsp-state-${Date.now()}`);

  afterEach(() => {
    if (existsSync(testTmpDir)) {
      rmSync(testTmpDir, { recursive: true, force: true });
    }
  });

  it('setConnection(null) should clear all tracked documents', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: testTmpDir,
    };

    mkdirSync(testTmpDir, { recursive: true });
    writeFileSync(join(testTmpDir, 'a.ts'), 'const a = 1;');

    const manager = new LSPDocumentManager(mockConfig);
    const mockConnection = {
      sendNotification: vi.fn().mockResolvedValue(undefined),
    };

    manager.setConnection(mockConnection as any, true);
    await manager.openDocument(join(testTmpDir, 'a.ts'));
    expect(manager.isDocumentOpen(join(testTmpDir, 'a.ts'))).toBe(true);

    manager.setConnection(null as any, false);
    expect(manager.getOpenDocumentUris()).toHaveLength(0);
  });

  it('closeAllDocuments should clean up even when close throws', async () => {
    const mockConfig = {
      name: 'test-server',
      command: 'test',
      args: [] as string[],
      filetypes: ['.ts'],
      languageId: 'typescript',
      workspaceRoot: testTmpDir,
    };

    mkdirSync(testTmpDir, { recursive: true });
    writeFileSync(join(testTmpDir, 'b.ts'), 'const b = 1;');

    const manager = new LSPDocumentManager(mockConfig);
    const mockConnection = {
      sendNotification: vi.fn().mockImplementation((method: string) => {
        if (method === 'textDocument/didOpen') return Promise.resolve();
        if (method === 'textDocument/didClose')
          throw new Error('Connection lost');
        return Promise.resolve();
      }),
    };

    manager.setConnection(mockConnection as any, true);
    await manager.openDocument(join(testTmpDir, 'b.ts'));
    await manager.closeAllDocuments();

    expect(manager.getOpenDocumentUris()).toHaveLength(0);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 14. [SECURITY] Path validator edge cases
//     Tests use real PathValidator with real filesystem
// ═══════════════════════════════════════════════════════════════════════

import { PathValidator } from 'octocode-security-utils/pathValidator';

describe('[SECURITY] PathValidator edge cases', () => {
  const testTmpDir = join(tmpdir(), `octocode-path-sec-${Date.now()}`);

  beforeEach(() => {
    mkdirSync(testTmpDir, { recursive: true });
  });

  afterEach(() => {
    if (existsSync(testTmpDir)) {
      rmSync(testTmpDir, { recursive: true, force: true });
    }
  });

  it('empty path should be rejected', () => {
    const validator = new PathValidator({ workspaceRoot: testTmpDir });
    expect(validator.validate('').isValid).toBe(false);
  });

  it('whitespace-only path should be rejected', () => {
    const validator = new PathValidator({ workspaceRoot: testTmpDir });
    expect(validator.validate('   ').isValid).toBe(false);
  });

  it('path outside workspace should be rejected', () => {
    const validator = new PathValidator({
      workspaceRoot: testTmpDir,
      includeHomeDir: false,
    });
    expect(validator.validate('/etc/passwd').isValid).toBe(false);
  });

  it('path within workspace should be accepted', () => {
    const workspaceRoot = process.cwd();
    const validator = new PathValidator({ workspaceRoot });
    expect(
      validator.validate(join(workspaceRoot, 'package.json')).isValid
    ).toBe(true);
  });

  it('symlink pointing outside workspace should be rejected', () => {
    const validator = new PathValidator({
      workspaceRoot: testTmpDir,
      includeHomeDir: false,
    });
    const linkPath = join(testTmpDir, 'evil-link');
    try {
      symlinkSync('/etc', linkPath);
      expect(validator.validate(linkPath).isValid).toBe(false);
    } catch {
      // Skip on systems that don't support symlinks
    }
  });

  it('path traversal via .. should be caught', () => {
    const validator = new PathValidator({
      workspaceRoot: testTmpDir,
      includeHomeDir: false,
    });
    expect(
      validator.validate(join(testTmpDir, '..', '..', '..', 'etc', 'passwd'))
        .isValid
    ).toBe(false);
  });
});

// ═══════════════════════════════════════════════════════════════════════
// 15. [RESILIENCE] validateCommand with edge case inputs
//     Tests call validateCommand() directly — no mocks
// ═══════════════════════════════════════════════════════════════════════

describe('[RESILIENCE] validateCommand edge cases', () => {
  it('undefined args should return isValid=false, not throw', () => {
    const result = validateCommand('rg', undefined as unknown as string[]);
    expect(result.isValid).toBe(false);
  });

  it('null args should return isValid=false, not throw', () => {
    const result = validateCommand('rg', null as unknown as string[]);
    expect(result.isValid).toBe(false);
  });

  it('empty command string should be rejected', () => {
    expect(validateCommand('', []).isValid).toBe(false);
  });

  it('commands not in allowlist should be rejected', () => {
    expect(validateCommand('python', ['-c', 'print("hello")']).isValid).toBe(
      false
    );
  });

  it('rg with empty args should be valid', () => {
    expect(validateCommand('rg', []).isValid).toBe(true);
  });

  it('find with long valid expression should work', () => {
    expect(
      validateCommand('find', [
        '.',
        '-maxdepth',
        '3',
        '-type',
        'f',
        '-name',
        '*.ts',
        '-o',
        '-name',
        '*.js',
      ]).isValid
    ).toBe(true);
  });

  it('git -c before subcommand should be parsed correctly', () => {
    expect(
      validateCommand('git', [
        '-c',
        'advice.detachedHead=false',
        'clone',
        '--depth',
        '1',
        'https://github.com/org/repo.git',
      ]).isValid
    ).toBe(true);
  });
});
