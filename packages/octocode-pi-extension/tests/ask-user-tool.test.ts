import assert from 'node:assert/strict';
import { test } from 'vitest';
import { Type } from 'typebox';
import { visibleWidth } from '@earendil-works/pi-tui';
import { registerAskUserTool, runAskPrompt } from '../src/tools/ask-user-tool.js';
import { configureInteractionBrokerRoute, setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import type { PiContext, ToolDefinition } from '../src/types.js';

function loadTool(): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerAskUserTool(pi, Type, new Set<string>(), (p, names, def) => {
    names.add(def.name);
    p.registerTool?.(def);
  });
  const tool = tools.get('askUser')!;
  const execute = tool.execute.bind(tool);
  tool.execute = (id, params, signal, onUpdate, ctx) => {
    const envelope = Array.isArray(params['queries'])
      ? params
      : { queries: [{ reasoning: 'resolve a genuine test decision', ...params }] };
    return execute(id, envelope, signal, onUpdate, ctx);
  };
  return tool;
}

// Inline harness: askUser renders via ctx.ui.custom(builder) with NO overlay
// options, so it appears inline in the message flow. The mock invokes the
// factory synchronously, captures the component + any opts (expected undefined),
// and resolves the custom() promise when the factory calls done().
function overlayCtx() {
  let component: { render(w: number): string[]; handleInput(d: string): void } | undefined;
  let overlayOpts: { overlay?: boolean } | undefined;
  const pendingInputs: string[] = [];
  const tui = { requestRender: () => {} };
  const ctx = {
    hasUI: true,
    mode: 'tui',
    ui: {
      custom: (
        factory: (tui: unknown, theme: unknown, kb: unknown, done: (v: unknown) => void) => { render(w: number): string[]; handleInput(d: string): void },
        opts?: { overlay?: boolean },
      ) =>
        new Promise((resolve) => {
          overlayOpts = opts;
          component = factory(tui, undefined, undefined, (v) => resolve(v));
          for (const input of pendingInputs.splice(0)) component.handleInput(input);
        }),
    },
  } as unknown as PiContext;
  configureInteractionBrokerRoute(ctx, true);
  return {
    ctx,
    send: (data: string) => {
      if (component) component.handleInput(data);
      else pendingInputs.push(data);
    },
    render: (w = 100) => component?.render(w) ?? [],
    overlayOpts: () => overlayOpts,
    // Simulate the TUI granting focus (Focusable.focused = true).
    focus: () => { if (component) (component as { focused?: boolean }).focused = true; },
  };
}

test('askUser registration teaches option lists, concise labels, and inline fallback', () => {
  const tool = loadTool();

  assert.equal(tool.name, 'askUser');
  assert.match(tool.description, /keyboard-navigable list/);
  assert.match(tool.description, /Discuss or type your own answer/);
  assert.match(tool.description, /pros\[\] and cons\[\]/);
  assert.match(tool.description, /recommended:true/);
  assert.match(tool.description, /non-interactive hosts/);
  assert.match(tool.promptGuidelines?.join('\n') ?? '', /reply 1\/2\/3/);
  assert.match(tool.promptGuidelines?.join('\n') ?? '', /recommended:true/);
  assert.match(tool.promptGuidelines?.join('\n') ?? '', /Discuss or type your own answer/);
  assert.match(tool.promptGuidelines?.join('\n') ?? '', /fall back to asking the question directly/);
  const schema = tool.parameters as {
    properties?: { queries?: { items?: { properties?: Record<string, unknown>; required?: string[] } } };
    required?: string[];
  };
      assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType']);
  assert.ok(schema.required?.includes('queries'));
  assert.ok(schema.properties?.queries?.items?.properties?.['reasoning']);
  assert.ok(schema.properties?.queries?.items?.properties?.['timeoutMs']);
  assert.ok(schema.properties?.queries?.items?.required?.includes('reasoning'));
});

test('askUser processes multiple noninteractive questions in source order', async () => {
  const tool = loadTool();
  const result = await tool.execute('batch', {
    queries: [
      { reasoning: 'resolve first decision', question: 'First?' },
      { reasoning: 'resolve second decision', question: 'Second?' },
    ],
  }, undefined, undefined, { hasUI: false, mode: 'rpc' } as unknown as PiContext);
  assert.match((result.content[0] as { text: string }).text, /2 queries succeeded/);
  assert.equal((result.details as { results: unknown[] }).results.length, 2);
});

