import { SettingsManager } from '@earendil-works/pi-coding-agent';

import type { NotifyFn, PiContext, PiInstance } from '../types.js';
import { formatCompactionPolicyWarning } from './context-tools.js';

export interface CompactionSettingsSnapshot {
  enabled: boolean;
  reserveTokens: number;
}

export type ReadCompactionSettings = (cwd: string) => CompactionSettingsSnapshot;

const readPiCompactionSettings: ReadCompactionSettings = (cwd) => {
  const settings = SettingsManager.create(cwd).getCompactionSettings();
  return {
    enabled: settings.enabled,
    reserveTokens: settings.reserveTokens,
  };
};

/** Surface a precise, deduplicated warning; user settings remain user-owned. */
export function registerCompactionPolicyGuidance(
  pi: PiInstance,
  notify: NotifyFn,
  readSettings: ReadCompactionSettings = readPiCompactionSettings,
): void {
  let lastWarning: string | undefined;

  const check = (ctx: PiContext): void => {
    const contextWindow = ctx.getContextUsage?.()?.contextWindow;
    if (typeof contextWindow !== 'number' || !Number.isFinite(contextWindow) || contextWindow <= 0) return;
    try {
      const warning = formatCompactionPolicyWarning({ contextWindow, ...readSettings(ctx.cwd ?? process.cwd()) });
      if (warning && warning !== lastWarning) notify(ctx, warning, 'warning');
      lastWarning = warning;
    } catch {
      // A settings-read failure must not break session startup; Pi still owns compaction.
    }
  };

  pi.on('session_start', async (_event, ctx) => {
    lastWarning = undefined;
    check(ctx);
  });
  pi.on('model_select', async (_event, ctx) => check(ctx));
  pi.on('session_shutdown', async () => {
    lastWarning = undefined;
  });
}
