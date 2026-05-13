/**
 * README validation tests — ensures every code example in README.md
 * is accurate and produces the documented behavior.
 */

import { describe, it, expect, afterEach } from 'vitest';
import path from 'path';
import os from 'os';

import {
  PathValidator,
  pathValidator,
  reinitializePathValidator,
  ContentSanitizer,
  maskSensitiveData,
  validateCommand,
  validateExecutionContext,
  resolveWorkspaceRoot,
  shouldIgnore,
  shouldIgnorePath,
  shouldIgnoreFile,
  allRegexPatterns,
  redactPath,
  ALLOWED_COMMANDS,
  DANGEROUS_PATTERNS,
  PATTERN_DANGEROUS_PATTERNS,
  IGNORED_PATH_PATTERNS,
  IGNORED_FILE_PATTERNS,
  withSecurityValidation,
  withBasicSecurityValidation,
  configureSecurity,
  extractResearchFields,
  extractRepoOwnerFromParams,
  securityRegistry,
} from '../src/index.js';

import type {
  SanitizationResult,
  ValidationResult,
  PathValidationResult,
  ToolResult,
  SensitiveDataPattern,
} from '../src/index.js';

import type { SecurityDepsConfig } from '../src/withSecurityValidation.js';

const workspace = process.cwd();

describe('README: Quick Start', () => {
  it('PathValidator blocks path traversal', () => {
    const validator = new PathValidator({
      workspaceRoot: workspace,
      includeHomeDir: false,
    });
    const result = validator.validate('../../etc/passwd');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('outside allowed directories');
  });

  it('ContentSanitizer.sanitizeContent detects GitHub PAT', () => {
    const result = ContentSanitizer.sanitizeContent(
      'key: ghp_1234567890abcdefghijklmnopqrstuvwxyz123456'
    );
    expect(result.hasSecrets).toBe(true);
    expect(result.content).toContain('[REDACTED-');
    expect(result.secretsDetected.length).toBeGreaterThan(0);
  });

  it('maskSensitiveData partially masks secrets', () => {
    const masked = maskSensitiveData(
      'token: sk-1234567890abcdefT3BlbkFJ1234567890abcdef'
    );
    expect(masked).not.toContain('sk-1234567890abcdefT3BlbkFJ1234567890abcdef');
    expect(masked).toContain('*');
  });

  it('validateCommand allows valid rg invocation', () => {
    const result = validateCommand('rg', ['pattern', './src']);
    expect(result.isValid).toBe(true);
  });

  it('validateExecutionContext validates workspace-internal path', () => {
    const result = validateExecutionContext(
      path.join(workspace, 'packages', 'octocode-security')
    );
    expect(result.isValid).toBe(true);
    expect(result.sanitizedPath).toBeDefined();
  });
});

