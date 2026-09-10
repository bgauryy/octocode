import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  RECENT_AGENT_OUTCOME_MS,
} from '../src/tools/agents/display-state.js';
import {
  selectStatusRows,
  type StatusDensity,
} from '../src/tui/status-policy.js';
import type { UxSnapshotV1 } from '../src/tools/ux-snapshot.js';
import { renderFooterView } from '../src/tui/footer-view.js';

function snapshot(overrides: Partial<UxSnapshotV1> = {}): UxSnapshotV1 {
  return {
    version: 1,
    observedAt: 10_000,
    session: {
      phase: 'ready',
      activity: {
        kind: 'working',
        label: 'Working · task 2 Implement',
        since: 8_000,
      },
      elapsedMs: 9_000,
      contextPressure: 61,
      observationTime: 10_000,
    },
    goal: { text: 'Ship adaptive status', nextAction: 'Run verification' },
    plan: {
      id: 'plan-1',
      phase: 'executing',
      revision: 'rev-1',
      total: 4,
      displayTotal: 4,
      done: 1,
      active: 1,
      ready: 1,
      blocked: 1,
      verifying: 0,
      failed: 0,
      dynamic: false,
      progressMode: 'linear',
      detailRoute: 'plan',
    },
    tasks: [
      {
        id: 't1',
        index: 1,
        label: 'Research',
        status: 'done',
        dependencies: [],
        verification: 'passed',
        updatedAt: 10_000,
      },
      {
        id: 't2',
        index: 2,
        label: 'Implement',
        activeLabel: 'Implementing',
        status: 'doing',
        dependencies: [1],
        verification: 'pending',
        updatedAt: 10_000,
      },
    ],
    agents: [],
    attention: [],
    messages: { unread: 0, queued: 0, detailRoute: '/octocode-inbox' },
    provenance: [],
    ...overrides,
  };
}

function agent(index: number, state = 'running') {
  return {
    id: `agent-${index}`,
    label: `worker-${index}`,
    state,
    pendingMessages: 0,
    elapsedMs: 2_000,
    updatedAt: 10_000 - index,
  };
}

test('automatic footer lists active workers with updates beside the running plan and task', () => {
  const source = snapshot({
    session: { ...snapshot().session, activity: { kind: 'working', label: 'MCPTool localSearch', since: 8000 } },
    agents: [
      { ...agent(1), label: 'atlas', activeOperation: 'tool localSearch' },
      { ...agent(2, 'queued'), label: 'nova', activeOperation: 'follow-up ready' },
      { ...agent(3, 'done'), label: 'old-worker' },
      { ...agent(4, 'killed'), label: 'stopped-worker' },
    ],
  });
  const selected = selectStatusRows(source, { width:120, height:24 });
  const text = renderFooterView({ rows:selected.rows }, { width:120 }).join('\n');
  assert.match(text, /running.*atlas.*tool localSearch/);
  assert.match(text, /queued.*nova.*follow-up ready/);
  assert.match(text, /Plan 1\/4.*task 2 running.*Implementing/);
  assert.doesNotMatch(text, /old-worker|stopped-worker/);
  assert.equal(selected.rows.length, 3);
});

test('compact plan keeps the current task while a tool is running', () => {
  const source = snapshot({ session: { ...snapshot().session, activity: { kind:'working', label:'MCPTool localSearch' } } });
  const selected = selectStatusRows(source, { width:100, height:24, density:'compact' });
  const text = renderFooterView({ rows:selected.rows }, { width:100 }).join('\n');
  assert.match(text, /Plan 1\/4.*task 2 running.*Implementing/);
  assert.match(text, /MCPTool localSearch/);
});

test('compact worker overflow stays on the roster without displacing plan and task state', () => {
  const source = snapshot({ agents:[{...agent(1),label:'atlas'}, {...agent(2),label:'nova'}] });
  const selected = selectStatusRows(source, { width:52, height:24, density:'compact' });
  const lines = renderFooterView({ rows:selected.rows }, { width:52 });
  assert.match(lines[0]!, /Plan 1\/4.*task 2 running/);
  assert.doesNotMatch(lines[0]!, /inbox/);
  assert.match(lines[1]!, /running.*atlas.*\+1 agent.*inbox/);
});