test('askUser preflights every question before opening an earlier prompt', async () => {
  const tool = loadTool();
  let customCalled = false;
  const ctx = {
    hasUI: true,
    mode: 'tui',
    ui: { custom: async () => { customCalled = true; return undefined; } },
  } as unknown as PiContext;
  await assert.rejects(tool.execute('batch-invalid', {
    queries: [
      { reasoning: 'ask valid first question', question: 'First?' },
      { reasoning: 'invalid blank question', question: '   ' },
    ],
  }, undefined, undefined, ctx), /queries\[1\] failed preflight/);
  assert.equal(customCalled, false);
});

test('askUser creates a pending RPC interaction even though hasUI is true and custom exists', async () => {
  const tool = loadTool();
  let customCalled = false;
  const ctx = {
    hasUI: true,
    mode: 'rpc',
    ui: {
      // In RPC pi exposes custom() but it returns undefined; askUser must NOT
      // treat this as interactive (would resolve as a bogus cancellation).
      custom: async () => { customCalled = true; return undefined; },
    },
  } as unknown as PiContext;
  configureInteractionBrokerRoute(ctx, true);

  const result = await tool.execute('id', { question: 'Ship it?', options: ['yes', 'no'] }, undefined, undefined, ctx);

  assert.equal(customCalled, false, 'custom() must not be called outside tui mode');
  assert.match((result.content[0] as { text: string }).text, /Structured interaction pending \(mode=rpc/);
  assert.equal((result.details as { status: string }).status, 'pending');
});

test('askUser fails closed when a headless host has no durable answer route', async () => {
  const tool = loadTool();
  let created = 0;
  setInteractionStoreFactoryForTests(() => ({
    createInteraction: () => { created += 1; },
    answerInteraction: () => undefined,
    close: () => undefined,
  }));
  try {
    const ctx = {
      cwd: '/tmp/ask-no-host-route',
      hasUI: false,
      mode: 'rpc',
    } as unknown as PiContext;
    configureInteractionBrokerRoute(ctx, false);
    const result = await tool.execute('id', { question: 'Ship it?', options: ['yes', 'no'] }, undefined, undefined, ctx);

    assert.equal(created, 0, 'an unreachable durable interaction must not be persisted');
    assert.equal((result.details as { status: string }).status, 'unavailable');
    assert.match((result.content[0] as { text: string }).text, /no durable .*answer route/i);
    assert.match((result.content[0] as { text: string }).text, /ask the user inline/i);
  } finally {
    setInteractionStoreFactoryForTests();
  }
});

test('durable answer routes are isolated between mixed hosts in one process', async () => {
  const tool = loadTool();
  const supported = { cwd: '/tmp/ask-supported-host', hasUI: false, mode: 'rpc' } as unknown as PiContext;
  const unsupported = { cwd: '/tmp/ask-unsupported-host', hasUI: false, mode: 'rpc' } as unknown as PiContext;
  configureInteractionBrokerRoute(supported, true);
  configureInteractionBrokerRoute(unsupported, false);

  const pending = await tool.execute('supported', { question: 'Supported?' }, undefined, undefined, supported);
  const unavailable = await tool.execute('unsupported', { question: 'Unsupported?' }, undefined, undefined, unsupported);

  assert.equal((pending.details as { status: string }).status, 'pending');
  assert.equal((unavailable.details as { status: string }).status, 'unavailable');
});

test('runAskPrompt returns the durable correlation when an internal RPC prompt cannot render', async () => {
  const created: Array<{ interactionId: string; correlationId: string }> = [];
  setInteractionStoreFactoryForTests(() => ({
    createInteraction: (request: { interactionId: string; correlationId: string }) => { created.push(request); },
    answerInteraction: () => undefined,
    close: () => undefined,
  }));
  try {
    const ctx = {
      cwd: '/tmp/ask-internal-rpc',
      mode: 'rpc',
      hasUI: true,
      ui: { custom: async () => undefined },
    } as unknown as PiContext;
    configureInteractionBrokerRoute(ctx, true);
    const outcome = await runAskPrompt(ctx, {
      question: 'Approve the plan?',
      options: [{ value: 'approve', label: 'Approve' }],
    });

    assert.equal(outcome?.status, 'pending');
    assert.equal(outcome?.interaction?.interactionId, created[0]?.interactionId);
    assert.equal(outcome?.interaction?.correlationId, created[0]?.correlationId);
  } finally {
    setInteractionStoreFactoryForTests();
  }
});

test('askUser emits CURSOR_MARKER at the caret in text mode when focused (IME positioning)', async () => {
  const tool = loadTool();
  const { ctx, send, render, focus } = overlayCtx();
  const pending = tool.execute('id', { question: 'Name?' }, undefined, undefined, ctx);
  focus(); // TUI grants focus → Focusable.focused = true
  send('Gu');
  const lines = render(80);
  const caretLine = lines.find((l) => l.includes('\u203a'))!;
  // CURSOR_MARKER (APC escape) is appended after the typed text for the hardware cursor.
  assert.ok(caretLine.includes('\u001b_pi:c\u0007'), 'focused text input must emit CURSOR_MARKER');
  assert.ok(caretLine.trimEnd().endsWith('\u0007') || caretLine.includes('Gu'), 'marker sits at the caret after typed text');
  send('\r');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'text', value: 'Gu' });
});

