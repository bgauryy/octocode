import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  ACTION_VALUE_PREFIX,
  CMD_VALUE_PREFIX,
  buildPaletteItems,
  dispatchPaletteSelection,
  openCommandPalette,
  registerCommandPalette,
  type PaletteAction,
  type PaletteCommand,
} from '../src/tools/command-palette.js';
import type { PiCommandContext, PiContext, PiInstance } from '../src/types.js';

// ─── Fakes ───────────────────────────────────────────────────────────────────

interface FakePi {
  sent: Array<{ text: string; opts?: { deliverAs?: string; expandPromptTemplates?: boolean } }>;
  commands: Map<string, { description: string; handler(args: string, ctx: PiCommandContext): Promise<void> }>;
  shortcuts: Map<string, { description: string; handler(ctx: PiContext): Promise<void> }>;
  shortcutThrows?: boolean;
  pi: PiInstance;
}

function makeFakePi(opts: { shortcutThrows?: boolean; getCommands?: () => Array<{ name: string; description?: string }> } = {}): FakePi {
  const state: FakePi = {
    sent: [],
    commands: new Map(),
    shortcuts: new Map(),
    shortcutThrows: opts.shortcutThrows,
    pi: undefined as unknown as PiInstance,
  };
  state.pi = {
    sendUserMessage: (text: string, o?: { deliverAs?: string; expandPromptTemplates?: boolean }) => {
      state.sent.push({ text, opts: o });
    },
    registerCommand: (name: string, def: unknown) => {
      state.commands.set(name, def as { description: string; handler(args: string, ctx: PiCommandContext): Promise<void> });
    },
    registerShortcut: (key: string, def: unknown) => {
      if (state.shortcutThrows) throw new Error('shortcut conflict (restrictOverride)');
      state.shortcuts.set(key, def as { description: string; handler(ctx: PiContext): Promise<void> });
    },
    getCommands: opts.getCommands,
  } as unknown as PiInstance;
  return state;
}

function makeCtx(overrides: Record<string, unknown> = {}): PiCommandContext {
  return {
    hasUI: true,
    // Real TUI contexts carry mode:'tui'; the overlay helper requires it.
    mode: 'tui',
    ui: {},
    ...overrides,
  } as unknown as PiCommandContext;
}

// ─── buildPaletteItems ───────────────────────────────────────────────────────

test('buildPaletteItems shapes cmd: and action: items with labels and descriptions', () => {
  const commands: PaletteCommand[] = [
    { name: 'octocode-status', description: 'Show status' },
    { name: 'octocode-theme', description: 'Switch theme', takesArgs: true },
  ];
  const actions: PaletteAction[] = [
    { id: 'inbox', label: 'Open inbox', description: 'Show agent inbox', run: () => {} },
  ];

  const items = buildPaletteItems({ commands, actions });

  const inbox = items.find((i) => i.value === `${ACTION_VALUE_PREFIX}inbox`)!;
  assert.equal(inbox.label, 'Open inbox');
  assert.equal(inbox.description, 'Show agent inbox');

  const status = items.find((i) => i.value === `${CMD_VALUE_PREFIX}octocode-status`)!;
  assert.equal(status.label, '/octocode-status');
  assert.equal(status.description, 'Show status');

  const theme = items.find((i) => i.value === `${CMD_VALUE_PREFIX}octocode-theme`)!;
  assert.equal(theme.label, '/octocode-theme');
  assert.match(theme.description ?? '', /prompts for args/);
});

test('buildPaletteItems pre-sorts: actions first, then commands, each alphabetical', () => {
  const items = buildPaletteItems({
    commands: [
      { name: 'zebra' },
      { name: 'alpha' },
    ],
    actions: [
      { id: 'w', label: 'Zulu action', run: () => {} },
      { id: 'a', label: 'Alpha action', run: () => {} },
    ],
  });
  assert.deepEqual(
    items.map((i) => i.value),
    ['action:a', 'action:w', 'cmd:alpha', 'cmd:zebra'],
  );
});

test('buildPaletteItems returns an empty list for empty deps', () => {
  assert.deepEqual(buildPaletteItems({}), []);
});

// ─── dispatchPaletteSelection ────────────────────────────────────────────────

test('dispatch: no-arg command is sent via sendUserMessage with deliverAs followUp and command expansion', async () => {
  const fake = makeFakePi();
  const handled = await dispatchPaletteSelection(
    'cmd:octocode-status',
    fake.pi,
    makeCtx(),
    { commands: [{ name: 'octocode-status' }] },
  );
  assert.equal(handled, true);
  assert.deepEqual(fake.sent, [{ text: '/octocode-status', opts: { deliverAs: 'followUp', expandPromptTemplates: true } }]);
});

test('dispatch: arg-taking command prefills the editor instead of sending', async () => {
  const fake = makeFakePi();
  const setTexts: string[] = [];
  const ctx = makeCtx({ ui: { setEditorText: (t: string) => setTexts.push(t) } });

  const handled = await dispatchPaletteSelection(
    'cmd:octocode-theme',
    fake.pi,
    ctx,
    { commands: [{ name: 'octocode-theme', takesArgs: true }] },
  );
  assert.equal(handled, true);
  assert.deepEqual(setTexts, ['/octocode-theme ']);
  assert.deepEqual(fake.sent, []);
});