test('active worker row order stays stable when updates arrive', () => {
  const source = snapshot({ plan:undefined, agents:[agent(2),agent(1)] });
  const options = { width:100, height:40 };
  const first = selectStatusRows(source, options);
  const second = selectStatusRows({ ...source, agents:[{...agent(1),updatedAt:20000},agent(2)] }, options);
  assert.deepEqual(first.rowIds, second.rowIds);
  assert.ok(first.rowIds.includes('worker:agent-1'));
});

test('plan phases distinguish review from execution and retain the verifying task', () => {
  const source = snapshot();
  for (const phase of ['draft', 'in_review', 'accepted', 'complete'] as const) {
    const selected = selectStatusRows({ ...source, plan:{...source.plan!,phase} }, { width:100, height:40 });
    const row = selected.rows[selected.rowIds.indexOf('plan:progress')]!;
    const text = row.map(segment => segment.text).join(' ');
    assert.match(text, new RegExp(`Plan ${phase.replaceAll('_', ' ')}`));
    assert.doesNotMatch(text, /running|Implementing/);
    if (phase === 'complete') assert.match(text, /1\/4 done/);
  }
  const selected = selectStatusRows({ ...source, plan:{...source.plan!,phase:'verifying'} }, { width:100, height:40 });
  const text = selected.rows[selected.rowIds.indexOf('plan:progress')]!.map(segment => segment.text).join(' ');
  assert.match(text, /Plan verifying.*task 2 verifying: Implementing/);
});

test('recent completed workers remain briefly visible, then leave the footer for the inbox', () => {
  const recent = selectStatusRows(
    snapshot({
      plan: undefined,
      agents: [{ ...agent(1, 'done'), label: 'atlas', updatedAt: 10_000 }],
    }),
    { width: 100, height: 40 }
  );
  assert.match(
    renderFooterView({ rows: recent.rows }, { width: 100 }).join('\n'),
    /done.*atlas.*\/octocode-inbox/
  );

  const expired = selectStatusRows(
    snapshot({
      observedAt: 10_000 + RECENT_AGENT_OUTCOME_MS + 1,
      plan: undefined,
      agents: [{ ...agent(1, 'done'), label: 'atlas', updatedAt: 10_000 }],
    }),
    { width: 100, height: 40 }
  );
  assert.doesNotMatch(
    renderFooterView({ rows: expired.rows }, { width: 100 }).join('\n'),
    /atlas/
  );
});

test('compact status keeps blocked workers visible when session metadata is present', () => {
  const result = selectStatusRows(
    snapshot({ agents: [{ ...agent(1, 'blocked'), label: 'parser-reviewer' }] }),
    {
      width: 52, height: 24, density: 'compact',
      diagnostics: [{ id: 'session', priority: 'P4', segments: [{ text: 'model · tools 12' }] }],
    }
  );
  const text = renderFooterView({ rows: result.rows }, { width: 52 }).join('\n');
  assert.match(text, /blocked/i);
  assert.match(text, /parser-reviewer/);
  assert.ok(!result.rowIds.includes('diagnostic:session'));
});

test('a one-row footer puts a failed worker ahead of ordinary activity and progress', () => {
  const result = selectStatusRows(
    snapshot({ agents: [{ ...agent(1, 'failed'), label: 'parser-reviewer' }] }),
    { width: 52, height: 10, density: 'automatic' }
  );
  assert.match(renderFooterView({ rows: result.rows }, { width: 52 })[0]!, /failed/i);
});

test('worker state remains readable at narrow widths with an inbox route', () => {
  for (const state of ['blocked', 'failed', 'error']) {
    for (const width of [20, 28, 36, 52]) {
      const result = selectStatusRows(
        snapshot({ plan: undefined, agents: [{ ...agent(1, state), label: 'parser-reviewer-with-a-long-name' }] }),
        { width, height: 40, density: 'compact' }
      );
      const text = renderFooterView({ rows: result.rows }, { width }).join('\n');
      assert.match(text, new RegExp(state, 'i'), `${state} at ${width} columns`);
      assert.match(text, /inbox/);
    }
  }
});