test('askUser accepts bracketed paste in free-text mode', async () => {
  const tool = loadTool();
  const { ctx, send, render, focus } = overlayCtx();
  const pending = tool.execute('id', { question: 'What should we do?' }, undefined, undefined, ctx);

  focus();
  send('\x1b[200~paste this answer\x1b[201~');
  assert.match(render(100).join('\n'), /paste this answer/);
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'text', value: 'paste this answer' });
});

test('askUser free-text mode supports cursor editing through Pi Input', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();
  const pending = tool.execute('id', { question: 'Name?' }, undefined, undefined, ctx);

  send('ac');
  send('\x1b[D');
  send('b');
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'text', value: 'abc' });
});

test('askUser centers a bounded responsive card without exceeding the terminal', async () => {
  const tool = loadTool();
  const wide = overlayCtx();
  const pendingWide = tool.execute('id', { question: 'Choose?', options: ['safe', 'fast'] }, undefined, undefined, wide.ctx);
  const wideLines = wide.render(160);

  const wideHeader = wideLines[0]!.replace(/\x1b\[[0-9;]*m/g, '');
  const wideStart = wideHeader.indexOf('╭');
  assert.equal(wideStart, 36, 'an 88-column card is centered in a 160-column terminal');
  assert.equal(visibleWidth(wideHeader.slice(wideStart)), 88, 'wide terminals cap the reading measure at 88 columns');
  assert.ok(wideLines.every((line) => visibleWidth(line) <= 160), 'wide rendering stays within the terminal');
  wide.send('\x1b');
  await pendingWide;

  for (const [terminalWidth, expectedStart, expectedCardWidth] of [
    [52, 2, 48],
    [80, 4, 72],
    [100, 14, 72],
    [120, 17, 86],
  ] as const) {
    const sized = overlayCtx();
    const pendingSized = tool.execute('id', { question: 'Choose?', options: ['safe', 'fast'] }, undefined, undefined, sized.ctx);
    const header = sized.render(terminalWidth)[0]!.replace(/\x1b\[[0-9;]*m/g, '');
    const start = header.indexOf('╭');
    assert.equal(start, expectedStart, `${terminalWidth}-column terminal centers the decision card`);
    assert.equal(visibleWidth(header.slice(start)), expectedCardWidth, `${terminalWidth}-column card uses the responsive reading measure`);
    sized.send('\x1b');
    await pendingSized;
  }

  const narrow = overlayCtx();
  const pendingNarrow = tool.execute('id', { question: 'Choose?', options: ['safe', 'fast'] }, undefined, undefined, narrow.ctx);
  assert.ok(narrow.render(36).every((line) => visibleWidth(line) <= 36), 'narrow rendering never overflows');
  narrow.send('\x1b');
  await pendingNarrow;
});

test('askUser validates that a non-empty question is required', async () => {
  const tool = loadTool();
  const result = await tool.execute('id', { question: '   ' });

  assert.equal(result.isError, true);
  assert.match((result.content[0] as { text: string }).text, /question is required/);
});

test('askUser returns a structured pending interaction when no interactive UI is available', async () => {
  const tool = loadTool();
  setInteractionStoreFactoryForTests(() => ({ createInteraction: () => undefined, answerInteraction: () => undefined, close: () => undefined }));
  try {
    const ctx = { mode: 'rpc', hasUI: false } as PiContext;
    configureInteractionBrokerRoute(ctx, true);
    const result = await tool.execute(
    'id',
    {
      question: 'Choose a strategy?',
      options: [
        { value: 'safe', label: 'Safe', description: 'Recommended' },
        { value: 'fast', label: 'Fast' },
      ],
    },
    undefined,
    undefined,
    ctx,
    );

    assert.equal(result.isError, undefined);
    assert.match((result.content[0] as { text: string }).text, /Structured interaction pending \(mode=rpc/);
    assert.match((result.content[0] as { text: string }).text, /submit one matching answer through the InteractionBroker adapter/);
    assert.match((result.content[0] as { text: string }).text, /drain its durable continuation/);
    assert.match((result.content[0] as { text: string }).text, /Safe, Fast/);
    const details = result.details as {
      status: string;
      mode: string;
      interaction: { correlationId: string; question: string };
      continuation: { version: number; adapter: string; resumeOn: string[] };
    };
    assert.equal(details.status, 'pending');
    assert.equal(details.mode, 'rpc');
    assert.equal(details.interaction.question, 'Choose a strategy?');
    assert.match(details.interaction.correlationId, /^correlation_/);
    assert.deepEqual(details.continuation, {
      version: 1,
      adapter: 'interaction-broker',
      resumeOn: ['answer', 'session_start'],
    });
  } finally {
    setInteractionStoreFactoryForTests();
  }
});

test('askUser uses the custom overlay and never Pi native select', async () => {
  const tool = loadTool();
  const selectCalls: Array<{ title: string; items: string[] }> = [];
  const { ctx, send, overlayOpts } = overlayCtx();
  (ctx as unknown as { ui: Record<string, unknown> }).ui.select = async (title: string, items: string[]) => {
    selectCalls.push({ title, items });
    return 'safe';
  };

  const pending = tool.execute(
    'id',
    {
      question: 'Choose a strategy?',
      options: ['safe', 'fast'],
    },
    undefined,
    undefined,
    ctx,
  );
  send('\r');
  const result = await pending;

  assert.deepEqual(selectCalls, [], 'askUser must not call Pi native select (it uses the inline prompt)');
  assert.notEqual(overlayOpts()?.overlay, true, 'askUser renders inline in the message flow, not as an overlay');
  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'safe' });
});

test('askUser renders choices inline in the message flow, not as a floating overlay', async () => {
  const tool = loadTool();
  const { ctx, render, send, overlayOpts } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Choose a strategy?',
      options: ['safe', 'fast'],
    },
    undefined,
    undefined,
    ctx,
  );

  const opts = overlayOpts() as { overlay?: boolean; overlayOptions?: { anchor?: string } } | undefined;
  assert.notEqual(opts?.overlay, true, 'askUser must render inline (non-overlay) in the message flow');
  assert.equal(opts?.overlayOptions, undefined, 'inline prompt passes no overlay positioning options');
  const lines = render(100);
  assert.match(lines.join('\n'), /Input needed/);
  assert.match(lines.join('\n'), /Choose a strategy\?/);
  // Discuss / free-text row appears AFTER the listed options.
  assert.match(lines.join('\n'), /Discuss or type your own answer/);
  // The decision hierarchy is carried by the heading ("Input needed") and whitespace.
  const plain = lines.join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /Input needed/);
  send('\r');
  const result = await pending;

  assert.match((result.content[0] as { text: string }).text, /User selected: safe/);
  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'safe' });
});

