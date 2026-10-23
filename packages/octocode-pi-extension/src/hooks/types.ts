import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import type { HookConfig, HookEvent } from './config.js';
import type { HookVerdict } from './runner.js';

/** Time each hook command took, for `/octocode hooks`. */
export interface HookTiming {
  runs: number;
  totalMs: number;
  maxMs: number;
}

/** Records a `PreToolUse` hook that gave no answer (`allowed`: the call went ahead) in the permission audit. */
export type HookFailureSink = (what: readonly string[], note: string, allowed: boolean) => void;

/** What every event handler shares: the loaded config (replaced on each session start) and the repository root. */
export interface HookSession {
  config: HookConfig;
  errors: string[];
  /** The repository root for CLAUDE_PROJECT_DIR, found once per session and only when a hook actually runs. */
  root: string | undefined;
}

/** The shared hook runner: `dispatch` runs the hooks of one event, `reportFailure` says once per event that a dispatch itself failed. */
export interface HookRunner {
  dispatch(this: void, ctx: ExtensionContext, event: HookEvent, names: string[], input: () => Record<string, unknown>, signal?: AbortSignal): Promise<HookVerdict>;
  reportFailure(this: void, ctx: ExtensionContext, event: HookEvent, error: unknown): void;
}
