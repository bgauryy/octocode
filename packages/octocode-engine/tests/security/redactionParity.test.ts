import { describe, expect, it } from 'vitest';

import {
  getSecurityBackendStatus,
  nativeSanitizeContent,
  sanitizeWithJsFallback,
} from '../../src/security/native.js';

// Differential test: the Rust engine and the pure-JS fallback must redact
// IDENTICALLY. `verify:patterns` only proves patterns.rs == the TS pattern
// source array; it does not prove the two ENGINES (Rust regex crate + REGEX_SET
// prefilter vs JS RegExp iteration) produce the same output over that shared
// array. This closes that gap and catches future drift (a JS-only regex feature,
// a replacement-casing mismatch, an ordering divergence, a file-context skew).

type Case = {
  name: string;
  content: string;
  path?: string;
  obviousSecret?: boolean;
};

const CORPUS: Case[] = [
  {
    name: 'github-pat',
    content: `const token = "ghp_${'a'.repeat(37)}";`,
    obviousSecret: true,
  },
  {
    name: 'aws-access-key',
    content: 'AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE',
  },
  {
    name: 'slack-bot-token',
    content: `SLACK_TOKEN=xoxb-123456789012-1234567890123-${'A'.repeat(24)}`,
  },
  {
    name: 'stripe-live-key',
    content: `stripe=sk_live_${'a'.repeat(24)}`,
  },
  {
    name: 'pem-private-key',
    content:
      '-----BEGIN RSA PRIVATE KEY-----\nMIIEabc123DEF456ghi789\n-----END RSA PRIVATE KEY-----',
    obviousSecret: true,
  },
  {
    // file-context-gated pattern: must behave the same in both engines when a
    // matching path is supplied.
    name: 'k8s-secret-yaml',
    content: 'apiVersion: v1\nkind: Secret\ndata:\n  password: c3VwZXJzZWNyZXQ=',
    path: 'k8s/secret.yaml',
  },
  {
    name: 'negative-uuid',
    content: 'requestId = 550e8400-e29b-41d4-a716-446655440000',
  },
  {
    name: 'negative-prose',
    content: 'The quick brown fox jumps over the lazy dog thirteen times.',
  },
];

describe('secret redaction — Rust native vs JS fallback parity', () => {
  const backend = getSecurityBackendStatus().backend;
  const nativeLoaded = backend === 'native';

  // Silent-skip guard: `it.skipIf(!nativeLoaded)` below means that without a
  // built native addon the whole parity suite skips and still reports green —
  // a false pass that proves nothing about Rust↔JS equivalence. In any run that
  // is supposed to exercise native (the engine CI job sets OCTOCODE_REQUIRE_NATIVE),
  // turn that silent skip into a hard failure. Local dev without a native build
  // legitimately skips.
  it('native addon is loaded when the environment requires it', () => {
    if (process.env.OCTOCODE_REQUIRE_NATIVE) {
      expect(
        nativeLoaded,
        `native addon not loaded (backend=${backend}) while OCTOCODE_REQUIRE_NATIVE is set — ` +
          `the redaction-parity tests would silently skip and report false-green. Build the addon first (yarn build:dev).`
      ).toBe(true);
    } else {
      expect(nativeLoaded || backend === 'fallback').toBe(true);
    }
  });

  it.skipIf(!nativeLoaded)(
    'native and JS fallback produce identical redaction over the corpus',
    () => {
      for (const { name, content, path } of CORPUS) {
        const p = path ?? null;
        const native = nativeSanitizeContent(content, p);
        const js = sanitizeWithJsFallback(content, p);

        expect(native.content, `redacted content mismatch for ${name}`).toBe(
          js.content
        );
        expect(native.hasSecrets, `hasSecrets mismatch for ${name}`).toBe(
          js.hasSecrets
        );
        expect(
          [...native.secretsDetected].sort(),
          `secretsDetected mismatch for ${name}`
        ).toEqual([...js.secretsDetected].sort());
      }
    }
  );

  it.skipIf(!nativeLoaded)(
    'both engines actually redact obvious secrets (guards against no-op parity)',
    () => {
      for (const { name, content } of CORPUS.filter(c => c.obviousSecret)) {
        const native = nativeSanitizeContent(content, null);
        const js = sanitizeWithJsFallback(content, null);
        expect(native.hasSecrets, `${name} must be detected by native`).toBe(
          true
        );
        expect(js.hasSecrets, `${name} must be detected by JS fallback`).toBe(
          true
        );
        expect(native.content).toContain('[REDACTED-');
      }
    }
  );
});