test('askUser option picker allows bracketed paste in the custom free-text answer', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Choose a strategy?',
      options: ['safe', 'fast'],
    },
    undefined,
    undefined,
    ctx,
  );

  send('\x1b[B');
  send('\x1b[B');
  send('\r');
  send('[200~custom plan[201~');
  send('\r');
  const result = await pending;

  assert.match((result.content[0] as { text: string }).text, /User answered: custom plan/);
  assert.deepEqual(result.details, { status: 'text', value: 'custom plan' });
});

test('askUser routes described choices through the custom overlay', async () => {
  const tool = loadTool();
  const { ctx, send, overlayOpts } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Choose a strategy?',
      options: [
        { value: 'safe', label: 'Safe', description: 'Recommended' },
        { value: 'fast', label: 'Fast' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );

  assert.notEqual(overlayOpts()?.overlay, true, 'described choices render inline in the message flow');
  send('\r');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'Safe' });
});

test('askUser never calls Pi ui.input (it uses the overlay)', async () => {
  const tool = loadTool();
  const prompts: string[] = [];
  const { ctx, send } = overlayCtx();
  (ctx as unknown as { ui: Record<string, unknown> }).ui.input = async (question: string) => {
    prompts.push(question);
    return 'typed answer';
  };

  const pending = tool.execute('id', { question: 'What should we do?' }, undefined, undefined, ctx);
  send('ship it');
  send('\r');
  const result = await pending;

  assert.deepEqual(prompts, [], 'askUser must not call Pi input (it uses the overlay)');
  assert.match((result.content[0] as { text: string }).text, /User answered: ship it/);
  assert.deepEqual(result.details, { status: 'text', value: 'ship it' });
});

