import assert from 'node:assert/strict';
import { test } from 'vitest';
import { applyOctocodeUi } from '../src/extension-ui.js';
import { bindRuntimeRenderer } from '../src/tools/runtime-renderer.js';
import { createRuntimeStore } from '../src/tools/runtime-store.js';
import type { PiContext } from '../src/types.js';

test('fresh Pi contexts do not restart an unchanged working indicator', () => {
  let indicators = 0;
  let labels = 0;
  const ctx: PiContext = {
    hasUI: true,
    mode: 'tui',
    sessionManager: { getSessionId: () => 'live-ui' },
    ui: {
      setWorkingIndicator: () => { indicators += 1; },
      setHiddenThinkingLabel: () => { labels += 1; },
    },
  };
  const dispose = bindRuntimeRenderer(ctx, createRuntimeStore());
  try {
    applyOctocodeUi(ctx);
    applyOctocodeUi({ ...ctx });
    applyOctocodeUi({ ...ctx }, 'high');
    assert.equal(indicators, 1);
    assert.equal(labels, 1);
  } finally {
    dispose({ clearUi: false });
  }
});

test('working indicator refreshes when theme output or session runtime changes', () => {
  const frames: string[][] = [];
  let color = 'first';
  const ctx: PiContext = {
    hasUI: true,
    mode: 'tui',
    sessionManager: { getSessionId: () => 'theme-ui' },
    ui: {
      theme: { fg: (_name, text) => `${color}:${text}`, bold: text => text },
      setWorkingIndicator: indicator => { if (indicator) { assert.ok(indicator.frames); frames.push([...indicator.frames]); } },
    },
  };
  let dispose = bindRuntimeRenderer(ctx, createRuntimeStore());
  try {
    applyOctocodeUi(ctx);
    color = 'second';
    applyOctocodeUi(ctx);
    assert.equal(frames.length, 2);
    assert.notDeepEqual(frames[0], frames[1]);
    dispose({ clearUi: false });
    dispose = bindRuntimeRenderer(ctx, createRuntimeStore());
    applyOctocodeUi(ctx);
    assert.equal(frames.length, 3);
  } finally {
    dispose({ clearUi: false });
  }
});
