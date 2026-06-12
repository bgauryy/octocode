import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import path from 'path';
import fs from 'fs';

describe('ContentSanitizer — coverage gaps', () => {
  let ContentSanitizer: typeof import('../src/contentSanitizer.js').ContentSanitizer;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/contentSanitizer.js');
    ContentSanitizer = mod.ContentSanitizer;
  });

  describe('chunked detection (content > 500KB)', () => {
    it('should detect secrets in content exceeding CHUNK_SIZE', () => {
      const padding = 'x'.repeat(510_000);
      const secret = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
      const content = padding + ' ' + secret + ' ' + padding;

      const result = ContentSanitizer.sanitizeContent(content);

      expect(result.hasSecrets).toBe(true);
      expect(result.secretsDetected).toContain('githubTokens');
      expect(result.content).not.toContain(secret);
      expect(result.content).toContain('[REDACTED-GITHUBTOKENS]');
    });

    it('should detect secrets near chunk boundaries (overlap region)', () => {
      const testPattern = {
        name: 'test-boundary-secret',
        description: 'test',
        regex: /BOUNDARY_SECRET_[A-Z0-9]{20}/g,
        matchAccuracy: 'high' as const,
      };
      const beforeBoundary = 'a'.repeat(499_950);
      const secret = 'BOUNDARY_SECRET_ABCDEFGHIJ1234567890';
      const afterBoundary = 'b'.repeat(100_000);
      const content = beforeBoundary + secret + afterBoundary;

      const result = ContentSanitizer.sanitizeContent(content, undefined, [
        testPattern,
      ]);

      expect(result.hasSecrets).toBe(true);
      expect(result.content).not.toContain(secret);
      expect(result.content).toContain('[REDACTED-TEST-BOUNDARY-SECRET]');
    });

    it('should handle multiple secrets across chunks', () => {
      const testPattern = {
        name: 'test-multi-secret',
        description: 'test',
        regex: /MULTI_SECRET_[A-Z0-9]{20}/g,
        matchAccuracy: 'high' as const,
      };
      const secret1 = 'MULTI_SECRET_AAAAAAAAAA1111111111';
      const secret2 = 'MULTI_SECRET_BBBBBBBBBB2222222222';
      const padding = 'x'.repeat(510_000);
      const content = secret1 + padding + secret2;

      const result = ContentSanitizer.sanitizeContent(content, undefined, [
        testPattern,
      ]);

      expect(result.hasSecrets).toBe(true);
      expect(result.content).not.toContain(secret1);
      expect(result.content).not.toContain(secret2);
    });

    it('should handle content with fileContext-filtered patterns in chunked mode', () => {
      const padding = 'x'.repeat(510_000);
      const content = padding + 'DB_PASSWORD=hunter2' + padding;

      const result = ContentSanitizer.sanitizeContent(content, '.env');
      expect(result).toBeDefined();
      expect(typeof result.content).toBe('string');
    });

    it('should handle chunked detection when no secrets are present', () => {
      const content = 'Hello world! '.repeat(50_000);

      const result = ContentSanitizer.sanitizeContent(content);

      expect(result.hasSecrets).toBe(false);
      expect(result.secretsDetected).toHaveLength(0);
    });
  });

  describe('detectSecrets error handling', () => {
    it('should handle regex errors in non-chunked path gracefully', () => {
      const evilPattern = {
        name: 'evil',
        description: 'causes error',
        regex: {
          test: () => {
            throw new Error('regex exploded');
          },
          [Symbol.match]: () => {
            throw new Error('regex exploded');
          },
          exec: () => {
            throw new Error('regex exploded');
          },
        } as unknown as RegExp,
        matchAccuracy: 'high' as const,
      };

      const result = ContentSanitizer.sanitizeContent(
        'some content',
        undefined,
        [evilPattern]
      );

      expect(result.hasSecrets).toBe(true);
      expect(result.secretsDetected).toContain('detection-error');
      expect(result.content).toBe('[CONTENT-REDACTED-DETECTION-ERROR]');
    });

    it('should handle regex errors in chunked path gracefully', () => {
      const evilPattern = {
        name: 'evil-chunked',
        description: 'causes error in chunk',
        regex: {
          test: () => {
            throw new Error('chunk regex exploded');
          },
          [Symbol.match]: () => {
            throw new Error('chunk regex exploded');
          },
          exec: () => {
            throw new Error('chunk regex exploded');
          },
        } as unknown as RegExp,
        matchAccuracy: 'high' as const,
      };

      const bigContent = 'x'.repeat(600_000);
      const result = ContentSanitizer.sanitizeContent(bigContent, undefined, [
        evilPattern,
      ]);

      expect(result.hasSecrets).toBe(true);
      expect(result.secretsDetected).toContain('detection-error');
      expect(result.content).toBe('[CONTENT-REDACTED-DETECTION-ERROR]');
    });
  });

  describe('sanitizeContent with null/non-string input', () => {
    it('should handle null content', () => {
      const result = ContentSanitizer.sanitizeContent(
        null as unknown as string
      );
      expect(result.content).toBe('');
      expect(result.hasSecrets).toBe(false);
    });

    it('should handle undefined content', () => {
      const result = ContentSanitizer.sanitizeContent(
        undefined as unknown as string
      );
      expect(result.content).toBe('');
      expect(result.hasSecrets).toBe(false);
    });

    it('should handle number content', () => {
      const result = ContentSanitizer.sanitizeContent(42 as unknown as string);
      expect(result.content).toBe('42');
      expect(result.hasSecrets).toBe(false);
    });
  });

  describe('sanitizeContent with filePath and fileContext patterns', () => {
    it('should skip fileContext patterns when filePath does not match', () => {
      const pattern = {
        name: 'env-only',
        description: 'only matches in .env files',
        regex: /SECRET_VALUE=\w+/g,
        fileContext: /\.env$/,
        matchAccuracy: 'high' as const,
      };

      const result = ContentSanitizer.sanitizeContent(
        'SECRET_VALUE=hello',
        'config.json',
        [pattern]
      );

      expect(result.hasSecrets).toBe(false);
    });

    it('should apply fileContext patterns when filePath matches', () => {
      const pattern = {
        name: 'env-only',
        description: 'only matches in .env files',
        regex: /SECRET_VALUE=\w+/g,
        fileContext: /\.env$/,
        matchAccuracy: 'high' as const,
      };

      const result = ContentSanitizer.sanitizeContent(
        'SECRET_VALUE=hello',
        '.env',
        [pattern]
      );

      expect(result.hasSecrets).toBe(true);
    });

    it('should skip fileContext patterns in chunked mode when filePath is undefined', () => {
      const pattern = {
        name: 'env-only',
        description: 'only matches in .env files',
        regex: /SECRET_VALUE=\w+/g,
        fileContext: /\.env$/,
        matchAccuracy: 'high' as const,
      };

      const bigContent = 'SECRET_VALUE=hello ' + 'x'.repeat(510_000);
      const result = ContentSanitizer.sanitizeContent(bigContent, undefined, [
        pattern,
      ]);

      expect(result.hasSecrets).toBe(false);
    });
  });

  describe('validateInputParameters — circular reference detection', () => {
    it('should detect circular references in nested objects', () => {
      const obj: Record<string, unknown> = { key: 'value' };
      obj.self = obj;

      const result = ContentSanitizer.validateInputParameters(obj);

      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(
          w => w.includes('Circular reference') || w.includes('Invalid nested')
        )
      ).toBe(true);
    });
  });

  describe('validateInputParameters — nested array with objects containing validation errors', () => {
    it('should propagate validation errors from array items with nested objects', () => {
      const params = {
        items: [{ constructor: 'dangerous', valid: 'ok' }],
      };

      const result = ContentSanitizer.validateInputParameters(params);

      expect(result.isValid).toBe(false);
    });
  });
});

