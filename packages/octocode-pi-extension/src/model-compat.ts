import type { PiModel } from './types.js';

const ADAPTIVE_ANTHROPIC_MODEL_IDS = [
  /^claude-fable-5(?:$|-)/,
  /^claude-mythos(?:-preview)?(?:$|-)/,
  /^claude-opus-(?:4-[6-9]|[5-9])(?:$|-)/,
  /^claude-sonnet-(?:4-[6-9]|[5-9])(?:$|-)/,
];

/**
 * Restore compatibility metadata that custom Anthropic-compatible provider
 * entries do not inherit from Pi's built-in model catalog.
 *
 * Sonnet 4.6+ and its adaptive-thinking peers must use a stable effort level.
 * Falling back to legacy budget_tokens makes Pi shrink that budget as context
 * fills, and Anthropic treats every shrink as a prompt-cache invalidation.
 */
export function ensureAdaptiveThinkingCompatibility(model: PiModel | undefined): boolean {
  if (!model?.id || model.api !== 'anthropic-messages') return false;
  if (!ADAPTIVE_ANTHROPIC_MODEL_IDS.some((pattern) => pattern.test(model.id!))) return false;
  if (model.compat?.forceAdaptiveThinking !== undefined) return false;

  model.compat = {
    ...model.compat,
    forceAdaptiveThinking: true,
  };
  return true;
}