describe('README: PathValidator', () => {
  describe('constructor', () => {
    it('accepts workspaceRoot, additionalRoots, includeHomeDir', () => {
      const v = new PathValidator({
        workspaceRoot: workspace,
        additionalRoots: ['/tmp'],
        includeHomeDir: false,
      });
      expect(v.getAllowedRoots()).toContain(path.resolve(workspace));
      expect(v.getAllowedRoots()).toContain('/tmp');
    });

    it('includes roots registered via securityRegistry', () => {
      const customRoot = '/tmp/custom-app-home';
      securityRegistry.addAllowedRoots([customRoot]);
      try {
        const v = new PathValidator({ workspaceRoot: workspace });
        expect(v.getAllowedRoots()).toContain(path.resolve(customRoot));
      } finally {
        securityRegistry.reset();
      }
    });
  });

  describe('validate()', () => {
    const v = new PathValidator({
      workspaceRoot: workspace,
      includeHomeDir: false,
    });

    it('returns isValid: true + sanitizedPath for workspace-internal path', () => {
      const result = v.validate(path.join(workspace, 'package.json'));
      expect(result.isValid).toBe(true);
      expect(result.sanitizedPath).toBeDefined();
    });

    it('returns isValid: false for path outside workspace', () => {
      const result = v.validate('/etc/shadow');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('outside allowed directories');
    });

    it('returns isValid: false for empty path', () => {
      const result = v.validate('');
      expect(result.isValid).toBe(false);
      expect(result.error).toContain('empty');
    });
  });

  describe('exists()', () => {
    const v = new PathValidator({ workspaceRoot: workspace });

    it('returns true for existing file', async () => {
      expect(await v.exists(path.join(workspace, 'package.json'))).toBe(true);
    });

    it('returns false for path outside root', async () => {
      const strict = new PathValidator({
        workspaceRoot: workspace,
        includeHomeDir: false,
      });
      expect(await strict.exists('/etc/shadow')).toBe(false);
    });

    it('returns false for non-existent file', async () => {
      expect(await v.exists(path.join(workspace, 'missing-file-xyz.ts'))).toBe(
        false
      );
    });
  });

  describe('getType()', () => {
    const v = new PathValidator({ workspaceRoot: workspace });

    it('returns "directory" for directory', async () => {
      expect(await v.getType(path.join(workspace, 'src'))).toBe('directory');
    });

    it('returns "file" for file', async () => {
      expect(await v.getType(path.join(workspace, 'package.json'))).toBe(
        'file'
      );
    });

    it('returns null for path outside root', async () => {
      const strict = new PathValidator({
        workspaceRoot: workspace,
        includeHomeDir: false,
      });
      expect(await strict.getType('/etc/passwd')).toBe(null);
    });
  });

  describe('addAllowedRoot()', () => {
    it('adds root and validates paths under it', () => {
      const v = new PathValidator({
        workspaceRoot: workspace,
        includeHomeDir: false,
      });
      v.addAllowedRoot('/tmp');
      expect(v.getAllowedRoots()).toContain('/tmp');
    });
  });

  describe('getAllowedRoots()', () => {
    it('returns readonly array of current allowed roots', () => {
      const v = new PathValidator({ workspaceRoot: workspace });
      const roots = v.getAllowedRoots();
      expect(Array.isArray(roots)).toBe(true);
      expect(roots.length).toBeGreaterThan(0);
      expect(roots).toContain(path.resolve(workspace));
    });
  });

  describe('global instance', () => {
    afterEach(() => {
      reinitializePathValidator();
    });

    it('pathValidator singleton validates workspace paths', () => {
      const result = pathValidator.validate(
        path.join(workspace, 'package.json')
      );
      expect(result.isValid).toBe(true);
    });

    it('reinitializePathValidator reconfigures singleton', () => {
      reinitializePathValidator({
        workspaceRoot: '/tmp',
        includeHomeDir: false,
      });
      expect(pathValidator.getAllowedRoots()).toContain('/tmp');
    });
  });
});

describe('README: ContentSanitizer', () => {
  describe('sanitizeContent()', () => {
    it('replaces secrets with [REDACTED-*] tokens', () => {
      const result = ContentSanitizer.sanitizeContent(
        'Authorization: Bearer ghp_1234567890abcdefghijklmnopqrstuvwxyz123456'
      );
      expect(result.hasSecrets).toBe(true);
      expect(result.content).toContain('[REDACTED-');
      expect(result.content).not.toContain('ghp_');
      expect(result.secretsDetected.length).toBeGreaterThan(0);
      expect(result.warnings.length).toBeGreaterThan(0);
    });

    it('returns hasSecrets: false for clean text', () => {
      const result = ContentSanitizer.sanitizeContent('just normal text');
      expect(result).toEqual({
        content: 'just normal text',
        hasSecrets: false,
        secretsDetected: [],
        warnings: [],
      });
    });

    it('accepts optional filePath for context-sensitive patterns', () => {
      const result = ContentSanitizer.sanitizeContent(
        'some content without actual secrets',
        '.env'
      );
      expect(result).toBeDefined();
      expect(typeof result.content).toBe('string');
    });
  });

  describe('validateInputParameters()', () => {
    it('sanitizes secrets in nested params', () => {
      const result = ContentSanitizer.validateInputParameters({
        query: 'search term',
        token: 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456',
      });
      expect(result.isValid).toBe(true);
      expect(result.hasSecrets).toBe(true);
      expect(result.sanitizedParams.token as string).toContain('[REDACTED-');
      expect(result.warnings.length).toBeGreaterThan(0);
    });

    it('blocks __proto__ keys (prototype pollution)', () => {
      const params: Record<string, unknown> = { normal: 'safe' };
      Object.defineProperty(params, '__proto__', {
        value: 'evil',
        enumerable: true,
        configurable: true,
        writable: true,
      });
      const result = ContentSanitizer.validateInputParameters(params);
      expect(result.isValid).toBe(false);
      expect(
        result.warnings.some(w => w.includes('Dangerous parameter key'))
      ).toBe(true);
    });

    it('blocks constructor keys', () => {
      const result = ContentSanitizer.validateInputParameters({
        constructor: 'evil',
        normal: 'safe',
      });
      expect(result.isValid).toBe(false);
    });

    it('blocks prototype keys', () => {
      const result = ContentSanitizer.validateInputParameters({
        prototype: 'evil',
        normal: 'safe',
      });
      expect(result.isValid).toBe(false);
    });

    it('truncates strings > 10,000 chars', () => {
      const result = ContentSanitizer.validateInputParameters({
        big: 'x'.repeat(15_000),
      });
      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.big as string).length).toBe(10_000);
      expect(
        result.warnings.some(w => w.includes('exceeds maximum length'))
      ).toBe(true);
    });

    it('truncates arrays > 100 items', () => {
      const result = ContentSanitizer.validateInputParameters({
        items: Array.from({ length: 150 }, (_, i) => `item${i}`),
      });
      expect(result.isValid).toBe(true);
      expect((result.sanitizedParams.items as string[]).length).toBe(100);
    });

    it('detects circular references', () => {
      const obj: Record<string, unknown> = { a: 1 };
      obj.self = obj;
      const outer = { nested: obj };
      const result = ContentSanitizer.validateInputParameters(outer);
      expect(result.isValid).toBe(false);
      expect(result.warnings.some(w => w.includes('Circular reference'))).toBe(
        true
      );
    });
  });
});

