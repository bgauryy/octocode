import { describe, expect, it } from 'vitest';
import { contentDigest } from '@octocodeai/octocode-awareness/host';
import { buildSessionContext } from '@earendil-works/pi-coding-agent';
import { collectPiRetainedContentDigests } from '../src/adapters/pi-retained-context.js';

const timestamp = '2026-09-06T00:00:00.000Z';
const user = (id: string, parentId: string | null, text: string) => ({
  type: 'message', id, parentId, timestamp,
  message: { role: 'user', content: text, timestamp: 1 },
});

describe('Pi retained context adapter', () => {
  it('matches the public SDK compaction projection and excludes discarded history', () => {
    const entries = [
      user('old', null, 'discarded history'),
      user('kept', 'old', 'retained prefix'),
      { type: 'compaction', id: 'compact', parentId: 'kept', timestamp, summary: 'native summary', firstKeptEntryId: 'kept', tokensBefore: 100 },
      user('after', 'compact', 'retained after'),
    ];
    expect(buildSessionContext(entries as never).messages).toHaveLength(3);
    const digests = collectPiRetainedContentDigests({ sessionManager: { getBranch: () => entries } } as never);
    expect(digests).toContain(contentDigest('retained prefix'));
    expect(digests).toContain(contentDigest('retained after'));
    expect(digests).not.toContain(contentDigest('discarded history'));
  });

  it('accepts exact trusted segment bytes from a combined context message', () => {
    const plan = '<active_plan>\nGoal\n\n- inspect\n- verify\n</active_plan>';
    const memory = '<memory>same</memory>';
    const entries = [{ type: 'custom_message', id: 'context', parentId: null, timestamp,
      customType: 'octocode-context-update', content: `${plan}\n\n${memory}`, display: false,
      details: { segments: [
        { id: 'active-plan', digest: contentDigest(plan) },
        { id: 'session-memory', digest: contentDigest(memory) },
      ] },
    }];
    const digests = collectPiRetainedContentDigests({ sessionManager: { getBranch: () => entries } } as never, {
      knownSegmentContents: { 'active-plan': plan, 'session-memory': memory },
    });
    expect(digests).toContain(contentDigest(plan));
    expect(digests).toContain(contentDigest(memory));
  });

  it('rejects forged digests, changed bytes, and attacker substrings', () => {
    const plan = '<active_plan>trusted</active_plan>';
    const attacker = `prefix ${plan} suffix`;
    const entries = [{ type: 'custom_message', id: 'context', parentId: null, timestamp,
      customType: 'octocode-context-update', content: attacker, display: false,
      details: { segments: [{ id: 'active-plan', digest: contentDigest(plan) }] },
    }];
    const digests = collectPiRetainedContentDigests({ sessionManager: { getBranch: () => entries } } as never, {
      knownSegmentContents: { 'active-plan': plan },
      additionalRetainedContents: ['current turn context'],
    });
    expect(digests).not.toContain(contentDigest(plan));
    expect(digests).toContain(contentDigest(attacker));
    expect(digests).toContain(contentDigest('current turn context'));
  });

  it('keeps independently delivered current-turn content when no branch API is available', () => {
    const current = '<active_plan>current turn</active_plan>';
    const digests = collectPiRetainedContentDigests({ sessionManager: {} } as never, {
      additionalRetainedContents: [current],
    });
    expect(digests).toEqual(new Set([contentDigest(current)]));
  });
});
