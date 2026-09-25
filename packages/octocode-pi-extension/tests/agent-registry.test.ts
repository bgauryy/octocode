import { describe, expect, it } from 'vitest';
import { withWorkerCoordination } from '../src/tools/agents/coordination.js';

describe('withWorkerCoordination', () => {
  it('routes communication discovery through the bound peers tool', () => {
    const out = withWorkerCoordination('do work');
    expect(out).toContain('do work');
    expect(out).toContain('call peers to discover recipient session IDs');
    expect(out).toContain('Local worker identities are not communication recipient IDs');
    expect(out).not.toMatch(/your agent id:|parent agent id:|peers:/);
  });

  it('preserves the exact durable handback destination without a synthetic parent route', () => {
    const out = withWorkerCoordination('do work', {
      handbackPath: '/repo/.octocode/tmp/agents/abc/handback.md',
    });
    expect(out).toContain('durable handback file: /repo/.octocode/tmp/agents/abc/handback.md');
    expect(out).toContain('[ARTIFACT] <path>');
    expect(out).not.toContain('parent agent id:');
  });
});