test('compact metadata stays below decisions and exposes additional pending decisions', () => {
  const result = selectStatusRows(
    snapshot({
      attention: [1, 2, 3].map(index => ({
        id: `input-${index}`,
        kind: 'input',
        priority: 'P0',
        severity: 'warning',
        actor: 'Octocode',
        reason: `Question ${index}`,
        detailRoute: 'interaction',
        createdAt: index,
      })),
    }),
    {
      width: 52,
      height: 40,
      density: 'compact',
      diagnostics: [
        {
          id: 'session',
          priority: 'P4',
          segments: [{ text: 'model · tools 5' }],
        },
      ],
    }
  );
  assert.equal(result.rows.length, 2);
  assert.equal(result.rowIds[1], 'attention:input-2');
  assert.ok(!result.rowIds.includes('diagnostic:session'));
  assert.match(
    renderFooterView({ rows: result.rows }, { width: 52 })[0]!,
    /\+1 needs you/
  );
  assert.ok(result.detailRoutes.includes('/octocode-status'));
});

test('automatic density obeys the 15% soft budget and six-row hard maximum', () => {
  for (const [height, expected] of [
    [10, 1],
    [20, 3],
    [40, 6],
    [80, 6],
  ] as const) {
    const result = selectStatusRows(
      snapshot({
        attention: [
          {
            id: 'context',
            kind: 'context_pressure',
            priority: 'P3',
            severity: 'warning',
            actor: 'Context',
            reason: '96% used',
            requiredAction: 'Compact',
            detailRoute: '/configuration',
            createdAt: 10_000,
          },
        ],
        agents: Array.from({ length: 8 }, (_, index) => agent(index)),
      }),
      { width: 80, height, density: 'automatic' }
    );
    assert.equal(result.maxRows, expected, `height ${height}`);
    assert.ok(result.rows.length <= expected, `height ${height}`);
  }
});

test('one-row automatic footer exposes urgent context and its route without counting P4 chrome', () => {
  const result = selectStatusRows(
    snapshot({
      plan: undefined,
      session: { ...snapshot().session, contextPressure: 96 },
    }),
    {
      width: 28,
      height: 10,
      density: 'automatic',
      diagnostics: [
        {
          id: 'identity',
          priority: 'P4',
          segments: [{ text: 'model openai/gpt-5.6' }],
        },
      ],
    }
  );
  const text = result.rows[0]!.map(segment => segment.text).join(' ');
  assert.equal(result.rows.length, 1);
  assert.equal(result.maxRows, 1);
  assert.ok(result.omitted > 0);
  assert.match(text, /ctx 96%.*\/configuration/);
  assert.doesNotMatch(
    text,
    /hidden|more/i,
    'P4 identity chrome does not create a hidden-state badge'
  );
  assert.ok(result.detailRoutes.includes('/configuration'));
});

test('compact and expanded density override row targets without changing priority', () => {
  const source = snapshot({
    attention: [
      {
        id: 'input',
        kind: 'input',
        priority: 'P0',
        severity: 'warning',
        actor: 'Octocode',
        reason: 'Start plan?',
        requiredAction: 'Answer prompt',
        detailRoute: 'interaction',
        createdAt: 10_000,
      },
    ],
  });
  const compact = selectStatusRows(source, {
    width: 80,
    height: 80,
    density: 'compact',
  });
  const expanded = selectStatusRows(source, {
    width: 80,
    height: 80,
    density: 'expanded',
  });
  assert.equal(compact.maxRows, 2);
  assert.ok(expanded.maxRows > compact.maxRows);
  assert.equal(compact.rowIds[0], 'attention:input');
  assert.equal(expanded.rowIds[0], 'attention:input');
});

