import { expect, it } from 'vitest';
import { buildFooterDiagnostics } from '../src/tools/footer-diagnostics.js';

it.each([
  ['authenticated', 'GitHub signed in'],
  ['missing', 'GitHub not signed in'],
  ['checking', 'GitHub checking'],
  ['error', 'GitHub status unavailable'],
] as const)('shows truthful GitHub state: %s', (githubStatus, label) => {
  const rows = buildFooterDiagnostics({ identity: [], metrics: [], statuses: {},
    githubStatus, awareness: null, awarenessHealth: { state: 'idle' } });
  expect(rows.flatMap(row => row.segments).map(segment => segment.text)).toContain(label);
});
