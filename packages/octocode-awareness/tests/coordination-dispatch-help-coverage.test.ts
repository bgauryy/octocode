import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openAwarenessStore } from '../src/coordination/open.js';
import type { AwarenessStore } from '../src/coordination/coordination-continuity.js';
import { dispatchAwarenessCommand } from '../src/coordination/dispatch.js';
import { focusedCoordinationUsage } from '../src/coordination/cli-help.js';

let root: string;
let store: AwarenessStore;

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'aw-dispatch-help-'));
  store = openAwarenessStore({ workspace: root, dbPath: join(root, 'awareness.sqlite3') });
});

afterEach(async () => {
  store.close();
  await rm(root, { recursive: true, force: true });
});

describe('coordination dispatch and focused help contracts', () => {
  it('routes status, agent presence, and scoped handoff lifecycle through the public dispatcher', () => {
    expect(dispatchAwarenessCommand(store, { command: 'status', params: { staleAfterMs: '0' } }))
      .toMatchObject({ exitCode: 0, result: { handoffs: 0 } });

    expect(dispatchAwarenessCommand(store, {
      command: 'agent', action: 'touch', params: { agentId: 'reviewer', status: 'IDLE' },
    })).toMatchObject({ result: { agentId: 'reviewer', status: 'IDLE' } });
    expect(dispatchAwarenessCommand(store, { command: 'status', params: { staleAfterMs: '0' } }))
      .toMatchObject({ result: { agents: 1, staleAgents: 0 } });
    expect(dispatchAwarenessCommand(store, {
      command: 'handoff', action: 'add', params: {
        agentId: 'reviewer', summary: 'Receipt is ready for review', files: ['src/a.ts', 'tests/a.test.ts'],
      },
    }).result).toMatchObject({ agentId: 'reviewer', files: ['src/a.ts', 'tests/a.test.ts'] });

    const listed = dispatchAwarenessCommand(store, { command: 'handoff', action: 'list' }).result as Array<{ handoffId: string }>;
    expect(listed).toHaveLength(1);
    expect(dispatchAwarenessCommand(store, {
      command: 'handoff', action: 'clear', params: { handoffId: listed[0]!.handoffId },
    })).toEqual({ result: { cleared: true }, exitCode: 0 });
    expect(dispatchAwarenessCommand(store, {
      command: 'handoff', action: 'list', params: { includeCleared: true },
    }).result).toHaveLength(1);
    expect(dispatchAwarenessCommand(store, {
      command: 'agent', action: 'leave', params: { agentId: 'reviewer' },
    })).toMatchObject({ result: { agentId: 'reviewer', status: 'LEFT' } });
  });

  it('routes verified-memory operations with their coercion and safety boundaries', () => {
    const stored = dispatchAwarenessCommand(store, {
      command: 'memory', action: 'store-verified', params: {
        label: 'BUILD', text: 'Run the focused verification receipt before merge.', sourceDigest: 'sha256:receipt',
        scope: 'project', verifiedAt: '2026-09-01T00:00:00.000Z', validUntil: '2026-10-01T00:00:00.000Z',
        importance: '9', tags: ['verification', 'merge'],
      },
    }).result as { memoryId: string; importance: number };
    expect(stored).toMatchObject({ importance: 9 });

    expect(dispatchAwarenessCommand(store, {
      command: 'memory', action: 'recall-verified', params: {
        query: 'focused verification', sourceDigest: 'sha256:receipt', scope: 'project', limit: '1', now: '2026-09-02T00:00:00.000Z',
      },
    }).result).toMatchObject([{ memoryId: stored.memoryId, sourceDigest: 'sha256:receipt' }]);
    expect(dispatchAwarenessCommand(store, {
      command: 'memory', action: 'reindex', params: { force: true, limit: '2' },
    }).result).toEqual({ enabled: false, scanned: 0, embedded: 0 });
    expect(dispatchAwarenessCommand(store, {
      command: 'memory', action: 'prune', params: { olderThanMs: '1', label: 'BUILD' },
    })).toMatchObject({ exitCode: 0, result: { dryRun: true } });

    expect(() => dispatchAwarenessCommand(store, {
      command: 'memory', action: 'evaluate', params: { corpusJson: '"not-a-corpus"' },
    })).toThrow('memory evaluate corpus-json must be a JSON object');
    expect(() => dispatchAwarenessCommand(store, {
      command: 'memory', action: 'store-verified', params: { label: 'BUILD' },
    })).toThrow('memory store-verified requires text');
  });

  it('renders catalog-backed focused help for each owned route and leaves unknown routes untouched', () => {
    const cases = [
      [{ command: 'status' }, 'usage: npx @octocodeai/octocode-awareness status [options]', '--stale-after <number>'],
      [{ command: 'handoff' }, 'actions: add (Record a handoff.)', 'global flags:'],
      [{ command: 'handoff', action: 'add' }, '--summary <value>', '--agent-id <value>'],
      [{ command: 'handoff', action: 'list' }, '--include-cleared', 'schema: handoff'],
      [{ command: 'handoff', action: 'clear' }, '--handoff-id <value>', 'schema: handoff'],
      [{ command: 'agent' }, 'actions: touch (Refresh presence.)', 'leave (End presence.)'],
      [{ command: 'agent', action: 'touch' }, '--status <value>', 'schema: agent_presence'],
      [{ command: 'agent', action: 'leave' }, '--agent-id <value>', 'schema: agent_presence'],
      [{ command: 'guide' }, '--json <value>', 'usage: npx @octocodeai/octocode-awareness guide [options]'],
      [{ command: 'instructions', action: 'export' }, '--format <value>', 'usage: npx @octocodeai/octocode-awareness instructions export [options]'],
      [{ command: 'memory', action: 'store-verified' }, '--source-digest <value>', 'schema: verified_memory'],
      [{ command: 'memory', action: 'recall-verified' }, '--min-similarity <value>', 'schema: verified_recall'],
      [{ command: 'memory', action: 'evaluate' }, '--corpus-json <value>', 'schema: memory_evaluate'],
      [{ command: 'memory', action: 'reindex' }, '--force <value>', 'schema: memory_reindex'],
      [{ command: 'memory', action: 'prune' }, '--confirm <value>', 'schema: memory_prune'],
    ] as const;

    for (const [route, ...tokens] of cases) {
      const help = focusedCoordinationUsage(route);
      expect(help, JSON.stringify(route)).toBeDefined();
      for (const token of tokens) expect(help).toContain(token);
    }
    expect(focusedCoordinationUsage({ command: 'unknown', action: 'route' })).toBeUndefined();
  });

  it('advertises only canonical root CLI forms in the JSON guide', async () => {
    const { getExternalAgentAwarenessGuide } = await import('../src/coordination/external-policy.js');
    const guide = getExternalAgentAwarenessGuide();
    expect(guide.commands).not.toHaveLength(0);
    expect(guide.commands.map((entry) => entry.cli)).toEqual(expect.arrayContaining([
      'npx @octocodeai/octocode-awareness status',
      'npx @octocodeai/octocode-awareness handoff',
      'npx @octocodeai/octocode-awareness agent',
    ]));
    expect(guide.commands.some((entry) => entry.cli.startsWith('coordination '))).toBe(false);
  });
});