test('dispatch: unknown command name still sends as followUp with command expansion (safe default)', async () => {
  const fake = makeFakePi();
  const handled = await dispatchPaletteSelection('cmd:mystery', fake.pi, makeCtx(), {});
  assert.equal(handled, true);
  assert.deepEqual(fake.sent, [{ text: '/mystery', opts: { deliverAs: 'followUp', expandPromptTemplates: true } }]);
});

test('dispatch: action invokes the injected handler with the ctx', async () => {
  const fake = makeFakePi();
  const ctx = makeCtx();
  let ranWith: unknown;
  const handled = await dispatchPaletteSelection('action:inbox', fake.pi, ctx, {
    actions: [{ id: 'inbox', label: 'Open inbox', run: (c) => { ranWith = c; } }],
  });
  assert.equal(handled, true);
  assert.equal(ranWith, ctx);
  assert.deepEqual(fake.sent, []);
});

test('dispatch: unknown action id and unknown scheme are not handled', async () => {
  const fake = makeFakePi();
  assert.equal(await dispatchPaletteSelection('action:nope', fake.pi, makeCtx(), { actions: [] }), false);
  assert.equal(await dispatchPaletteSelection('bogus:thing', fake.pi, makeCtx(), {}), false);
  assert.equal(await dispatchPaletteSelection('cmd:', fake.pi, makeCtx(), {}), false);
  assert.deepEqual(fake.sent, []);
});

// ─── registerCommandPalette ──────────────────────────────────────────────────

test('registerCommandPalette registers /octocode-palette and the default ctrl+shift+k shortcut', () => {
  const fake = makeFakePi();
  const reg = registerCommandPalette(fake.pi, { env: {} as NodeJS.ProcessEnv });
  assert.equal(reg.shortcut, 'ctrl+shift+k');
  assert.ok(fake.commands.has('octocode-palette'));
  assert.ok(fake.shortcuts.has('ctrl+shift+k'));
  assert.match(fake.commands.get('octocode-palette')!.description, /palette/i);
});

test('registerCommandPalette honors the OCTOCODE_PALETTE_KEY env override', () => {
  const fake = makeFakePi();
  const reg = registerCommandPalette(fake.pi, {
    env: { OCTOCODE_PALETTE_KEY: 'ctrl+p' } as NodeJS.ProcessEnv,
  });
  assert.equal(reg.shortcut, 'ctrl+p');
  assert.ok(fake.shortcuts.has('ctrl+p'));
  assert.equal(fake.shortcuts.has('ctrl+shift+k'), false);
});

test('registerCommandPalette degrades to command-only when registerShortcut throws', () => {
  const fake = makeFakePi({ shortcutThrows: true });
  const reg = registerCommandPalette(fake.pi, { env: {} as NodeJS.ProcessEnv });
  assert.equal(reg.shortcut, undefined);
  assert.ok(fake.commands.has('octocode-palette'));
  assert.equal(fake.shortcuts.size, 0);
});

test('registerCommandPalette survives a host without registerShortcut at all', () => {
  const fake = makeFakePi();
  (fake.pi as unknown as Record<string, unknown>)['registerShortcut'] = undefined;
  const reg = registerCommandPalette(fake.pi, { env: {} as NodeJS.ProcessEnv });
  // No shortcut API on this host → honest command-only registration.
  assert.equal(reg.shortcut, undefined);
  assert.ok(fake.commands.has('octocode-palette'));
});

// ─── openCommandPalette end-to-end (fake overlay) ────────────────────────────

test('palette command handler opens the overlay and dispatches the picked command', async () => {
  const fake = makeFakePi({
    getCommands: () => [{ name: 'octocode-status', description: 'Show status' }],
  });
  registerCommandPalette(fake.pi, { env: {} as NodeJS.ProcessEnv });

  const ctx = makeCtx({
    ui: {
      // runSelectOverlay funnels through ctx.ui.custom — resolve a selection directly.
      custom: async () => 'cmd:octocode-status',
    },
  });
  await fake.commands.get('octocode-palette')!.handler('', ctx);
  assert.deepEqual(fake.sent, [{ text: '/octocode-status', opts: { deliverAs: 'followUp', expandPromptTemplates: true } }]);
});

test('openCommandPalette merges injected commands over pi.getCommands and honors takesArgs', async () => {
  const fake = makeFakePi({
    getCommands: () => [{ name: 'octocode-theme', description: 'from discovery' }],
  });
  const setTexts: string[] = [];
  const ctx = makeCtx({
    ui: {
      custom: async () => 'cmd:octocode-theme',
      setEditorText: (t: string) => setTexts.push(t),
    },
  });
  await openCommandPalette(fake.pi, ctx, {
    commands: [{ name: 'octocode-theme', description: 'Switch theme', takesArgs: true }],
  });
  assert.deepEqual(setTexts, ['/octocode-theme ']);
  assert.deepEqual(fake.sent, []);
});

test('openCommandPalette is a no-op without UI and on overlay cancel', async () => {
  const fake = makeFakePi({ getCommands: () => [{ name: 'x' }] });

  await openCommandPalette(fake.pi, { hasUI: false } as unknown as PiContext, {});
  await openCommandPalette(fake.pi, undefined, {});

  const cancelCtx = makeCtx({ ui: { custom: async () => null } });
  await openCommandPalette(fake.pi, cancelCtx, {});
  assert.deepEqual(fake.sent, []);
});
