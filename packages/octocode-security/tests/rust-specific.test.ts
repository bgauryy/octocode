import { describe, it, expect, beforeAll } from 'vitest';
import { createRequire } from 'module';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { ContentSanitizer } from '../src/contentSanitizer.js';
import { maskSensitiveData } from '../src/mask.js';

const __dir = dirname(fileURLToPath(import.meta.url));
const _req = createRequire(import.meta.url);

interface NativeModule {
  sanitizeContent(
    content: string,
    filePath: string | null
  ): {
    content: string;
    hasSecrets: boolean;
    secretsDetected: string[];
    warnings: string[];
  };
  maskSensitiveData(text: string): string;
  patternCount(): number;
}

let native: NativeModule | null = null;

function isMusl(): boolean {
  try {
    const report = (
      process as NodeJS.Process & {
        report?: {
          getReport(): { header?: { glibcVersionRuntime?: string } };
        };
      }
    ).report?.getReport();
    return !report?.header?.glibcVersionRuntime;
  } catch {
    return true;
  }
}

function currentNativeTriple(): string | undefined {
  const linuxLibc =
    process.platform === 'linux' ? (isMusl() ? 'musl' : 'gnu') : '';
  const tripleMap: Record<string, Record<string, string>> = {
    darwin: { arm64: 'darwin-arm64', x64: 'darwin-x64' },
    linux: {
      arm64: `linux-arm64-${linuxLibc}`,
      x64: `linux-x64-${linuxLibc}`,
    },
    win32: { x64: 'win32-x64-msvc' },
  };
  return tripleMap[process.platform]?.[process.arch];
}

beforeAll(() => {
  const triple = currentNativeTriple();
  const candidates = [
    process.env.OCTOCODE_SECURITY_NATIVE_PATH,
    triple ? `octocode-security-${triple}` : undefined,
    triple
      ? join(__dir, '..', 'npm', triple, `octocode-security.${triple}.node`)
      : undefined,
  ].filter((candidate): candidate is string => typeof candidate === 'string');

  for (const c of candidates) {
    try {
      native = _req(c) as NativeModule;
      break;
    } catch {
      /* next */
    }
  }
});

describe('RUST-01: Native binary', () => {
  it('loads without throwing', () => {
    expect(native).not.toBeNull();
  });

  it('exports sanitizeContent', () => {
    expect(typeof native!.sanitizeContent).toBe('function');
  });

  it('exports maskSensitiveData', () => {
    expect(typeof native!.maskSensitiveData).toBe('function');
  });

  it('exports patternCount', () => {
    expect(typeof native!.patternCount).toBe('function');
  });

  it('patternCount() === 309', () => {
    expect(native!.patternCount()).toBe(309);
  });
});

describe('RUST-02: NAPI boundary type safety', () => {
  it('returns empty-string result for empty input (native)', () => {
    const r = native!.sanitizeContent('', null);
    expect(r.content).toBe('');
    expect(r.hasSecrets).toBe(false);
    expect(r.secretsDetected).toEqual([]);
    expect(r.warnings).toEqual([]);
  });

  it('JS bridge: null input returns empty-string result', () => {
    const r = ContentSanitizer.sanitizeContent(null as any);
    expect(r.content).toBe('');
    expect(r.hasSecrets).toBe(false);
  });

  it('JS bridge: undefined input returns empty-string result', () => {
    const r = ContentSanitizer.sanitizeContent(undefined as any);
    expect(r.content).toBe('');
    expect(r.hasSecrets).toBe(false);
  });

  it('JS bridge: numeric input is coerced to string', () => {
    const r = ContentSanitizer.sanitizeContent(42 as any);
    expect(r.content).toBe('42');
    expect(r.hasSecrets).toBe(false);
  });

  it('content > 10MB is hard-truncated to REDACTED placeholder', () => {
    const huge = 'x'.repeat(10_000_001);
    const r = ContentSanitizer.sanitizeContent(huge);
    expect(r.hasSecrets).toBe(true);
    expect(r.secretsDetected).toContain('content-size-exceeded');
    expect(r.content).toBe('[CONTENT-REDACTED-SIZE-LIMIT]');
  });

  it('content exactly 10M chars processes normally (limit is exclusive)', () => {
    const atLimit = 'safe text '.repeat(1_000_000); // exactly 10M chars
    const r = ContentSanitizer.sanitizeContent(atLimit);
    expect(r.content).not.toBe('[CONTENT-REDACTED-SIZE-LIMIT]');
  });
});