describe('commandValidator — coverage gaps', () => {
  let validateCommand: typeof import('../src/commandValidator.js').validateCommand;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/commandValidator.js');
    validateCommand = mod.validateCommand;
  });

  describe('git edge cases', () => {
    it('should reject git with only -c flags and no subcommand', () => {
      const result = validateCommand('git', [
        '-c',
        'advice.detachedHead=false',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('requires a subcommand');
    });

    it('should reject git with only -C flag and no subcommand', () => {
      const result = validateCommand('git', ['-C', '/tmp']);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('requires a subcommand');
    });

    it('should allow git -c with valid key before clone', () => {
      const result = validateCommand('git', [
        '-c',
        'advice.detachedHead=false',
        'clone',
        '--depth',
        '1',
        'https://github.com/org/repo.git',
      ]);
      expect(result.isValid).toBe(true);
    });

    it('should reject git -c with dangerous config key', () => {
      const result = validateCommand('git', [
        '-c',
        'core.sshCommand=evil',
        'clone',
        'https://github.com/org/repo.git',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('not allowed via -c');
    });

    it('should reject git sparse-checkout without an action', () => {
      const result = validateCommand('git', ['sparse-checkout']);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('requires an action');
    });

    it('should reject git sparse-checkout with unknown action', () => {
      const result = validateCommand('git', ['sparse-checkout', 'reapply']);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("action 'reapply' is not allowed");
    });

    it('should reject git sparse-checkout with disallowed flag', () => {
      const result = validateCommand('git', [
        'sparse-checkout',
        'set',
        '--stdin',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("flag '--stdin' is not allowed");
    });

    it('should allow git sparse-checkout set with paths and --cone', () => {
      const result = validateCommand('git', [
        'sparse-checkout',
        'set',
        '--cone',
        'src/',
        'lib/',
      ]);
      expect(result.isValid).toBe(true);
    });

    it('should reject git clone with file:// protocol', () => {
      const result = validateCommand('git', ['clone', 'file:///etc/passwd']);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("protocol 'file://' is not allowed");
    });

    it('should reject git clone with git:// protocol', () => {
      const result = validateCommand('git', [
        'clone',
        'git://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("protocol 'git://' is not allowed");
    });

    it('should reject git clone with http:// protocol', () => {
      const result = validateCommand('git', [
        'clone',
        'http://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("protocol 'http://' is not allowed");
    });

    it('should allow git clone with https://', () => {
      const result = validateCommand('git', [
        'clone',
        'https://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(true);
    });

    it('should reject git clone with disallowed flag', () => {
      const result = validateCommand('git', [
        'clone',
        '--recursive',
        'https://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("flag '--recursive' is not allowed");
    });

    it('should allow git clone with -c key=value inside clone args', () => {
      const result = validateCommand('git', [
        'clone',
        '-c',
        'http.extraHeader=Authorization: Bearer token',
        'https://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(true);
    });

    it('should reject git clone with dangerous -c key inside clone args', () => {
      const result = validateCommand('git', [
        'clone',
        '-c',
        'core.sshCommand=evil',
        'https://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('not allowed via -c');
    });

    it('should handle git clone with -- separator', () => {
      const result = validateCommand('git', [
        'clone',
        '--',
        'https://github.com/repo.git',
      ]);
      expect(result.isValid).toBe(true);
    });
  });

  describe('find edge cases', () => {
    it('should handle find with -- separator', () => {
      const result = validateCommand('find', ['.', '--', '-name', '*.ts']);
      expect(result.isValid).toBe(true);
    });

    it('should reject find with unknown flag', () => {
      const result = validateCommand('find', ['.', '-unknown']);
      expect(result.isValid).toBe(false);
      expect(result.error).toContain("'-unknown' is not allowed");
    });

    it('should allow find with parentheses and -o operator', () => {
      const result = validateCommand('find', [
        '.',
        '(',
        '-name',
        '*.ts',
        '-o',
        '-name',
        '*.js',
        ')',
      ]);
      expect(result.isValid).toBe(true);
    });
  });

  describe('grep pattern positions', () => {
    it('should handle grep with --include=pattern', () => {
      const result = validateCommand('grep', [
        '--include=*.ts',
        'pattern',
        './src',
      ]);
      expect(result.isValid).toBe(true);
    });

    it('should handle grep with -- separator', () => {
      const result = validateCommand('grep', ['--', 'pattern', './src']);
      expect(result.isValid).toBe(true);
    });
  });

  describe('rg edge cases', () => {
    it('should allow rg short flag bundles with all valid chars', () => {
      const result = validateCommand('rg', ['-Fn', 'pattern', './src']);
      expect(result.isValid).toBe(true);
    });

    it('should reject rg short flag bundles with invalid chars', () => {
      const result = validateCommand('rg', ['-Fz', 'pattern', './src']);
      expect(result.isValid).toBe(false);
    });

    it('should handle rg with -- followed by pattern', () => {
      const result = validateCommand('rg', ['--', 'pattern-with-dashes']);
      expect(result.isValid).toBe(true);
    });
  });

  describe('args validation', () => {
    it('should reject non-array args', () => {
      const result = validateCommand(
        'rg',
        'not-an-array' as unknown as string[]
      );
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Arguments must be an array');
    });
  });
});

describe('ignoredPathFilter — coverage gaps', () => {
  let shouldIgnoreFile: typeof import('../src/ignoredPathFilter.js').shouldIgnoreFile;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/ignoredPathFilter.js');
    shouldIgnoreFile = mod.shouldIgnoreFile;
  });

  it('should extract filename from full path and check against file patterns', () => {
    expect(shouldIgnoreFile('/some/path/.env')).toBe(true);
    expect(shouldIgnoreFile('/some/path/.env.local')).toBe(true);
  });

  it('should check the full normalized path against file patterns', () => {
    expect(shouldIgnoreFile('/app/.ssh/id_rsa')).toBe(true);
    expect(shouldIgnoreFile('path/to/.env')).toBe(true);
  });

  it('should handle backslash paths (Windows-style)', () => {
    expect(shouldIgnoreFile('C:\\Users\\app\\.env')).toBe(true);
  });
});

describe('pathUtils — coverage gaps', () => {
  let redactPath: typeof import('../src/pathUtils.js').redactPath;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/pathUtils.js');
    redactPath = mod.redactPath;
  });

  it('should return empty string for empty input', () => {
    expect(redactPath('')).toBe('');
  });

  it('should return empty string for undefined-like input', () => {
    expect(redactPath(undefined as unknown as string)).toBe('');
  });

  it('should return filename only for path outside workspace and home', () => {
    const fakeRoot = '/nonexistent/workspace/root';
    const outsidePath = '/opt/system/secret-config.yaml';

    const result = redactPath(outsidePath, fakeRoot);
    expect(result).toBe('secret-config.yaml');
  });

  it('should normalize paths with backslashes', () => {
    const root = process.cwd();
    const winPath = root.replace(/\//g, '\\') + '\\src\\index.ts';

    const result = redactPath(winPath, root);
    expect(result).toBe('src/index.ts');
  });

  it('should normalize trailing slashes', () => {
    const root = process.cwd();
    const pathWithSlash = root + '/src/';

    const result = redactPath(pathWithSlash, root);
    expect(result).toBe('src');
  });

  it('should show ~/... for paths inside home directory but outside workspace', () => {
    const home = require('os').homedir();
    const fakeRoot = '/nonexistent/workspace/root';
    const homePath = path.join(home, '.config', 'test.json');

    const result = redactPath(homePath, fakeRoot);
    expect(result).toMatch(/^~\//);
    expect(result).toContain('.config/test.json');
  });

  it('should return . for workspace root path', () => {
    const root = process.cwd();
    const result = redactPath(root, root);
    expect(result).toBe('.');
  });
});

describe('pathValidator — coverage gaps', () => {
  let PathValidator: typeof import('../src/pathValidator.js').PathValidator;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/pathValidator.js');
    PathValidator = mod.PathValidator;
  });

  it('should handle ENAMETOOLONG error', () => {
    const validator = new PathValidator({ workspaceRoot: process.cwd() });
    const originalRealpathSync = fs.realpathSync;

    vi.spyOn(fs, 'realpathSync').mockImplementation((p: fs.PathLike) => {
      const err = new Error('Name too long');
      (err as NodeJS.ErrnoException).code = 'ENAMETOOLONG';
      throw err;
    });

    const longPath = path.join(process.cwd(), 'a'.repeat(300));
    const result = validator.validate(longPath);

    expect(result.isValid).toBe(false);
    expect(result.error).toContain('Path name too long');

    vi.restoreAllMocks();
  });

  it('should handle unexpected/unknown errors during path validation', () => {
    const validator = new PathValidator({ workspaceRoot: process.cwd() });

    vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
      throw { code: 'EUNKNOWN', message: 'Something bizarre happened' };
    });

    const testPath = path.join(process.cwd(), 'test-file.ts');
    const result = validator.validate(testPath);

    expect(result.isValid).toBe(false);
    expect(result.error).toContain('Unexpected error');

    vi.restoreAllMocks();
  });
});

describe('registry — coverage gaps', () => {
  let securityRegistry: import('../src/registry.js').SecurityRegistry;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/registry.js');
    securityRegistry = mod.securityRegistry;
    securityRegistry.reset();
  });

  afterEach(() => {
    securityRegistry.reset();
  });

  it('should reject empty root strings', () => {
    expect(() => securityRegistry.addAllowedRoots([''])).toThrow(
      'Each root must be a non-empty string'
    );
  });

  it('should reject whitespace-only root strings', () => {
    expect(() => securityRegistry.addAllowedRoots(['   '])).toThrow(
      'Each root must be a non-empty string'
    );
  });

  it('should reject non-string roots', () => {
    expect(() =>
      securityRegistry.addAllowedRoots([42 as unknown as string])
    ).toThrow('Each root must be a non-empty string');
  });

  it('should not duplicate roots when added twice', () => {
    securityRegistry.addAllowedRoots(['/tmp/myroot']);
    securityRegistry.addAllowedRoots(['/tmp/myroot']);
    expect(securityRegistry.extraAllowedRoots.length).toBe(1);
  });

  it('should reject whitespace-only command strings', () => {
    expect(() => securityRegistry.addAllowedCommands(['   '])).toThrow(
      'Each command must be a non-empty string'
    );
  });

  it('should not duplicate ignored path patterns with same source', () => {
    const pattern = /^\.mydir$/;
    securityRegistry.addIgnoredPathPatterns([pattern]);
    securityRegistry.addIgnoredPathPatterns([pattern]);
    expect(securityRegistry.extraIgnoredPathPatterns.length).toBe(1);
  });

  it('should not duplicate ignored file patterns with same source', () => {
    const pattern = /^myfile\.txt$/;
    securityRegistry.addIgnoredFilePatterns([pattern]);
    securityRegistry.addIgnoredFilePatterns([pattern]);
    expect(securityRegistry.extraIgnoredFilePatterns.length).toBe(1);
  });

  it('should not duplicate secret patterns with same name', () => {
    const pattern = {
      name: 'dupTest',
      description: 'test',
      regex: /test/g,
      matchAccuracy: 'high' as const,
    };
    securityRegistry.addSecretPatterns([pattern]);
    securityRegistry.addSecretPatterns([pattern]);
    expect(securityRegistry.extraSecretPatterns.length).toBe(1);
  });

  it('should increment version on each mutation', () => {
    const v0 = securityRegistry.version;
    securityRegistry.addAllowedCommands(['curl']);
    const v1 = securityRegistry.version;
    expect(v1).toBeGreaterThan(v0);

    securityRegistry.addAllowedRoots(['/tmp']);
    const v2 = securityRegistry.version;
    expect(v2).toBeGreaterThan(v1);

    securityRegistry.reset();
    const v3 = securityRegistry.version;
    expect(v3).toBeGreaterThan(v2);
  });
});

describe('withSecurityValidation — coverage gaps', () => {
  let withSecurityValidation: typeof import('../src/withSecurityValidation.js').withSecurityValidation;
  let withBasicSecurityValidation: typeof import('../src/withSecurityValidation.js').withBasicSecurityValidation;
  let configureSecurity: typeof import('../src/withSecurityValidation.js').configureSecurity;

  beforeEach(async () => {
    vi.resetModules();

    vi.doMock('../src/contentSanitizer.js', () => ({
      ContentSanitizer: {
        validateInputParameters: vi.fn().mockReturnValue({
          isValid: true,
          sanitizedParams: {},
          warnings: [],
          hasSecrets: false,
        }),
        sanitizeContent: vi.fn((content: string) => ({
          content,
          hasSecrets: false,
          secretsDetected: [],
          warnings: [],
        })),
      },
    }));

    const mod = await import('../src/withSecurityValidation.js');
    withSecurityValidation = mod.withSecurityValidation;
    withBasicSecurityValidation = mod.withBasicSecurityValidation;
    configureSecurity = mod.configureSecurity;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should return cancellation error when signal is already aborted', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
    });

    const wrapped = withSecurityValidation('test-tool', handler);
    const ac = new AbortController();
    ac.abort();

    const result = await wrapped({}, { signal: ac.signal });

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('cancelled');
  });

  it('should return error when signal is aborted during execution', async () => {
    const ac = new AbortController();

    const handler = vi.fn().mockImplementation(
      () =>
        new Promise(resolve => {
          setTimeout(() => {
            ac.abort();
            setTimeout(
              () => resolve({ content: [{ type: 'text', text: 'late' }] }),
              50
            );
          }, 10);
        })
    );

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, { signal: ac.signal });

    expect(result.content[0]?.text).toBeDefined();
  });

  it('should timeout after configured duration', async () => {
    const handler = vi
      .fn()
      .mockImplementation(
        () => new Promise(resolve => setTimeout(resolve, 5000))
      );

    const wrapped = withSecurityValidation('test-tool', handler, {
      timeoutMs: 50,
    });
    const result = await wrapped({}, {});

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('timed out');
  });

  it('should use defaultTimeoutMs from configureSecurity', async () => {
    configureSecurity({ defaultTimeoutMs: 50 });

    const handler = vi
      .fn()
      .mockImplementation(
        () => new Promise(resolve => setTimeout(resolve, 5000))
      );

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, {});

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('timed out');
  });

  it('should log local tool calls via withBasicSecurityValidation when conditions met', async () => {
    const mockLogToolCall = vi.fn().mockResolvedValue(undefined);
    const mockIsLoggingEnabled = vi.fn().mockReturnValue(true);
    const mockIsLocalTool = vi.fn().mockReturnValue(true);

    configureSecurity({
      logToolCall: mockLogToolCall,
      isLoggingEnabled: mockIsLoggingEnabled,
      isLocalTool: mockIsLocalTool,
    });

    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockReturnValue({
      isValid: true,
      sanitizedParams: { path: '/some/file' },
      warnings: [],
      hasSecrets: false,
    });

    const handler = vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
    });

    const wrapped = withBasicSecurityValidation(handler, 'local_read_file');
    await wrapped({ path: '/some/file' });

    expect(mockIsLocalTool).toHaveBeenCalledWith('local_read_file');
    expect(mockLogToolCall).toHaveBeenCalled();
  });

  it('should handle result with null/undefined content array gracefully', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: null,
    });

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, {});

    expect(result).toBeDefined();
    expect(result.content).toBeNull();
  });

  it('should handle result with non-array content gracefully', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: 'not-an-array',
    });

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, {});

    expect(result).toBeDefined();
  });

  it('should log session error on withBasicSecurityValidation exception', async () => {
    const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
    configureSecurity({ logSessionError: mockLogSessionError });

    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(
      () => {
        throw new Error('boom');
      }
    );

    const handler = vi.fn();
    const wrapped = withBasicSecurityValidation(handler, 'my_local_tool');
    const result = await wrapped({});

    expect(result.isError).toBe(true);
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'my_local_tool',
      'TOOL_SECURITY_VALIDATION_FAILED'
    );
  });

  it('should log session error on withSecurityValidation exception', async () => {
    const mockLogSessionError = vi.fn().mockResolvedValue(undefined);
    configureSecurity({ logSessionError: mockLogSessionError });

    const { ContentSanitizer } = await import('../src/contentSanitizer.js');
    vi.mocked(ContentSanitizer.validateInputParameters).mockImplementation(
      () => {
        throw new Error('boom');
      }
    );

    const handler = vi.fn();
    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, { sessionId: 'session-1' });

    expect(result.isError).toBe(true);
    expect(mockLogSessionError).toHaveBeenCalledWith(
      'test-tool',
      'TOOL_SECURITY_VALIDATION_FAILED'
    );
  });

  it('should use "tool" as fallback name when toolName is undefined for basic validation', async () => {
    const handler = vi
      .fn()
      .mockImplementation(
        () => new Promise(resolve => setTimeout(resolve, 5000))
      );

    configureSecurity({ defaultTimeoutMs: 50 });

    const wrapped = withBasicSecurityValidation(handler);
    const result = await wrapped({});

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("Tool 'tool' timed out");
  });
});

