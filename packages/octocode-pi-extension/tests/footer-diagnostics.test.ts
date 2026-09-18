import { expect, it } from 'vitest';
import { buildFooterDiagnostics } from '../src/tools/footer-diagnostics.js';

it('hides github row when authenticated (happy-path noise suppression)', () => {
  const rows = buildFooterDiagnostics({ identity: [], metrics: [], statuses: {},
    githubStatus: 'authenticated', awareness: null, awarenessHealth: { state: 'idle' } });
  const texts = rows.flatMap(row => row.segments).map(segment => segment.text);
  expect(texts).not.toContain('GitHub signed in');
  expect(rows.some(row => row.id === 'github')).toBe(false);
});

it.each([
  ['missing', 'GitHub not signed in'],
  ['checking', 'GitHub checking'],
  ['error', 'GitHub status unavailable'],
] as const)('shows actionable github state: %s', (githubStatus, label) => {
  const rows = buildFooterDiagnostics({ identity: [], metrics: [], statuses: {},
    githubStatus, awareness: null, awarenessHealth: { state: 'idle' } });
  expect(rows.flatMap(row => row.segments).map(segment => segment.text)).toContain(label);
});
