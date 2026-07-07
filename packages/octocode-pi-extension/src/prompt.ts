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
 * `replace` is kept as a compatibility alias for the accurate `octocode-first` mode.
 */
export function resolvePromptMode(option?: string): PromptMode {
  if (option === 'append' || option === 'octocode-first') return option;
  if (option === 'replace') return 'octocode-first';
  const envMode = process.env['OCTOCODE_PROMPT_MODE'];
  if (envMode === 'octocode-first' || envMode === 'replace') return 'octocode-first';
  return 'append';
}

/**
 * Build the system prompt the extension hands back to Pi.
 * - append (default): Pi's prompt, then the Octocode harness addendum.
 * - octocode-first: the Octocode harness leads, with Pi's prompt preserved below.
 */
export function composeSystemPrompt(opts: {
  piSystemPrompt: string;
  octocodePrompt: string;
  promptMode: PromptMode;
}): string {
  const addendum = renderSystemPromptAddendum(opts.octocodePrompt);
  // 'replace' is a public-API compat alias for 'octocode-first' (harness leads). The
  // extension's own wiring normalizes it via resolvePromptMode, but composeSystemPrompt
  // is exported and callers may still pass 'replace' directly.
  if (opts.promptMode === 'octocode-first' || opts.promptMode === 'replace') {
    return `${addendum}\n\n${opts.piSystemPrompt}`;
  }
  return `${opts.piSystemPrompt}\n\n${addendum}`;
}