describe('Security penetration — additional bypass vectors', () => {
  describe('ContentSanitizer — prototype pollution via nested objects', () => {
    it('should block constructor key in deeply nested objects', async () => {
      vi.resetModules();
      vi.doUnmock('../src/contentSanitizer.js');
      const { ContentSanitizer } = await import('../src/contentSanitizer.js');

      const params = {
        level1: {
          level2: {
            constructor: 'polluted',
            safe: 'value',
          },
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);
      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(
          w =>
            w.includes('Invalid nested object') ||
            w.includes('Dangerous parameter key')
        )
      ).toBe(true);
    });

    it('should block prototype key in deeply nested objects', async () => {
      vi.resetModules();
      vi.doUnmock('../src/contentSanitizer.js');
      const { ContentSanitizer } = await import('../src/contentSanitizer.js');

      const params = {
        level1: {
          level2: {
            prototype: 'polluted',
          },
        },
      };

      const result = ContentSanitizer.validateInputParameters(params);
      expect(result.isValid).toBe(false);
    });
  });

  describe('commandValidator — injection through pattern arguments', () => {
    it('should block command substitution in rg patterns', async () => {
      const { validateCommand } = await import('../src/commandValidator.js');

      const result = validateCommand('rg', ['$(whoami)', './src']);
      expect(result.isValid).toBe(false);
    });

    it('should block backtick substitution in patterns', async () => {
      const { validateCommand } = await import('../src/commandValidator.js');

      const result = validateCommand('rg', ['`whoami`', './src']);
      expect(result.isValid).toBe(false);
    });

    it('should block variable expansion in patterns', async () => {
      const { validateCommand } = await import('../src/commandValidator.js');

      const result = validateCommand('rg', ['${HOME}', './src']);
      expect(result.isValid).toBe(false);
    });

    it('should block semicolons in non-pattern args', async () => {
      const { validateCommand } = await import('../src/commandValidator.js');

      const result = validateCommand('rg', ['pattern', './src; rm -rf /']);
      expect(result.isValid).toBe(false);
    });
  });

  describe('PathValidator — symlink loop detection', () => {
    it('should handle ELOOP errors from symlink loops', async () => {
      const { PathValidator } = await import('../src/pathValidator.js');
      const validator = new PathValidator({ workspaceRoot: process.cwd() });

      vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        const err = new Error('Too many levels of symbolic links');
        (err as NodeJS.ErrnoException).code = 'ELOOP';
        throw err;
      });

      const testPath = path.join(process.cwd(), 'loopy-link');
      const result = validator.validate(testPath);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Symlink loop detected');

      vi.restoreAllMocks();
    });
  });

  describe('PathValidator — EACCES error handling', () => {
    it('should handle permission denied errors', async () => {
      const { PathValidator } = await import('../src/pathValidator.js');
      const validator = new PathValidator({ workspaceRoot: process.cwd() });

      vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
        const err = new Error('Permission denied');
        (err as NodeJS.ErrnoException).code = 'EACCES';
        throw err;
      });

      const testPath = path.join(process.cwd(), 'restricted-file');
      const result = validator.validate(testPath);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Permission denied');

      vi.restoreAllMocks();
    });
  });

  describe('mask — interaction with SecurityRegistry custom patterns', () => {
    it('should mask secrets from explicitly passed custom patterns', async () => {
      vi.resetModules();
      const { maskSensitiveData } = await import('../src/mask.js');

      const customPattern = {
        name: 'customCorpKey',
        description: 'Corp key',
        regex: /CORP_[A-Z0-9]{32}/g,
        matchAccuracy: 'high' as const,
      };

      const secret = 'CORP_ABCDEFGHIJKLMNOPQRSTUVWXYZ012345';
      const masked = maskSensitiveData(`key: ${secret}`, [customPattern]);

      expect(masked).not.toContain(secret);
      expect(masked).toContain('*');
    });
  });
});