test('normal agent count is bounded while blocked and failed agents are individually identifiable', () => {
  const result = selectStatusRows(
    snapshot({
      agents: [
        ...Array.from({ length: 100 }, (_, index) => agent(index)),
        { ...agent(101, 'blocked'), label: 'atlas' },
        { ...agent(102, 'failed'), label: 'nova' },
      ],
    }),
    { width: 120, height: 40, density: 'automatic' }
  );
  const rowText = result.rows.map(row =>
    row.map(segment => segment.text).join(' ')
  );
  const text = rowText.join('\n');
  assert.ok(rowText.some(row => /blocked.*atlas.*\/octocode-inbox/i.test(row)));
  assert.ok(rowText.some(row => /failed.*nova.*\/octocode-inbox/i.test(row)));
  assert.ok(rowText.some(row => /running.*worker-0.*\/octocode-inbox/i.test(row)));
  assert.match(text, /\+98 agents/, 'overflow counts live workers with a route to the full roster');
  assert.ok(result.rows.length <= 6);
  assert.doesNotMatch(text, /worker-99/);
});

test('compact mode groups active outcome facts so worker attention stays visible', () => {
  const result = selectStatusRows(
    snapshot({
      agents: [
        { ...agent(1, 'blocked'), label: 'atlas' },
        { ...agent(2, 'failed'), label: 'nova' },
      ],
    }),
    { width: 80, height: 40, density: 'compact' }
  );
  const text = result.rows
    .map(row => row.map(segment => segment.text).join(' '))
    .join('\n');
  assert.ok(result.rows.length <= 2);
  assert.match(text, /blocked atlas/);
  assert.match(text, /failed nova/);
  assert.match(text, /\/octocode-inbox/);
});

test('P0 attention preempts diagnostics and always preserves an action route', () => {
  const result = selectStatusRows(
    snapshot({
      attention: [
        {
          id: 'permission',
          kind: 'authorization',
          priority: 'P0',
          severity: 'warning',
          actor: 'Octocode',
          reason: 'Permission required',
          requiredAction: 'Review command',
          detailRoute: '/configuration',
          createdAt: 10_000,
        },
      ],
    }),
    {
      width: 36,
      height: 10,
      density: 'automatic',
      diagnostics: [
        {
          id: 'model',
          priority: 'P4',
          segments: [{ text: 'model openai/gpt-5.6' }],
        },
      ],
    }
  );
  assert.equal(result.rows.length, 1);
  assert.equal(result.rowIds[0], 'attention:permission');
  assert.match(
    result.rows[0]!.map(segment => segment.text).join(' '),
    /Permission required.*\/configuration/
  );
});

test('dynamic progress reports state counts without a fixed denominator', () => {
  const source = snapshot({
    plan: {
      ...snapshot().plan!,
      progressMode: 'dynamic',
      dynamic: true,
      displayTotal: undefined,
      done: 3,
      active: 2,
      ready: 1,
      blocked: 2,
      total: 8,
    },
  });
  const result = selectStatusRows(source, {
    width: 120,
    height: 40,
    density: 'automatic',
  });
  const text = result.rows[result.rowIds.indexOf('plan:progress')]!.map(
    segment => segment.text
  ).join(' ');
  assert.match(text, /3 done.*2 active.*scope changing/);
  assert.match(text, /task 2 running: Implementing/);
  assert.doesNotMatch(text, /3\/8|%/);
});

test('heartbeat-only changes preserve selected row identities and height', () => {
  const before = selectStatusRows(snapshot(), {
    width: 80,
    height: 40,
    density: 'automatic',
  });
  const after = selectStatusRows(
    snapshot({
      observedAt: 11_000,
      session: {
        ...snapshot().session,
        elapsedMs: 10_000,
        observationTime: 11_000,
      },
    }),
    { width: 80, height: 40, density: 'automatic' }
  );
  assert.deepEqual(after.rowIds, before.rowIds);
  assert.equal(after.rows.length, before.rows.length);
});

test('agent summary is suppressed when all agents are blocked or failed', () => {
  const result = selectStatusRows(
    snapshot({
      agents: [
        {
          id: 'a1',
          label: 'atlas',
          state: 'blocked',
          pendingMessages: 0,
          updatedAt: 9_000,
        },
        {
          id: 'a2',
          label: 'nova',
          state: 'failed',
          pendingMessages: 0,
          updatedAt: 8_000,
        },
      ],
    }),
    { width: 120, height: 40, density: 'automatic' }
  );
  // Named rows should exist for each attention agent
  assert.ok(
    result.rowIds.some(id => id.startsWith('agent:')),
    'named attention rows present'
  );
  // Summary must be absent — no state breakdown beyond what the named rows already say
  assert.ok(
    !result.rowIds.includes('agents:summary'),
    'no redundant summary row'
  );
});