test('askUser echoes the question in the selected result (durable context after compaction)', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();
  const pending = tool.execute(
    'id',
    { question: 'Choose a strategy?', options: [{ value: 'safe', label: 'Safe' }] },
    undefined,
    undefined,
    ctx,
  );
  send('\r');
  const result = await pending;
  assert.match((result.content[0] as { text: string }).text, /Choose a strategy\?/);
  assert.match((result.content[0] as { text: string }).text, /User selected: Safe/);
});

test('askUser echoes the question in free-text and cancelled results', async () => {
  const tool = loadTool();
  const answeredHarness = overlayCtx();
  const answeredPending = tool.execute(
    'id',
    { question: 'What should we do?' },
    undefined,
    undefined,
    answeredHarness.ctx,
  );
  answeredHarness.send('ship it');
  answeredHarness.send('\r');
  const answered = await answeredPending;
  assert.match((answered.content[0] as { text: string }).text, /What should we do\?/);
  assert.match((answered.content[0] as { text: string }).text, /User answered: ship it/);

  const cancelledHarness = overlayCtx();
  const cancelledPending = tool.execute(
    'id',
    { question: 'Pick one?', options: [{ value: 'a' }] },
    undefined,
    undefined,
    cancelledHarness.ctx,
  );
  cancelledHarness.send('\x1b');
  const cancelled = await cancelledPending;
  assert.match((cancelled.content[0] as { text: string }).text, /Pick one\?/);
  assert.match((cancelled.content[0] as { text: string }).text, /cancelled/i);
});

test('askUser schema gains preview, disabled options, multiSelect, min/max, and field validation additively', () => {
  const tool = loadTool();
  const params = tool.parameters as {
    properties: { queries: { items: { properties: Record<string, { items?: { properties?: Record<string, unknown> } }> } } };
  };
  const queryProps = params.properties.queries.items.properties;

  assert.ok(queryProps['multiSelect'], 'multiSelect input exists');
  assert.ok(queryProps['min'], 'min input exists');
  assert.ok(queryProps['max'], 'max input exists');
  assert.ok(queryProps['options']!.items?.properties?.['preview'], 'options gain preview');
  assert.ok(queryProps['options']!.items?.properties?.['disabled'], 'options gain disabled');
  const fieldProps = queryProps['fields']!.items?.properties ?? {};
  assert.deepEqual(Object.keys(fieldProps).sort(), ['label', 'maxLength', 'minLength', 'name', 'pattern', 'placeholder', 'required']);
});

test('askUser progressively discloses focused descriptions and trade-offs and lands on the recommendation', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Which approach?',
      options: [
        { value: 'risky', label: 'Aggressive cut', description: 'Removes the compatibility path.', pros: ['small diff'], cons: ['thins the safety net'] },
        { value: 'safe', label: 'Leave it', description: 'Keeps the supported behavior unchanged.', recommended: true, pros: ['no risk', 'load-bearing'], cons: ['no line-count win'] },
      ],
    },
    undefined,
    undefined,
    ctx,
  );

  const before = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(before, /Leave it \[recommended\]/);
  // Focused-row contract: description is always visible for all rows; pros/cons only appear on focused row.
  assert.match(before, /Keeps the supported behavior unchanged/);
  // Non-focused rows now always show their description (dim), so Aggressive cut's description IS visible.
  assert.match(before, /Removes the compatibility path/);
  assert.doesNotMatch(before, /✓ small diff/, 'non-focused Aggressive cut does not show pros');
  assert.doesNotMatch(before, /✗ thins the safety net/, 'non-focused Aggressive cut does not show cons');
  assert.match(before, /✓ no risk/);
  assert.match(before, /✓ load-bearing/);
  assert.match(before, /✗ no line-count win/);

  send('\x1b[A');
  const after = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  // After moving focus to Aggressive cut: its detail appears; Leave it's description stays visible (always-on).
  assert.match(after, /Removes the compatibility path/);
  assert.match(after, /Keeps the supported behavior unchanged/);
  assert.match(after, /✓ small diff/);
  assert.match(after, /✗ thins the safety net/);

  send('\x1b[B');
  send('\r');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'Leave it' });
});

