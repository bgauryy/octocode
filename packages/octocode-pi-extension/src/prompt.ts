import { SYSTEM_PROMPT_MARKER, MANAGED_BLOCK_START, MANAGED_BLOCK_END } from './constants.js';
import type { PromptMode } from './types.js';

export function shouldAppendSystemPrompt(
  systemPrompt: string,
  octocodePrompt: string,
): boolean {
  const trimmedPrompt = octocodePrompt.trim();
  if (trimmedPrompt.length === 0) return false;
  if (systemPrompt.includes(SYSTEM_PROMPT_MARKER)) return false;
  const proofSlice = trimmedPrompt.slice(0, Math.min(160, trimmedPrompt.length));
  return !systemPrompt.includes(proofSlice);
}

export function renderSystemPromptAddendum(octocodePrompt: string): string {
  return `${SYSTEM_PROMPT_MARKER}\n${octocodePrompt.trim()}\n${SYSTEM_PROMPT_MARKER}`;
}

export function renderManagedAppendSystem(octocodePrompt: string): string {
  return `${MANAGED_BLOCK_START}\n${octocodePrompt.trim()}\n${MANAGED_BLOCK_END}\n`;
}

export function mergeManagedAppendSystem(
  existingContent: string,
  octocodePrompt: string,
): string {
  const block = renderManagedAppendSystem(octocodePrompt);
  const startIndex = existingContent.indexOf(MANAGED_BLOCK_START);
  const endIndex = existingContent.indexOf(MANAGED_BLOCK_END);

  if (startIndex !== -1 && endIndex !== -1 && endIndex > startIndex) {
    const afterEnd = endIndex + MANAGED_BLOCK_END.length;
    return `${existingContent.slice(0, startIndex)}${block}${existingContent.slice(afterEnd).replace(/^\n+/, '')}`;
  }

  const prefix = existingContent.trimEnd();
  return prefix.length > 0 ? `${prefix}\n\n${block}` : block;
}

/**
 * Resolve the harness prompt mode.
 * Precedence: explicit option > OCTOCODE_PROMPT_MODE env > 'append'.
 */
export function resolvePromptMode(option?: string): PromptMode {
  if (option === 'replace' || option === 'append') return option;
  return process.env['OCTOCODE_PROMPT_MODE'] === 'replace' ? 'replace' : 'append';
}

/**
 * Build the system prompt the extension hands back to Pi.
 * - append (default): Pi's prompt, then the Octocode harness addendum.
 * - replace: the Octocode harness leads as authoritative, Pi's prompt preserved below.
 */
export function composeSystemPrompt(opts: {
  piSystemPrompt: string;
  octocodePrompt: string;
  promptMode: PromptMode;
}): string {
  const addendum = renderSystemPromptAddendum(opts.octocodePrompt);
  if (opts.promptMode === 'replace') {
    return `${addendum}\n\n${opts.piSystemPrompt}`;
  }
  return `${opts.piSystemPrompt}\n\n${addendum}`;
}