test('mixed workers show named live rows and omit expired settled workers', () => {
  const result = selectStatusRows(
    snapshot({
      agents: [
        {
          id: 'a1',
          label: 'atlas',
          state: 'blocked',
          pendingMessages: 0,
          updatedAt: 9_000,
        },
        {
          id: 'a2',
          label: 'nova',
          state: 'running',
          pendingMessages: 0,
          updatedAt: 8_500,
        },
        {
          id: 'a3',
          label: 'luna',
          state: 'done',
          pendingMessages: 0,
          updatedAt: -1,
        },
      ],
    }),
    { width: 120, height: 40, density: 'automatic' }
  );
  assert.ok(
    result.rowIds.includes('worker:a2'),
    'named live worker row present'
  );
  const workerRow = result.rows[result.rowIds.indexOf('worker:a2')]!;
  const text = workerRow.map(s => s.text).join(' ');
  assert.match(text, /running nova/);
  assert.doesNotMatch(result.rows.flat().map(s => s.text).join(' '), /luna|done/);
});

test('compact mode caps agent attention at 2 named workers and shows overflow count', () => {
  const result = selectStatusRows(
    snapshot({
      agents: [
        {
          id: 'a1',
          label: 'atlas',
          state: 'blocked',
          pendingMessages: 0,
          updatedAt: 9_500,
        },
        {
          id: 'a2',
          label: 'nova',
          state: 'blocked',
          pendingMessages: 0,
          updatedAt: 9_000,
        },
        {
          id: 'a3',
          label: 'luna',
          state: 'blocked',
          pendingMessages: 0,
          updatedAt: 8_500,
        },
        {
          id: 'a4',
          label: 'mercury',
          state: 'failed',
          pendingMessages: 0,
          updatedAt: 8_000,
        },
        {
          id: 'a5',
          label: 'venus',
          state: 'error',
          pendingMessages: 0,
          updatedAt: 7_500,
        },
      ],
    }),
    { width: 120, height: 40, density: 'compact' }
  );
  const attentionRow = result.rows[result.rowIds.indexOf('agents:attention')];
  assert.ok(attentionRow, 'compact attention row present');
  const text = attentionRow!.map(s => s.text).join(' ');
  // Only the two most-recently-updated workers are named
  assert.match(text, /blocked atlas/, 'first named worker');
  assert.match(text, /blocked nova/, 'second named worker');
  // Remaining 3 workers collapsed into overflow count
  assert.match(text, /\+3/, 'overflow count');
  // Third worker must NOT be named individually
  assert.doesNotMatch(text, /blocked luna/, 'third worker collapsed');
  assert.match(text, /\/octocode-inbox/, 'inbox route preserved');
});

test('selection is deterministic for every density regardless of attention input order', () => {
  const attention = [
    {
      id: 'b',
      kind: 'messages' as const,
      priority: 'P2' as const,
      severity: 'info' as const,
      actor: 'Messages',
      reason: '2 pending',
      detailRoute: '/octocode-inbox',
      createdAt: 9_000,
    },
    {
      id: 'a',
      kind: 'input' as const,
      priority: 'P0' as const,
      severity: 'warning' as const,
      actor: 'Octocode',
      reason: 'Answer',
      detailRoute: 'interaction',
      createdAt: 10_000,
    },
  ];
  for (const density of [
    'automatic',
    'compact',
    'expanded',
  ] as StatusDensity[]) {
    const normal = selectStatusRows(snapshot({ attention }), {
      width: 80,
      height: 40,
      density,
    });
    const reversed = selectStatusRows(
      snapshot({ attention: [...attention].reverse() }),
      { width: 80, height: 40, density }
    );
    assert.deepEqual(reversed.rowIds, normal.rowIds);
  }
});