describe('SecurityRegistry — freeze & ReDoS protection', () => {
  let securityRegistry: import('../src/registry.js').SecurityRegistry;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/registry.js');
    securityRegistry = mod.securityRegistry;
    securityRegistry.reset();
  });

  afterEach(() => {
    securityRegistry.reset();
  });

  describe('freeze()', () => {
    it('should prevent addSecretPatterns after freeze', () => {
      securityRegistry.freeze();
      expect(securityRegistry.frozen).toBe(true);
      expect(() =>
        securityRegistry.addSecretPatterns([
          {
            name: 'x',
            description: 'x',
            regex: /x/g,
            matchAccuracy: 'high',
          },
        ])
      ).toThrow('frozen');
    });

    it('should prevent addAllowedCommands after freeze', () => {
      securityRegistry.freeze();
      expect(() => securityRegistry.addAllowedCommands(['curl'])).toThrow(
        'frozen'
      );
    });

    it('should prevent addAllowedRoots after freeze', () => {
      securityRegistry.freeze();
      expect(() => securityRegistry.addAllowedRoots(['/tmp'])).toThrow(
        'frozen'
      );
    });

    it('should prevent addIgnoredPathPatterns after freeze', () => {
      securityRegistry.freeze();
      expect(() =>
        securityRegistry.addIgnoredPathPatterns([/^\.vault$/])
      ).toThrow('frozen');
    });

    it('should prevent addIgnoredFilePatterns after freeze', () => {
      securityRegistry.freeze();
      expect(() =>
        securityRegistry.addIgnoredFilePatterns([/^secrets\.yml$/])
      ).toThrow('frozen');
    });

    it('should allow mutations before freeze', () => {
      securityRegistry.addAllowedCommands(['jq']);
      securityRegistry.freeze();
      expect(securityRegistry.extraAllowedCommands).toContain('jq');
    });

    it('should unfreeze on reset()', () => {
      securityRegistry.freeze();
      expect(securityRegistry.frozen).toBe(true);
      securityRegistry.reset();
      expect(securityRegistry.frozen).toBe(false);
      expect(() => securityRegistry.addAllowedCommands(['jq'])).not.toThrow();
    });
  });

  describe('ReDoS protection', () => {
    it('should accept safe regex patterns', () => {
      expect(() =>
        securityRegistry.addSecretPatterns([
          {
            name: 'safe',
            description: 'safe pattern',
            regex: /SAFE_[A-Z]{10}/g,
            matchAccuracy: 'high',
          },
        ])
      ).not.toThrow();
    });

    it('should reject secret patterns where regex.test throws', () => {
      const throwingRegex = {
        test: () => {
          throw new Error('regex exploded');
        },
        source: 'bad',
        flags: 'g',
      } as unknown as RegExp;

      expect(() =>
        securityRegistry.addSecretPatterns([
          {
            name: 'throwing-pattern',
            description: 'throws',
            regex: throwingRegex,
            matchAccuracy: 'high',
          },
        ])
      ).toThrow('ReDoS safety check');
    });

    it('should reject ignored path patterns where regex.test throws', () => {
      const throwingRegex = {
        test: () => {
          throw new Error('regex exploded');
        },
        source: 'bad-path',
        flags: '',
      } as unknown as RegExp;

      expect(() =>
        securityRegistry.addIgnoredPathPatterns([throwingRegex])
      ).toThrow('ReDoS safety check');
    });

    it('should reject ignored file patterns where regex.test throws', () => {
      const throwingRegex = {
        test: () => {
          throw new Error('regex exploded');
        },
        source: 'bad-file',
        flags: '',
      } as unknown as RegExp;

      expect(() =>
        securityRegistry.addIgnoredFilePatterns([throwingRegex])
      ).toThrow('ReDoS safety check');
    });

    it('should accept safe ignored path patterns', () => {
      expect(() =>
        securityRegistry.addIgnoredPathPatterns([/^\.safe$/])
      ).not.toThrow();
    });

    it('should accept safe ignored file patterns', () => {
      expect(() =>
        securityRegistry.addIgnoredFilePatterns([/^safe\.txt$/])
      ).not.toThrow();
    });
  });

  describe('cached frozen getters', () => {
    it('should return same frozen reference on consecutive reads without mutation', () => {
      securityRegistry.addAllowedCommands(['jq']);
      const first = securityRegistry.extraAllowedCommands;
      const second = securityRegistry.extraAllowedCommands;
      expect(first).toBe(second);
    });

    it('should return different frozen reference after mutation', () => {
      securityRegistry.addAllowedCommands(['jq']);
      const first = securityRegistry.extraAllowedCommands;
      securityRegistry.addAllowedCommands(['yq']);
      const second = securityRegistry.extraAllowedCommands;
      expect(first).not.toBe(second);
      expect(second).toContain('yq');
    });

    it('should cache extraSecretPatterns between mutations', () => {
      securityRegistry.addSecretPatterns([
        {
          name: 'cache-test',
          description: 'test',
          regex: /CACHE_[A-Z]{5}/g,
          matchAccuracy: 'high',
        },
      ]);
      const first = securityRegistry.extraSecretPatterns;
      const second = securityRegistry.extraSecretPatterns;
      expect(first).toBe(second);
    });

    it('should cache extraAllowedRoots between mutations', () => {
      securityRegistry.addAllowedRoots(['/tmp/test']);
      const first = securityRegistry.extraAllowedRoots;
      const second = securityRegistry.extraAllowedRoots;
      expect(first).toBe(second);
    });

    it('should cache extraIgnoredPathPatterns between mutations', () => {
      securityRegistry.addIgnoredPathPatterns([/^\.test$/]);
      const first = securityRegistry.extraIgnoredPathPatterns;
      const second = securityRegistry.extraIgnoredPathPatterns;
      expect(first).toBe(second);
    });

    it('should cache extraIgnoredFilePatterns between mutations', () => {
      securityRegistry.addIgnoredFilePatterns([/^test\.txt$/]);
      const first = securityRegistry.extraIgnoredFilePatterns;
      const second = securityRegistry.extraIgnoredFilePatterns;
      expect(first).toBe(second);
    });
  });
});