describe('README: maskSensitiveData', () => {
  it('partially masks secrets (alternating chars)', () => {
    const masked = maskSensitiveData(
      'export GITHUB_TOKEN=ghp_1234567890123456789012345678901234567890'
    );
    expect(masked).toContain('*');
    expect(masked).not.toContain(
      'ghp_1234567890123456789012345678901234567890'
    );
    expect(masked.startsWith('export GITHUB_TOKEN=')).toBe(true);
  });

  it('returns unchanged string when no secrets', () => {
    const text = 'no secrets here';
    expect(maskSensitiveData(text)).toBe(text);
  });

  it('signature accepts optional patterns array', () => {
    const result = maskSensitiveData('some text', []);
    expect(result).toBe('some text');
  });
});

describe('README: validateCommand', () => {
  it('allows rg with valid flags', () => {
    expect(validateCommand('rg', ['--json', 'pattern', './src'])).toEqual({
      isValid: true,
    });
  });

  it('rejects non-whitelisted command (rm)', () => {
    const result = validateCommand('rm', ['-rf', '/']);
    expect(result.isValid).toBe(false);
    expect(result.error).toContain("Command 'rm' is not allowed");
    expect(result.error).toContain('rg');
    expect(result.error).toContain('ls');
    expect(result.error).toContain('find');
    expect(result.error).toContain('grep');
    expect(result.error).toContain('git');
  });

  it('rejects rg --pre (dangerous flag)', () => {
    const result = validateCommand('rg', ['--pre', 'evil-script', 'pattern']);
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('not allowed');
  });

  it('rejects find -exec', () => {
    const result = validateCommand('find', ['.', '-exec', 'rm', '{}', ';']);
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('not allowed');
  });

  it('rejects git push (non-allowed subcommand)', () => {
    const result = validateCommand('git', ['push', '--force']);
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('not allowed');
    expect(result.error).toContain('clone');
    expect(result.error).toContain('sparse-checkout');
  });

  it('function signature returns { isValid, error? }', () => {
    const valid = validateCommand('rg', ['pattern']);
    expect(valid).toHaveProperty('isValid');
    expect(valid.error).toBeUndefined();

    const invalid = validateCommand('curl', ['http://evil.com']);
    expect(invalid).toHaveProperty('isValid');
    expect(invalid).toHaveProperty('error');
  });

  it('allowed commands list matches README', () => {
    expect(ALLOWED_COMMANDS).toContain('rg');
    expect(ALLOWED_COMMANDS).toContain('ls');
    expect(ALLOWED_COMMANDS).toContain('find');
    expect(ALLOWED_COMMANDS).toContain('grep');
    expect(ALLOWED_COMMANDS).toContain('git');
    expect(ALLOWED_COMMANDS.length).toBe(5);
  });
});