test('askUser preserves complete decision content at narrow widths without clipping or detail caps', async () => {
  const tool = loadTool();
  const { ctx, render, send } = overlayCtx();
  const question = 'Which complete rollout strategy should remain readable in a narrow terminal before implementation starts?';
  const label = 'Keep the compatibility adapter until every persisted session has migrated safely';
  const description = 'This deliberately long description explains the session, data, browser, and agent consequences without dropping its final words.';
  const pros = [
    'preserves restart safety for sessions created by earlier releases',
    'keeps browser and terminal behavior aligned during rollout',
    'allows deterministic rollback after a failed verification receipt',
  ];
  const cons = [
    'requires one additional compatibility checkpoint before cleanup',
    'keeps a temporary adapter visible for another release window',
    'adds a final migration audit before deletion is authorized',
  ];
  const preview = [
    'preview line one: read the durable session',
    'preview line two: validate the accepted revision',
    'preview line three: resume the exact continuation',
    'preview line four: verify browser and terminal parity',
    'preview line five: remove the adapter only after success',
  ].join('\n');

  const pending = tool.execute('id', {
    question,
    options: [{ value: 'safe', label, description, pros, cons, preview, recommended: true }],
  }, undefined, undefined, ctx);

  const lines = render(30);
  const normalized = lines.join('\n')
    .replace(/\x1b\[[0-9;]*m/g, '')
    .replace(/[│╭╮╰╯─]/g, ' ')
    .replace(/\s+/g, ' ');
  assert.ok(lines.every((line) => visibleWidth(line) <= 30), 'every wrapped row remains terminal-width safe');
  for (const completeText of [question, label, description, ...pros, ...cons, ...preview.split('\n')]) {
    assert.ok(normalized.includes(completeText), `complete UI content remains visible: ${completeText}`);
  }
  assert.match(normalized, /recommended/);
  assert.match(normalized, /← back • ↑↓ • enter • esc/);

  send('\x1b');
  await pending;
});

test('askUser transcript renderers wrap complete questions and selected labels instead of truncating them', () => {
  const tool = loadTool();
  const question = 'Should the complete narrow transcript preserve this entire question for later review?';
  const label = 'Yes, preserve the complete selected option label across every wrapped transcript row';
  const render = (component: unknown, width: number): string[] =>
    (component as { render: (w: number) => string[] }).render(width);
  const normalize = (lines: string[]): string => lines.join(' ')
    .replace(/\x1b\[[0-9;]*m/g, '')
    .replace(/\s+/g, ' ');

  const callLines = render(tool.renderCall?.({ queries: [{ question, options: [{ value: 'yes', label }] }] }), 24);
  assert.ok(callLines.every((line) => visibleWidth(line) <= 24));
  assert.ok(normalize(callLines).includes(question));

  const resultLines = render(tool.renderResult?.({ content: [], details: { status: 'selected', label } }, { isPartial: false }), 24);
  assert.ok(resultLines.every((line) => visibleWidth(line) <= 24));
  assert.ok(normalize(resultLines).includes(label));
});

test('askUser schema exposes pros, cons, and recommended on options', () => {
  const tool = loadTool();
  const params = tool.parameters as {
    properties: { queries: { items: { properties: Record<string, { items?: { properties?: Record<string, unknown> } }> } } };
  };
  const optProps = params.properties.queries.items.properties['options']!.items?.properties ?? {};
  assert.ok(optProps['pros'], 'options gain pros');
  assert.ok(optProps['cons'], 'options gain cons');
  assert.ok(optProps['recommended'], 'options gain recommended');
  assert.ok(optProps['disabled'], 'options gain disabled');
  assert.ok(optProps['group'], 'options gain group');
});

test('askUser multiSelect returns multiSelected values through the overlay', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Pick strategies?',
      multiSelect: true,
      min: 1,
      max: 2,
      options: [
        { value: 'safe', label: 'Safe', preview: 'diff --git a b' },
        { value: 'fast', label: 'Fast' },
        { value: 'risky' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );
  send(' ');
  send('\x1b[B');
  send(' ');
  send('\r');
  const result = await pending;

  assert.match((result.content[0] as { text: string }).text, /Pick strategies\?/);
  assert.match((result.content[0] as { text: string }).text, /User selected 2 options: Safe, Fast/);
  assert.deepEqual(result.details, { status: 'multiSelected', values: ['safe', 'fast'] });
});

test('askUser multiSelect custom answer row bypasses min/max option validation', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Pick some?', multiSelect: true, min: 1, options: [{ value: 'a' }] },
    undefined,
    undefined,
    ctx,
  );
  send('\x1b[B');
  send('\r');
  send('something else');
  send('\r');
  const result = await pending;

  assert.match((result.content[0] as { text: string }).text, /User answered: something else/);
  assert.deepEqual(result.details, { status: 'text', value: 'something else' });
});

