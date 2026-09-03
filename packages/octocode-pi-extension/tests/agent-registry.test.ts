import { describe, expect, it } from 'vitest';
import { withPeerCoordination } from '../src/tools/agent-tools.js';

describe('withPeerCoordination', () => {
  it('appends self id and peer ids, excluding self and blanks', () => {
    const out = withPeerCoordination('do work', 'me', ['me', 'peer-a', '', 'peer-b']);
    expect(out).toContain('do work');
    expect(out).toContain('your agent id: me');
    expect(out).toContain('peers: peer-a, peer-b');
    expect(out).toContain('octocode-awareness guide');
    expect(out).not.toContain('message send');
  });

  it('appends parent id and durable handback file when provided', () => {
    const out = withPeerCoordination('do work', 'worker-1', [], {
      parentId: 'parent-1',
      handbackPath: '/repo/.octocode/tmp/agents/abc/handback.md',
    });
    expect(out).toContain('parent agent id: parent-1');
    expect(out).toContain('durable handback file: /repo/.octocode/tmp/agents/abc/handback.md');
    expect(out).toContain('[ARTIFACT] <path>');
  });

  it('notes no peers when only self is present', () => {
    const out = withPeerCoordination('do work', 'me', ['me']);
    expect(out).toContain('peers: none yet');
  });

  it('is a no-op when there is no self id', () => {
    expect(withPeerCoordination('do work', undefined, ['peer-a'])).toBe('do work');
  });
});