describe('README: withSecurityValidation', () => {
  it('wraps handler with input sanitization', async () => {
    const handler = withSecurityValidation<{ query: string }>(
      'test_tool',
      async args => ({
        content: [{ type: 'text', text: `Result: ${args.query}` }],
      })
    );

    const result = await handler(
      { query: 'search term' },
      { sessionId: 'test-session' }
    );
    expect(result.content[0]?.text).toBe('Result: search term');
  });

  it('sanitizes secrets in input', async () => {
    const handler = withSecurityValidation<{ token: string }>(
      'test_tool',
      async args => ({
        content: [{ type: 'text', text: args.token }],
      })
    );

    const result = await handler(
      { token: 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456' },
      {}
    );
    expect(result.content[0]?.text).toContain('[REDACTED-');
  });

  it('returns error result on invalid input', async () => {
    const handler = withSecurityValidation<{ x: string }>(
      'test_tool',
      async args => ({
        content: [{ type: 'text', text: args.x }],
      })
    );

    const result = await handler(null as unknown, {});
    expect(result.isError).toBe(true);
  });
});

describe('README: withBasicSecurityValidation', () => {
  it('wraps handler without auth requirements', async () => {
    const handler = withBasicSecurityValidation<{ path: string }>(
      async args => ({
        content: [{ type: 'text', text: `Read: ${args.path}` }],
      }),
      'local_read_file'
    );

    const result = await handler({ path: './src/index.ts' });
    expect(result.content[0]?.text).toBe('Read: ./src/index.ts');
  });
});

describe('README: configureSecurity', () => {
  it('accepts SecurityDepsConfig object', () => {
    expect(() => {
      configureSecurity({
        logToolCall: async () => {},
        logSessionError: async () => {},
        isLoggingEnabled: () => false,
        isLocalTool: name => name.startsWith('local'),
      });
    }).not.toThrow();
  });
});

describe('README: helper exports', () => {
  it('extractResearchFields extracts from bulk query params', () => {
    const result = extractResearchFields({
      queries: [{ researchGoal: 'find auth flow', reasoning: 'tracing login' }],
    });
    expect(result.researchGoal).toBe('find auth flow');
    expect(result.reasoning).toBe('tracing login');
  });

  it('extractRepoOwnerFromParams combines owner/repo', () => {
    const repos = extractRepoOwnerFromParams({
      owner: 'facebook',
      repo: 'react',
    });
    expect(repos).toEqual(['facebook/react']);
  });

  it('extractRepoOwnerFromParams parses repository field', () => {
    const repos = extractRepoOwnerFromParams({
      repository: 'facebook/react',
    });
    expect(repos).toEqual(['facebook/react']);
  });
});

describe('README: validateExecutionContext', () => {
  it('returns isValid: true for workspace-internal path', () => {
    const result = validateExecutionContext(
      path.join(workspace, 'packages', 'octocode-security')
    );
    expect(result.isValid).toBe(true);
    expect(result.sanitizedPath).toBeDefined();
  });

  it('returns isValid: false for outside path', () => {
    const result = validateExecutionContext('/etc');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain(
      'Can only execute commands within the configured workspace directory'
    );
  });

  it('returns isValid: true for undefined cwd', () => {
    const result = validateExecutionContext(undefined);
    expect(result.isValid).toBe(true);
  });

  it('returns isValid: false for empty string', () => {
    const result = validateExecutionContext('');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('cannot be empty');
  });

  it('accepts optional workspaceRoot override', () => {
    const result = validateExecutionContext('/tmp/sub', '/tmp');
    expect(result.isValid).toBe(true);
  });
});

describe('README: resolveWorkspaceRoot', () => {
  const savedEnv = process.env.WORKSPACE_ROOT;

  afterEach(() => {
    if (savedEnv === undefined) {
      delete process.env.WORKSPACE_ROOT;
    } else {
      process.env.WORKSPACE_ROOT = savedEnv;
    }
  });

  it('returns explicit path when provided', () => {
    expect(resolveWorkspaceRoot('/explicit/path')).toBe(
      path.resolve('/explicit/path')
    );
  });

  it('falls back to WORKSPACE_ROOT env var', () => {
    process.env.WORKSPACE_ROOT = workspace;
    expect(resolveWorkspaceRoot()).toBe(path.resolve(workspace));
  });

  it('falls back to process.cwd()', () => {
    delete process.env.WORKSPACE_ROOT;
    expect(resolveWorkspaceRoot()).toBe(process.cwd());
  });
});

describe('README: Ignored Path Filters', () => {
  describe('shouldIgnore()', () => {
    it('returns true for .git paths', () => {
      expect(shouldIgnore('/app/.git/config')).toBe(true);
    });

    it('returns true for .env files', () => {
      expect(shouldIgnore('/app/.env')).toBe(true);
    });

    it('returns false for normal source files', () => {
      expect(shouldIgnore('/app/src/index.ts')).toBe(false);
    });
  });

  describe('shouldIgnorePath()', () => {
    it('returns true for .aws', () => {
      expect(shouldIgnorePath('.aws/credentials')).toBe(true);
    });

    it('returns true for .ssh', () => {
      expect(shouldIgnorePath('.ssh/id_rsa')).toBe(true);
    });
  });

  describe('shouldIgnoreFile()', () => {
    it('returns true for .env.local', () => {
      expect(shouldIgnoreFile('.env.local')).toBe(true);
    });

    it('returns false for package.json', () => {
      expect(shouldIgnoreFile('package.json')).toBe(false);
    });
  });

  describe('raw pattern lists', () => {
    it('IGNORED_PATH_PATTERNS is a non-empty array of RegExp', () => {
      expect(Array.isArray(IGNORED_PATH_PATTERNS)).toBe(true);
      expect(IGNORED_PATH_PATTERNS.length).toBeGreaterThan(0);
      expect(IGNORED_PATH_PATTERNS[0]).toBeInstanceOf(RegExp);
    });

    it('IGNORED_FILE_PATTERNS is a non-empty array of RegExp', () => {
      expect(Array.isArray(IGNORED_FILE_PATTERNS)).toBe(true);
      expect(IGNORED_FILE_PATTERNS.length).toBeGreaterThan(0);
      expect(IGNORED_FILE_PATTERNS[0]).toBeInstanceOf(RegExp);
    });
  });
});

describe('README: Regex Patterns', () => {
  it('allRegexPatterns is a non-empty array', () => {
    expect(Array.isArray(allRegexPatterns)).toBe(true);
    expect(allRegexPatterns.length).toBeGreaterThan(100);
  });

  it('each pattern has name, description, regex', () => {
    for (const pattern of allRegexPatterns) {
      expect(typeof pattern.name).toBe('string');
      expect(pattern.name.length).toBeGreaterThan(0);
      expect(typeof pattern.description).toBe('string');
      expect(pattern.regex).toBeInstanceOf(RegExp);
    }
  });

  it('patterns detect GitHub PAT when iterated', () => {
    const content = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const matched = allRegexPatterns.filter(p => p.regex.test(content));
    expect(matched.length).toBeGreaterThan(0);
  });

  it('SensitiveDataPattern shape includes optional fields', () => {
    const pattern = allRegexPatterns.find(p => p.matchAccuracy);
    if (pattern) {
      expect(['high', 'medium']).toContain(pattern.matchAccuracy);
    }
  });
});

describe('README: Path Utilities — redactPath', () => {
  it('returns project-relative path within workspace', () => {
    const result = redactPath(
      path.join(workspace, 'src', 'index.ts'),
      workspace
    );
    expect(result).toBe('src/index.ts');
  });

  it('returns ~/... for home-relative paths', () => {
    const homeDir = os.homedir();
    const result = redactPath(path.join(homeDir, '.config', 'secrets.json'));
    expect(result).toBe('~/.config/secrets.json');
  });

  it('returns filename only for paths outside all roots', () => {
    const result = redactPath('/opt/system/config.yaml', '/nonexistent');
    expect(result).toBe('config.yaml');
  });

  it('returns empty string for empty input', () => {
    expect(redactPath('')).toBe('');
  });
});

describe('README: Security Constants', () => {
  it('ALLOWED_COMMANDS matches documented list', () => {
    expect([...ALLOWED_COMMANDS]).toEqual(['rg', 'ls', 'find', 'grep', 'git']);
  });

  it('DANGEROUS_PATTERNS is a non-empty readonly array of RegExp', () => {
    expect(DANGEROUS_PATTERNS.length).toBe(3);
    for (const p of DANGEROUS_PATTERNS) {
      expect(p).toBeInstanceOf(RegExp);
    }
    expect(DANGEROUS_PATTERNS[0]!.test(';')).toBe(true);
    expect(DANGEROUS_PATTERNS[1]!.test('${')).toBe(true);
    expect(DANGEROUS_PATTERNS[2]!.test('$(')).toBe(true);
  });

  it('PATTERN_DANGEROUS_PATTERNS is more permissive for search patterns', () => {
    expect(PATTERN_DANGEROUS_PATTERNS.length).toBe(4);
    for (const p of PATTERN_DANGEROUS_PATTERNS) {
      expect(p).toBeInstanceOf(RegExp);
    }
  });
});

describe('README: Types', () => {
  it('SanitizationResult has documented shape', () => {
    const r: SanitizationResult = {
      content: 'test',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    };
    expect(r.content).toBe('test');
    expect(r.hasSecrets).toBe(false);
  });

  it('ValidationResult has documented shape', () => {
    const r: ValidationResult = {
      sanitizedParams: {},
      isValid: true,
      hasSecrets: false,
      warnings: [],
    };
    expect(r.isValid).toBe(true);
  });

  it('PathValidationResult has documented shape', () => {
    const r: PathValidationResult = {
      isValid: true,
      sanitizedPath: '/foo',
    };
    expect(r.sanitizedPath).toBe('/foo');

    const err: PathValidationResult = {
      isValid: false,
      error: 'bad path',
    };
    expect(err.error).toBe('bad path');
  });

  it('ToolResult has documented shape', () => {
    const r: ToolResult = {
      content: [{ type: 'text', text: 'hello' }],
    };
    expect(r.content[0]?.type).toBe('text');

    const errResult: ToolResult = {
      content: [{ type: 'text', text: 'fail' }],
      isError: true,
    };
    expect(errResult.isError).toBe(true);
  });

  it('SecurityDepsConfig has documented shape', () => {
    const deps: SecurityDepsConfig = {
      logToolCall: async () => {},
      logSessionError: async () => {},
      isLoggingEnabled: () => true,
      isLocalTool: name => name.startsWith('local'),
    };
    expect(deps.isLoggingEnabled!()).toBe(true);
    expect(deps.isLocalTool!('localSearch')).toBe(true);
    expect(deps.isLocalTool!('githubSearch')).toBe(false);
  });

  it('SensitiveDataPattern has documented shape', () => {
    const p: SensitiveDataPattern = {
      name: 'test',
      description: 'test pattern',
      regex: /test/,
    };
    expect(p.regex.test('test')).toBe(true);

    const pFull: SensitiveDataPattern = {
      name: 'test',
      description: 'test pattern',
      regex: /test/,
      fileContext: /\.env/,
      matchAccuracy: 'high',
    };
    expect(pFull.matchAccuracy).toBe('high');
  });
});

describe('README: Sub-path Exports', () => {
  it('all documented sub-path exports resolve', async () => {
    const pathValidatorMod = await import('../src/pathValidator.js');
    expect(pathValidatorMod.PathValidator).toBeDefined();

    const contentSanitizerMod = await import('../src/contentSanitizer.js');
    expect(contentSanitizerMod.ContentSanitizer).toBeDefined();

    const wsvMod = await import('../src/withSecurityValidation.js');
    expect(wsvMod.withSecurityValidation).toBeDefined();

    const maskMod = await import('../src/mask.js');
    expect(maskMod.maskSensitiveData).toBeDefined();

    const cmdMod = await import('../src/commandValidator.js');
    expect(cmdMod.validateCommand).toBeDefined();

    const execMod = await import('../src/executionContextValidator.js');
    expect(execMod.validateExecutionContext).toBeDefined();

    const wrMod = await import('../src/workspaceRoot.js');
    expect(wrMod.resolveWorkspaceRoot).toBeDefined();

    const ignoreMod = await import('../src/ignoredPathFilter.js');
    expect(ignoreMod.shouldIgnore).toBeDefined();

    const pathUtilsMod = await import('../src/pathUtils.js');
    expect(pathUtilsMod.redactPath).toBeDefined();

    const regexesMod = await import('../src/regexes/index.js');
    expect(regexesMod.allRegexPatterns).toBeDefined();

    const typesMod = await import('../src/types.js');
    expect(typesMod).toBeDefined();
  });
});
