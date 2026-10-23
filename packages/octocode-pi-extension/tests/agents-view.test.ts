import { describe, expect, it, vi } from 'vitest';
import type { Member, Traffic } from '../src/team/model.js';
import { AgentsView } from '../src/team/panel.js';
import type { TeamView } from '../src/team/ticker.js';
import { fakeCtx, theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { member } from './team-harness.js';

/** A Team stand-in that honors the watch contract: one listener call now and one per second. */
function fakeTeam(read: () => TeamView, id?: string): ConstructorParameters<typeof AgentsView>[0] {
  return {
    id,
    get view() {
      return read();
    },
    watch(listener: () => void) {
      const timer = setInterval(listener, 1_000);
      listener();
      return () => clearInterval(timer);
    },
  };
}

const now = Date.now();

const peer = (id: string, status: 'working' | 'idle' = 'working', parentId: string | null = 'me-0001'): Member => member(id, { status, ...(parentId ? { parentId } : {}), joinedAt: now - 5_000, updatedAt: now, toolCalls: 2, input: 1000, output: 200, goal: 'review the parser', activity: 'localSearch foo' });

describe('AgentsView', () => {
  it('shows a widget for this session\'s subagents only, repaints, and clears when they leave', () => {
    vi.useFakeTimers();
    try {
      const otherSession = [peer('main-9999', 'working', null), peer('researcher-8888', 'working', 'main-9999')];
      let members = [peer('me-0001', 'working', null), peer('reviewer-1a2b'), peer('tester-3c4d', 'idle'), ...otherSession];
      const traffic: Traffic[] = [
        { id: 2, from: 'me-0001', to: ['reviewer-1a2b'], text: 'look', at: now, state: 'delivered' },
        { id: 3, from: 'reviewer-1a2b', to: ['tester-3c4d'], text: 'between subagents', at: now, state: 'delivered' },
        { id: 4, from: 'main-9999', to: ['researcher-8888'], text: 'theirs', at: now, state: 'delivered' },
      ];
      const view = new AgentsView(fakeTeam(() => ({ members, traffic }), 'me-0001'));
      const ctx = fakeCtx({ cwd: tmp() });
      view.start(ctx);
      const factory = ctx.ui.widgets.get('octocode-agents');
      if (typeof factory !== 'function') throw new Error('expected a widget factory');
      const requestRender = vi.fn();
      const widget = factory({ requestRender }, theme);
      widget.invalidate();
      const lines = widget.render(120).join('\n');
      expect(lines).toContain('1 working · 1 idle');
      expect(lines).toContain('reviewer-1a2b');
      // Only traffic between subagents: this session's own messages are already in its transcript.
      expect(lines).toContain('#3');
      expect(lines).not.toContain('#2');
      expect(lines).not.toContain('main-9999');
      expect(lines).not.toContain('researcher-8888');
      expect(lines).not.toContain('#4');
      // The widget counts the agents; the footer does not repeat them.
      expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();
      vi.advanceTimersByTime(1_000);
      expect(requestRender).toHaveBeenCalled();

      members = [peer('me-0001', 'working', null), ...otherSession];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();

      members = [peer('me-0001', 'working', null), peer('reviewer-1a2b', 'idle')];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(true);
      view.stop(ctx);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('never mounts a widget for subagents whose heartbeat stopped, and clears it once they go silent', () => {
    vi.useFakeTimers({ now });
    try {
      const silent = { ...peer('reviewer-1a2b'), updatedAt: now - 16_000 };
      let members = [peer('me-0001', 'working', null), silent];
      const view = new AgentsView(fakeTeam(() => ({ members, traffic: [] }), 'me-0001'));
      const ctx = fakeCtx({ cwd: tmp() });
      view.start(ctx);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      members = [peer('me-0001', 'working', null), { ...peer('reviewer-1a2b'), updatedAt: Date.now() }];
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(true);
      // The heartbeat stops: 16 s later the agent is unresponsive and the widget unmounts.
      vi.advanceTimersByTime(16_000);
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      view.stop(ctx);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('AgentsView', () => {
  it('mounts the widget once a database error clears, and shows a transient error as a retry, not as team off', () => {
    vi.useFakeTimers({ now });
    try {
      let view: TeamView = { members: [], traffic: [], error: 'Team database unavailable: database is locked', retrying: true };
      const agents = new AgentsView(fakeTeam(() => view, 'me-0001'));
      const ctx = fakeCtx({ cwd: tmp() });
      agents.start(ctx);
      expect(ctx.ui.statuses.get('octocode-team')).toBe('team retrying: Team database unavailable: database is locked');
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(false);
      view = { members: [peer('me-0001', 'working', null), peer('reviewer-1a2b')], traffic: [] };
      vi.advanceTimersByTime(1_000);
      expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();
      expect(ctx.ui.widgets.has('octocode-agents')).toBe(true);
      agents.stop(ctx);
    } finally {
      vi.useRealTimers();
    }
  });

  it('reports an unusable team database in the status line and survives read failures', () => {
    const ctx = fakeCtx({ cwd: tmp() });
    const view = new AgentsView(fakeTeam(() => ({ members: [], traffic: [], error: 'Team database schema v9 is newer\nupgrade' })));
    view.start(ctx);
    expect(ctx.ui.statuses.get('octocode-team')).toBe('team off: Team database schema v9 is newer');
    view.stop(ctx);
    expect(ctx.ui.statuses.get('octocode-team')).toBeUndefined();

    const failing = new AgentsView(
      fakeTeam(() => {
        throw new Error('boom');
      }),
    );
    expect(() => failing.start(ctx)).not.toThrow();
    failing.stop();

    const headless = fakeCtx({ cwd: tmp(), hasUI: false });
    const read = vi.fn(() => ({ members: [], traffic: [] }));
    new AgentsView(fakeTeam(read)).start(headless);
    expect(read).not.toHaveBeenCalled();
  });
});