test('askUser multiSelect reports cancellation when the overlay is dismissed', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Pick some?', multiSelect: true, options: [{ value: 'a' }, { value: 'b' }] },
    undefined,
    undefined,
    ctx,
  );
  send('\x1b');
  const result = await pending;

  assert.match((result.content[0] as { text: string }).text, /Pick some\?/);
  assert.match((result.content[0] as { text: string }).text, /cancelled/i);
  assert.deepEqual(result.details, { status: 'cancelled' });
});

test('askUser previewed options route through the overlay, not the native selector', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Choose?', options: [{ value: 'safe', preview: 'preview body' }, { value: 'fast' }] },
    undefined,
    undefined,
    ctx,
  );
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'safe' });
});

test('askUser form collects fields in order via the overlay modal', async () => {
  const tool = loadTool();
  const { ctx, send, overlayOpts } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'New profile',
      fields: [
        { name: 'name', label: 'Full name' },
        { name: 'email', label: 'Email', placeholder: 'you@host' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );
  send('Guy');
  send('\r');
  send('guy@example.com');
  send('\r');
  const result = await pending;

  assert.notEqual(overlayOpts()?.overlay, true, 'form renders inline in the message flow');
  assert.match((result.content[0] as { text: string }).text, /New profile/);
  assert.match((result.content[0] as { text: string }).text, /name: Guy/);
  assert.match((result.content[0] as { text: string }).text, /email: guy@example.com/);
  assert.deepEqual(result.details, { status: 'form', values: { name: 'Guy', email: 'guy@example.com' } });
});

test('askUser form keeps focus on invalid fields until valid or escaped', async () => {
  const tool = loadTool();
  const harness = overlayCtx();
  const pending = tool.execute(
    'id',
    { question: 'Profile', fields: [{ name: 'name', label: 'Name', required: true, minLength: 3 }] },
    undefined,
    undefined,
    harness.ctx,
  );

  harness.send('\r');
  assert.match(harness.render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /Name is required/);
  harness.send('Al');
  harness.send('\r');
  assert.match(harness.render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /Name must be at least 3 characters/);
  harness.send('i');
  harness.send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'form', values: { name: 'Ali' } });
});

test('askUser form cancels when the user escapes any prompt', async () => {
  const tool = loadTool();
  const { ctx, send } = overlayCtx();
  const pending = tool.execute(
    'id',
    { question: 'Profile', fields: [{ name: 'name' }, { name: 'email' }] },
    undefined,
    undefined,
    ctx,
  );
  send('\x1b');
  const result = await pending;
  assert.match((result.content[0] as { text: string }).text, /cancelled/i);
  assert.deepEqual(result.details, { status: 'cancelled' });
});

test('askUser single-select digit keys pick the numbered option outright', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Choose?', options: ['alpha', 'beta', 'gamma'] },
    undefined,
    undefined,
    ctx,
  );
  const plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /①  alpha/, 'options use circle badge for quick-select');
  assert.match(plain, /②  beta/);
  send('2');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'selected', value: 'beta', label: 'beta' });
});

test('askUser multiSelect digit keys toggle and footer shows a live count', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Pick?', multiSelect: true, min: 1, options: ['a', 'b', 'c'] },
    undefined,
    undefined,
    ctx,
  );
  assert.match(render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /0 selected · min 1/);
  send('1');
  send('3');
  assert.match(render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /2 selected · min 1/);
  send('\r');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'multiSelected', values: ['a', 'c'] });
});

test('askUser windows long rich lists by complete option blocks with position and more-markers', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const options = Array.from({ length: 20 }, (_, i) => ({
    value: `opt-${String(i + 1).padStart(2, '0')}`,
    pros: [`pro-${String(i + 1).padStart(2, '0')}`],
    cons: [`con-${String(i + 1).padStart(2, '0')}`],
  }));
  const pending = tool.execute('id', { question: 'Long?', options }, undefined, undefined, ctx);

  const first = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(first, /opt-01/);
  assert.match(first, /pro-01/);
  assert.match(first, /con-01/);
  // Focused-row contract: opt-01 (focused) shows its detail; opt-02 visible without pros/cons.
  assert.doesNotMatch(first, /opt-02[\s\S]*pro-02/, 'non-focused rows show label only, without pros/cons');
  assert.match(first, /↓ \d+ more/, 'hidden tail advertised');
  assert.doesNotMatch(first, /opt-20/, 'blocks beyond the viewport are not painted');

  for (let i = 0; i < 15; i++) send('\x1b[B');
  const scrolled = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(scrolled, /↑ \d+ more/, 'hidden head advertised after scrolling');
  // opt-16 is focused so its detail (pro-16, con-16) IS shown.
  assert.match(scrolled, /opt-16[\s\S]*pro-16[\s\S]*con-16/);

  send('\x1b');
  const result = await pending;
  assert.deepEqual(result.details, { status: 'cancelled' });
});

