import { expect, it } from 'vitest';
import { containsSecretLikeText } from '../src/memory-hardening.js';

it('rejects hyphenated API credentials and authorization bearer values before archiving', () => {
  expect(containsSecretLikeText(`Example credential: ${'sk-'}${'A'.repeat(32)}`)).toBe(true);
  expect(containsSecretLikeText(`Authorization: Bearer ${'b'.repeat(32)}`)).toBe(true);
  expect(containsSecretLikeText(JSON.stringify({ api_key: 'c'.repeat(32) }))).toBe(true);
});

it('does not mistake discussion of credentials for a stored credential', () => {
  expect(containsSecretLikeText('Use the configured API key. Never store bearer credentials.')).toBe(false);
});