describe('ContentSanitizer — input size guard', () => {
  let ContentSanitizer: typeof import('../src/contentSanitizer.js').ContentSanitizer;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/contentSanitizer.js');
    ContentSanitizer = mod.ContentSanitizer;
  });

  it('should reject content exceeding 10MB', () => {
    const hugeContent = 'x'.repeat(10_000_001);
    const result = ContentSanitizer.sanitizeContent(hugeContent);

    expect(result.hasSecrets).toBe(true);
    expect(result.secretsDetected).toContain('content-size-exceeded');
    expect(result.content).toBe('[CONTENT-REDACTED-SIZE-LIMIT]');
    expect(result.warnings[0]).toContain('limit');
  });

  it('should allow content just at the 10MB boundary', () => {
    const maxContent = 'x'.repeat(10_000_000);
    const result = ContentSanitizer.sanitizeContent(maxContent);

    expect(result.content).not.toBe('[CONTENT-REDACTED-SIZE-LIMIT]');
  });
});

describe('ContentSanitizer — pattern caching', () => {
  let ContentSanitizer: typeof import('../src/contentSanitizer.js').ContentSanitizer;
  let securityRegistry: import('../src/registry.js').SecurityRegistry;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/contentSanitizer.js');
    ContentSanitizer = mod.ContentSanitizer;
    const regMod = await import('../src/registry.js');
    securityRegistry = regMod.securityRegistry;
    securityRegistry.reset();
  });

  afterEach(() => {
    securityRegistry.reset();
  });

  it('should use cached patterns on consecutive calls without registry change', () => {
    const content = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const result1 = ContentSanitizer.sanitizeContent(content);
    const result2 = ContentSanitizer.sanitizeContent(content);
    expect(result1.hasSecrets).toBe(true);
    expect(result2.hasSecrets).toBe(true);
  });

  it('should invalidate cache when registry adds new patterns', () => {
    const token = 'NEWCACHE_ABCDEFGHIJ1234567890';
    const result1 = ContentSanitizer.sanitizeContent(token);
    expect(result1.hasSecrets).toBe(false);

    securityRegistry.addSecretPatterns([
      {
        name: 'newcache-test',
        description: 'test',
        regex: /NEWCACHE_[A-Z0-9]{20}/g,
        matchAccuracy: 'high',
      },
    ]);

    const result2 = ContentSanitizer.sanitizeContent(token);
    expect(result2.hasSecrets).toBe(true);
  });
});