test('askUser disabled options stay visible but cannot be selected', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Choose?',
      options: [
        { value: 'blocked', label: 'Blocked', disabled: 'needs auth' },
        { value: 'safe', label: 'Safe' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );

  let plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /Blocked \(needs auth\)/);
  send('\r');
  plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /"Blocked" is needs auth/);
  send('\x1b[B');
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'Safe' });
});

test('askUser renders grouped choices as non-selectable headings', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Choose?',
      options: [
        { value: 'safe', label: 'Safe', group: 'Recommended' },
        { value: 'fast', label: 'Fast', group: 'Risky' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );

  const plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /┌ Recommended/);
  assert.match(plain, /┌ Risky/);
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'Safe' });
});

test('askUser filters options with slash search and clears search with escape before cancellation', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    { question: 'Choose?', options: ['alpha', 'beta', 'gamma'] },
    undefined,
    undefined,
    ctx,
  );

  send('/');
  send('ga');
  let plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /\/ ga/);
  assert.match(plain, /gamma/);
  assert.doesNotMatch(plain, /alpha/);
  send('\x1b');
  plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /alpha/);
  send('/');
  send('zz');
  plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');
  assert.match(plain, /No matches/);
  send('\x1b');
  send('\x1b');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'cancelled' });
});

test('askUser multiSelect supports all and invert shortcuts while skipping disabled options', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute(
    'id',
    {
      question: 'Pick?',
      multiSelect: true,
      options: [
        { value: 'a', label: 'A' },
        { value: 'b', label: 'B', disabled: true },
        { value: 'c', label: 'C' },
      ],
    },
    undefined,
    undefined,
    ctx,
  );

  send('a');
  assert.match(render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /2 selected/);
  send('i');
  assert.match(render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, ''), /0 selected/);
  send('i');
  send('\r');
  const result = await pending;

  assert.deepEqual(result.details, { status: 'multiSelected', values: ['a', 'c'] });
});

test('askUser renders a final submitted state after completion', async () => {
  const tool = loadTool();
  const { ctx, send, render } = overlayCtx();

  const pending = tool.execute('id', { question: 'Choose?', options: ['safe'] }, undefined, undefined, ctx);
  send('\r');
  const result = await pending;
  const plain = render(100).join('\n').replace(/\x1b\[[0-9;]*m/g, '');

  assert.match(plain, /safe/);
  assert.match(plain, /submitted/);
  assert.deepEqual(result.details, { status: 'selected', value: 'safe', label: 'safe' });
});

test('askUser multiSelect and form degrade to inline hints without an interactive UI', async () => {
  const tool = loadTool();
  const printCtx = { mode: 'print', hasUI: false } as PiContext;
  const rpcCtx = { mode: 'rpc', hasUI: false } as PiContext;
  configureInteractionBrokerRoute(printCtx, true);
  configureInteractionBrokerRoute(rpcCtx, true);

  const multi = await tool.execute(
    'id',
    {
      question: 'Pick strategies?',
      multiSelect: true,
      options: [{ value: 'safe', label: 'Safe' }, { value: 'fast', label: 'Fast' }],
    },
    undefined,
    undefined,
    printCtx,
  );
  assert.match((multi.content[0] as { text: string }).text, /Structured interaction pending \(mode=print/);
  assert.match((multi.content[0] as { text: string }).text, /Safe, Fast/);
  assert.match((multi.content[0] as { text: string }).text, /may choose more than one/);
  assert.equal((multi.details as { status: string }).status, 'pending');

  const form = await tool.execute(
    'id',
    { question: 'New profile', fields: [{ name: 'name', label: 'Full name' }, { name: 'email' }] },
    undefined,
    undefined,
    rpcCtx,
  );
  assert.match((form.content[0] as { text: string }).text, /Collect these fields inline: Full name, email/);
  assert.equal((form.details as { status: string }).status, 'pending');
});