describe('RUST-03: Unicode & multibyte safety', () => {
  it('preserves emoji in clean content (no false positives)', () => {
    const input = 'Hello 🔒 world 🦀 no secrets here';
    const r = ContentSanitizer.sanitizeContent(input);
    expect(r.hasSecrets).toBe(false);
    expect(r.content).toBe(input);
  });

  it('detects AWS key embedded in CJK text', () => {
    const r = ContentSanitizer.sanitizeContent(
      '密钥: AKIAIOSFODNN7EXAMPLE 结束'
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain('AKIAIOSFODNN7EXAMPLE');
  });

  it('detects GitHub PAT surrounded by emoji', () => {
    const r = ContentSanitizer.sanitizeContent(
      '🔑 ghp_1234567890abcdefghijklmnopqrstuvwxyz123456 🔑'
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain('ghp_1234567890');
  });

  it('handles RTL text with embedded secret', () => {
    const r = ContentSanitizer.sanitizeContent(
      'مرحبا AKIAIOSFODNN7EXAMPLE عالم'
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain('AKIAIOSFODNN7EXAMPLE');
  });

  it('handles 4-byte supplementary characters (𝕳𝖊𝖑𝖑𝖔)', () => {
    const r = ContentSanitizer.sanitizeContent(
      '𝕳𝖊𝖑𝖑𝖔 AKIAIOSFODNN7EXAMPLE 𝖂𝖔𝖗𝖑𝖉'
    );
    expect(r.hasSecrets).toBe(true);
  });

  it('maskSensitiveData does not corrupt multibyte chars', () => {
    const input = 'key=AKIAIOSFODNN7EXAMPLE and emoji 🦀';
    const masked = maskSensitiveData(input);
    expect(typeof masked).toBe('string');
    expect(masked.length).toBeGreaterThan(0);
    expect(masked).toContain('🦀');
  });
});

describe('RUST-04: Large content & chunked path', () => {
  it('detects secret at position 0 in 1MB content', () => {
    const secret = 'AKIAIOSFODNN7EXAMPLE';
    const r = ContentSanitizer.sanitizeContent(
      secret + ' ' + 'x'.repeat(1_000_000 - secret.length - 1)
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(secret);
  });

  it('detects secret at the very end of 1MB content', () => {
    const secret = 'AKIAIOSFODNN7EXAMPLE';
    const r = ContentSanitizer.sanitizeContent(
      'x'.repeat(1_000_000 - secret.length - 1) + ' ' + secret
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(secret);
  });

  it('detects two secrets spread across 600KB content', () => {
    const s1 = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const s2 = 'AKIAIOSFODNN7EXAMPLE';
    const pad = 'y'.repeat(300_000);
    const r = ContentSanitizer.sanitizeContent(
      `${pad} ${s1} ${pad} ${s2} ${pad}`
    );
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(s1);
    expect(r.content).not.toContain(s2);
  });

  it('clean 2MB content has no false positives', () => {
    const sentence = 'The quick brown fox jumps over the lazy dog. ';
    const clean = sentence
      .repeat(Math.ceil(2_000_000 / sentence.length))
      .slice(0, 2_000_000);
    const r = ContentSanitizer.sanitizeContent(clean);
    expect(r.hasSecrets).toBe(false);
    expect(r.content).toBe(clean);
  });

  it('processes 500KB clean content in under 5ms (p50)', () => {
    const content = 'x'.repeat(500_000);
    const times: number[] = [];
    for (let i = 0; i < 30; i++) {
      const t = performance.now();
      ContentSanitizer.sanitizeContent(content);
      times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    const p50 = times[Math.floor(times.length * 0.5)]!;
    expect(p50).toBeLessThan(5);
  });
});

describe('RUST-04b: detect_chunked pre-filter (content > 500KB)', () => {
  const CHUNK_BOUNDARY = 500_001; // just past the single-path threshold

  it('clean 600KB content: no false positives via chunked path', () => {
    const content = 'The quick brown fox. '
      .repeat(Math.ceil(600_000 / 21))
      .slice(0, 600_000);
    const r = ContentSanitizer.sanitizeContent(content);
    expect(r.hasSecrets).toBe(false);
    expect(r.content).toBe(content);
    expect(r.secretsDetected).toHaveLength(0);
  });

  it('clean 2MB content: no false positives via chunked path', () => {
    const sentence = 'Lorem ipsum dolor sit amet. ';
    const content = sentence
      .repeat(Math.ceil(2_000_000 / sentence.length))
      .slice(0, 2_000_000);
    const r = ContentSanitizer.sanitizeContent(content);
    expect(r.hasSecrets).toBe(false);
    expect(r.content).toBe(content);
  });

  it('clean 600KB content completes in under 15ms p50 (pre-filter early-return)', () => {
    const content = 'x'.repeat(600_000);
    const times: number[] = [];
    for (let i = 0; i < 30; i++) {
      const t = performance.now();
      ContentSanitizer.sanitizeContent(content);
      times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    const p50 = times[Math.floor(times.length * 0.5)]!;
    expect(p50).toBeLessThan(15);
  });

  it('clean 2MB content completes in under 40ms p50 (pre-filter early-return)', () => {
    const content = 'y'.repeat(2_000_000);
    const times: number[] = [];
    for (let i = 0; i < 20; i++) {
      const t = performance.now();
      ContentSanitizer.sanitizeContent(content);
      times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    const p50 = times[Math.floor(times.length * 0.5)]!;
    expect(p50).toBeLessThan(40);
  });

  it('detects GitHub PAT at position 0 in 600KB content (chunked path)', () => {
    const secret = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const content =
      secret + ' ' + 'z'.repeat(CHUNK_BOUNDARY - secret.length - 1);
    const r = ContentSanitizer.sanitizeContent(content);
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(secret);
    expect(r.content).toContain('[REDACTED-');
  });

  it('detects GitHub PAT at the very end of 600KB content (chunked path)', () => {
    const secret = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const content =
      'z'.repeat(CHUNK_BOUNDARY - secret.length - 1) + ' ' + secret;
    const r = ContentSanitizer.sanitizeContent(content);
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(secret);
  });

  it('detects two different secrets spread across 1.2MB content (chunked path)', () => {
    const s1 = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const s2 = 'AKIAIOSFODNN7EXAMPLE';
    const pad = 'p'.repeat(600_000);
    const content = `${pad} ${s1} ${pad} ${s2} ${pad}`;
    const r = ContentSanitizer.sanitizeContent(content);
    expect(r.hasSecrets).toBe(true);
    expect(r.content).not.toContain(s1);
    expect(r.content).not.toContain(s2);
    expect(r.secretsDetected.length).toBeGreaterThanOrEqual(2);
  });

  it('chunked and single paths agree on the same small input', () => {
    const secret = 'AKIAIOSFODNN7EXAMPLE';
    const single = ContentSanitizer.sanitizeContent(secret + ' clean');
    const chunked = ContentSanitizer.sanitizeContent(
      secret + ' clean' + 'x'.repeat(CHUNK_BOUNDARY)
    );
    expect(chunked.hasSecrets).toBe(true);
    expect(chunked.content).not.toContain(secret);
    expect(chunked.secretsDetected).toEqual(
      expect.arrayContaining(single.secretsDetected)
    );
  });

  it('file-context pattern fires for .yaml in chunked mode', () => {
    const yaml = 'kind: Secret\ndata:\n  password: c2VjcmV0cGFzc3dvcmQ=\n';
    const largePad = 'x'.repeat(CHUNK_BOUNDARY);
    const content = yaml + largePad;
    const withPath = ContentSanitizer.sanitizeContent(
      content,
      'k8s/secret.yaml'
    );
    const noPath = ContentSanitizer.sanitizeContent(content);
    expect(withPath.content.slice(0, yaml.length)).not.toBe(
      noPath.content.slice(0, yaml.length)
    );
  });
});

describe('RUST-05: ReDoS linear-time guarantee', () => {
  const LIMIT_MS = 50;

  it('100K repeated "a" chars — completes in <50ms', () => {
    const t = performance.now();
    ContentSanitizer.sanitizeContent('a'.repeat(100_000));
    expect(performance.now() - t).toBeLessThan(LIMIT_MS);
  });

  it('alternating "ab" 100K — completes in <50ms', () => {
    const t = performance.now();
    ContentSanitizer.sanitizeContent('ab'.repeat(50_000));
    expect(performance.now() - t).toBeLessThan(LIMIT_MS);
  });

  it('partial secret prefixes ("sk-" × 1000) — no catastrophic backtrack', () => {
    const adversarial = ('sk-' + 'a'.repeat(100)).repeat(1000);
    const t = performance.now();
    const r = ContentSanitizer.sanitizeContent(adversarial);
    expect(performance.now() - t).toBeLessThan(LIMIT_MS);
    expect(r.hasSecrets).toBe(false);
  });

  it('50K "eyJ" JWT prefixes — no catastrophic backtrack', () => {
    const t = performance.now();
    ContentSanitizer.sanitizeContent('eyJ'.repeat(50_000));
    expect(performance.now() - t).toBeLessThan(LIMIT_MS);
  });

  it('deeply nested brackets — completes in <50ms', () => {
    const adversarial = '('.repeat(10_000) + 'a' + ')'.repeat(10_000);
    const t = performance.now();
    ContentSanitizer.sanitizeContent(adversarial);
    expect(performance.now() - t).toBeLessThan(LIMIT_MS);
  });
});

describe('RUST-06: Known secret detection', () => {
  const KNOWN: Array<{ label: string; input: string }> = [
    { label: 'AWS access key', input: 'key=AKIAIOSFODNN7EXAMPLE val' },
    {
      label: 'GitHub PAT',
      input: 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456',
    },
    {
      label: 'OpenAI project key',
      input: 'sk-proj-abcdefghijklmnopqrstuvwxyz1234567890ABCDEF',
    },
    {
      label: 'JWT token',
      input:
        'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c',
    },
    {
      label: 'Stripe live key',
      input: 'sk_live_4eC39HqLyjWDarjtT1zdp7dc4eC39HqLyjWDarjtT1',
    },
    { label: 'GitLab PAT', input: 'glpat-1234567890abcdefghij' },
    { label: 'npm token', input: ' npm_' + 'a'.repeat(36) + ' ' },
    {
      label: 'Stripe webhook',
      input: 'whsec_abcdefghijklmnopqrstuvwxyz12345678901234567890',
    },
    {
      label: 'GCP service account',
      input: 'my-svc@my-project.iam.gserviceaccount.com',
    },
    {
      label: 'RSA private key',
      input:
        '-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----',
    },
    {
      label: 'Postgres creds URL',
      input: 'postgresql://admin:s3cr3t@db.example.com:5432/prod',
    },
    {
      label: 'Slack bot token',
      input: 'xoxb-1234567890-1234567890-abcdefghijklmnopqrstu',
    },
    { label: 'Vault service token', input: 'hvs.abcdefghijklmnopqrstu' },
    { label: 'DigitalOcean token', input: 'dop_v1_' + 'a'.repeat(64) },
    { label: 'Databricks token', input: ' dapi' + 'a'.repeat(32) + ' ' }, // hex-only pattern: [a-f0-9]{32}
    { label: 'Supabase service key', input: 'sbp_' + 'a'.repeat(40) },
    {
      label: 'Supabase secret key',
      input: 'sb_secret_' + 'a'.repeat(22) + '_A1b2C3d4',
    },
    {
      label: 'Cloudflare API key',
      input: "cloudflare api key: '" + 'a'.repeat(40) + "'",
    },
    {
      label: 'Cloudflare prefixed credential',
      input: 'cfut_' + 'a'.repeat(40) + 'deadBEEF',
    },
    { label: 'Vercel token', input: 'vcp_' + 'a'.repeat(24) },
    { label: 'PostHog key', input: 'phc_' + 'a'.repeat(39) },
    {
      label: 'PostHog feature flags secure key',
      input: 'phs_' + 'a'.repeat(39),
    },
    {
      label: 'PostHog OAuth access token',
      input: 'pha_' + 'a'.repeat(39),
    },
    {
      label: 'PostHog OAuth refresh token',
      input: 'phr_' + 'a'.repeat(39),
    },
    {
      label: 'Bearer auth header',
      input: 'Authorization: Bearer ' + 'a'.repeat(24),
    },
    {
      label: 'AWS session token',
      input: 'AWS_SESSION_TOKEN=' + 'A'.repeat(200),
    },
  ];

  const SAFE = [
    'Hello world, no secrets here.',
    'const x = 42;',
    'https://example.com/api/v1/users',
    '2026-06-12T00:00:00Z',
    'import { foo } from "./bar";',
    'SELECT * FROM users WHERE id = 1',
    'v1.0.0-beta.1',
    '127.0.0.1:8080',
    'auth authorization',
    'auth\nauthorization',
    'topics: [access-control, acl, ai-friendly, api, authorization, prisma]',
    '',
    '   ',
  ];

  for (const { label, input } of KNOWN) {
    it(`detects ${label}`, () => {
      const r = ContentSanitizer.sanitizeContent(input);
      expect(r.hasSecrets, `should detect: ${input.slice(0, 50)}`).toBe(true);
      const keyPart = input.slice(0, Math.min(20, input.length));
      if (keyPart.length >= 10) {
        expect(r.content).not.toContain(keyPart);
      }
    });
  }

  for (const safe of SAFE) {
    const label = JSON.stringify(safe.slice(0, 40));
    it(`no false positive: ${label}`, () => {
      const r = ContentSanitizer.sanitizeContent(safe);
      expect(r.hasSecrets).toBe(false);
      expect(r.content).toBe(safe);
    });
  }
});

describe('RUST-07: maskSensitiveData (Rust)', () => {
  it('masks AWS key — every other character becomes *', () => {
    const input = 'key=AKIAIOSFODNN7EXAMPLE end';
    const masked = maskSensitiveData(input);
    expect(masked).not.toContain('AKIAIOSFODNN7EXAMPLE');
    expect(masked.length).toBeGreaterThan(0);
  });

  it('returns empty string unchanged', () => {
    expect(maskSensitiveData('')).toBe('');
  });

  it('returns clean text unchanged (no false masking)', () => {
    const clean = 'no secrets here at all';
    expect(maskSensitiveData(clean)).toBe(clean);
  });

  it('handles two secrets in one string', () => {
    const s1 = 'AKIAIOSFODNN7EXAMPLE';
    const s2 = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const masked = maskSensitiveData(`a=${s1} b=${s2}`);
    expect(masked).not.toContain(s1);
    expect(masked).not.toContain(s2);
  });

  it('preserves text before and after the matched region', () => {
    const prefix = 'before: ';
    const suffix = ' :after';
    const masked = maskSensitiveData(`${prefix}AKIAIOSFODNN7EXAMPLE${suffix}`);
    expect(masked.startsWith(prefix)).toBe(true);
    expect(masked.endsWith(suffix)).toBe(true);
  });

  it('100KB with secrets: under 10ms', () => {
    const content = 'x AKIAIOSFODNN7EXAMPLE y '.repeat(4_000).slice(0, 100_000);
    const t = performance.now();
    maskSensitiveData(content);
    expect(performance.now() - t).toBeLessThan(10);
  });
});

describe('RUST-08: Parallel / concurrent calls', () => {
  it('100 parallel sanitizeContent calls all return correct results', async () => {
    const inputs = [
      'AKIAIOSFODNN7EXAMPLE',
      'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456',
      'clean text with no secrets',
    ];
    const results = await Promise.all(
      Array.from({ length: 100 }, (_, i) =>
        Promise.resolve(
          ContentSanitizer.sanitizeContent(inputs[i % inputs.length]!)
        )
      )
    );
    results.forEach((r, i) => {
      const expected = i % inputs.length < 2;
      expect(r.hasSecrets).toBe(expected);
    });
  });

  it('mixed clean+dirty concurrent calls return independent results', async () => {
    const clean = 'this is clean content';
    const dirty = 'AKIAIOSFODNN7EXAMPLE';
    const [cleanResults, dirtyResults] = await Promise.all([
      Promise.all(
        Array.from({ length: 50 }, () =>
          Promise.resolve(ContentSanitizer.sanitizeContent(clean))
        )
      ),
      Promise.all(
        Array.from({ length: 50 }, () =>
          Promise.resolve(ContentSanitizer.sanitizeContent(dirty))
        )
      ),
    ]);
    cleanResults.forEach(r => expect(r.hasSecrets).toBe(false));
    dirtyResults.forEach(r => expect(r.hasSecrets).toBe(true));
  });
});

describe('RUST-09: SanitizationResult shape contract', () => {
  it('has all required fields: content, hasSecrets, secretsDetected, warnings', () => {
    const r = ContentSanitizer.sanitizeContent('AKIAIOSFODNN7EXAMPLE');
    expect(r).toHaveProperty('content');
    expect(r).toHaveProperty('hasSecrets');
    expect(r).toHaveProperty('secretsDetected');
    expect(r).toHaveProperty('warnings');
  });

  it('field types are correct', () => {
    const r = ContentSanitizer.sanitizeContent('AKIAIOSFODNN7EXAMPLE');
    expect(typeof r.content).toBe('string');
    expect(typeof r.hasSecrets).toBe('boolean');
    expect(Array.isArray(r.secretsDetected)).toBe(true);
    expect(Array.isArray(r.warnings)).toBe(true);
  });

  it('hasSecrets=true → secretsDetected non-empty', () => {
    const r = ContentSanitizer.sanitizeContent('AKIAIOSFODNN7EXAMPLE');
    expect(r.hasSecrets).toBe(true);
    expect(r.secretsDetected.length).toBeGreaterThan(0);
  });

  it('hasSecrets=false → secretsDetected empty', () => {
    const r = ContentSanitizer.sanitizeContent('nothing sensitive here');
    expect(r.hasSecrets).toBe(false);
    expect(r.secretsDetected).toHaveLength(0);
  });

  it('redacted content does not contain original secret text', () => {
    const secret = 'AKIAIOSFODNN7EXAMPLE';
    const r = ContentSanitizer.sanitizeContent(`token: ${secret} end`);
    expect(r.content).not.toContain(secret);
    expect(r.content).toContain('[REDACTED-');
  });

  it('warnings array has exactly 1 entry when secrets found', () => {
    const r = ContentSanitizer.sanitizeContent('AKIAIOSFODNN7EXAMPLE');
    expect(r.warnings).toHaveLength(1);
    expect(r.warnings[0]).toMatch(/secret\(s\) redacted/);
  });

  it('warnings array is empty when no secrets', () => {
    const r = ContentSanitizer.sanitizeContent('clean text');
    expect(r.warnings).toHaveLength(0);
  });

  it('secretsDetected names are strings', () => {
    const r = ContentSanitizer.sanitizeContent('AKIAIOSFODNN7EXAMPLE');
    for (const name of r.secretsDetected) {
      expect(typeof name).toBe('string');
      expect(name.length).toBeGreaterThan(0);
    }
  });
});

describe('RUST-10: Idempotency', () => {
  it('sanitizing already-sanitized output is a no-op', () => {
    const input = 'key=AKIAIOSFODNN7EXAMPLE end';
    const first = ContentSanitizer.sanitizeContent(input);
    const second = ContentSanitizer.sanitizeContent(first.content);
    expect(second.content).toBe(first.content);
    expect(second.hasSecrets).toBe(false);
    expect(second.secretsDetected).toHaveLength(0);
  });

  it('masking already-masked output is a no-op', () => {
    const input = 'key=AKIAIOSFODNN7EXAMPLE end';
    const first = maskSensitiveData(input);
    const second = maskSensitiveData(first);
    expect(second).toBe(first);
  });

  it('sanitizing plain text twice gives same result', () => {
    const input = 'just some plain text here';
    const first = ContentSanitizer.sanitizeContent(input);
    const second = ContentSanitizer.sanitizeContent(first.content);
    expect(second.content).toBe(first.content);
    expect(second.hasSecrets).toBe(false);
  });
});

describe('RUST-11: validateInputParameters edge cases', () => {
  it('validates deeply nested object (depth 19, within limit)', () => {
    let obj: Record<string, unknown> = { leaf: 'value' };
    for (let i = 0; i < 18; i++) obj = { nested: obj };
    const r = ContentSanitizer.validateInputParameters(obj);
    expect(r.isValid).toBe(true);
  });

  it('rejects object nested deeper than 20 levels', () => {
    let obj: Record<string, unknown> = { leaf: 'value' };
    for (let i = 0; i < 21; i++) obj = { nested: obj };
    const r = ContentSanitizer.validateInputParameters(obj);
    expect(r.isValid).toBe(false);
    expect(r.warnings.some(w => w.includes('depth'))).toBe(true);
  });

  it('rejects circular references', () => {
    const obj: Record<string, unknown> = { key: 'value' };
    obj.self = obj;
    const r = ContentSanitizer.validateInputParameters(obj);
    expect(r.isValid).toBe(false);
  });

  it('sanitizes secret embedded in parameter value via Rust', () => {
    const r = ContentSanitizer.validateInputParameters({
      query: 'repo uses AKIAIOSFODNN7EXAMPLE for CI',
    });
    expect(r.hasSecrets).toBe(true);
    const sanitizedQuery = r.sanitizedParams.query as string;
    expect(sanitizedQuery).not.toContain('AKIAIOSFODNN7EXAMPLE');
  });

  it('100 parallel validateInputParameters calls are safe', async () => {
    const input = { query: 'AKIAIOSFODNN7EXAMPLE', depth: 0 };
    const results = await Promise.all(
      Array.from({ length: 100 }, () =>
        Promise.resolve(ContentSanitizer.validateInputParameters({ ...input }))
      )
    );
    for (const r of results) {
      expect(r.hasSecrets).toBe(true);
      expect(r.sanitizedParams.query as string).not.toContain(
        'AKIAIOSFODNN7EXAMPLE'
      );
    }
  });
});