describe('pathValidator — getType edge cases', () => {
  let PathValidator: typeof import('../src/pathValidator.js').PathValidator;

  beforeEach(async () => {
    vi.resetModules();
    const mod = await import('../src/pathValidator.js');
    PathValidator = mod.PathValidator;
  });

  it('should return "symlink" for symbolic links', async () => {
    const validator = new PathValidator({ workspaceRoot: process.cwd() });
    const testPath = path.join(process.cwd(), 'test-symlink-gettype');

    vi.spyOn(fs.promises, 'lstat').mockResolvedValueOnce({
      isFile: () => false,
      isDirectory: () => false,
      isSymbolicLink: () => true,
    } as unknown as fs.Stats);

    const type = await validator.getType(testPath);
    expect(type).toBe('symlink');

    vi.restoreAllMocks();
  });

  it('should return null for non-standard entries (socket, pipe, etc.)', async () => {
    const validator = new PathValidator({ workspaceRoot: process.cwd() });
    const testPath = path.join(process.cwd(), 'test-socket-gettype');

    vi.spyOn(fs.promises, 'lstat').mockResolvedValueOnce({
      isFile: () => false,
      isDirectory: () => false,
      isSymbolicLink: () => false,
    } as unknown as fs.Stats);

    const type = await validator.getType(testPath);
    expect(type).toBe(null);

    vi.restoreAllMocks();
  });
});

describe('withSecurityValidation — handler error in withToolTimeout', () => {
  let withSecurityValidation: typeof import('../src/withSecurityValidation.js').withSecurityValidation;

  beforeEach(async () => {
    vi.resetModules();
    vi.doMock('../src/contentSanitizer.js', () => ({
      ContentSanitizer: {
        validateInputParameters: vi.fn().mockReturnValue({
          isValid: true,
          sanitizedParams: {},
          warnings: [],
          hasSecrets: false,
        }),
        sanitizeContent: vi.fn((content: string) => ({
          content,
          hasSecrets: false,
          secretsDetected: [],
          warnings: [],
        })),
      },
    }));
    const mod = await import('../src/withSecurityValidation.js');
    withSecurityValidation = mod.withSecurityValidation;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should catch handler errors and return error result', async () => {
    const handler = vi.fn().mockRejectedValue(new Error('handler went boom'));

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, {});

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('handler went boom');
  });

  it('should catch non-Error handler rejections', async () => {
    const handler = vi.fn().mockRejectedValue('string rejection');

    const wrapped = withSecurityValidation('test-tool', handler);
    const result = await wrapped({}, {});

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain('Unknown error');
  });
});
