import { SYSTEM_PROMPT_MARKER, MANAGED_BLOCK_START, MANAGED_BLOCK_END } from './constants.js';
import type { PromptMode } from '@octocodeai/agent-contracts/protocols';

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
    // Well-formed block → replace it in place.
    const afterEnd = endIndex + MANAGED_BLOCK_END.length;
    return `${existingContent.slice(0, startIndex)}${block}${existingContent.slice(afterEnd).replace(/^\n+/, '')}`;
  }

  // A dangling/corrupted managed block (START without a valid END after it, or an
  // orphaned END) must not accumulate: drop everything from the first marker of a
  // broken pair onward, then append one fresh block. Without this, every write
  // would leave the old half-block and stack a new one below it.
  const brokenAt = startIndex !== -1 ? startIndex : endIndex;
  const base = brokenAt !== -1 ? existingContent.slice(0, brokenAt) : existingContent;
  const prefix = base.trimEnd();
  return prefix.length > 0 ? `${prefix}\n\n${block}` : block;
}

/**
 * Resolve the harness prompt mode.
 * Precedence: explicit option > OCTOCODE_PROMPT_MODE env > 'append'.
 */
export function resolvePromptMode(option?: string): PromptMode {
  if (option === 'append' || option === 'octocode-first') return option;
  const envMode = process.env['OCTOCODE_PROMPT_MODE'];
  if (envMode === 'octocode-first') return 'octocode-first';
  return 'append';
}

/**
 * Build the system prompt the extension hands back to Pi.
 * - append (default): Pi's prompt, then the Octocode harness addendum.
 * - octocode-first: the Octocode harness leads, with Pi's prompt preserved below.
 */
/**
 * Remove Pi's `<project_context>` block (AGENTS.md / CLAUDE.md content) from an
 * already-built Pi system prompt. Pi assembles the prompt BEFORE
 * `before_agent_start` fires, so suppressing context files can only happen by
 * stripping the block from `event.systemPrompt` — mutating
 * `systemPromptOptions.contextFiles` in the hook has no effect.
 */
export function stripProjectContext(piSystemPrompt: string): string {
  return stripTaggedBlocks(piSystemPrompt, 'project_context', () => true, true).trim();
}

/**
 * Remove Pi's own skills section from an already-built Pi system prompt.
 * Octocode owns the model-facing skill flow (the `skill` tool + the
 * `<available_skills>` addendum): Pi's section instructs the model to use the
 * `read` builtin (removed by Octocode) and duplicates the same
 * `<available_skills>` tag. Pi normally omits it when `read` is inactive, but
 * that is a side effect of tool selection — stripping here makes the disable
 * deterministic regardless of tool-set timing.
 */
export function stripPiSkillsSection(piSystemPrompt: string): string {
  return stripTaggedBlocks(
    piSystemPrompt,
    'available_skills',
    (block) => /<skill(?:\s|>)/.test(block) && /<location(?:\s|>)/.test(block),
    true,
    true,
  ).trim();
}

function stripTaggedBlocks(
  source: string,
  tag: string,
  shouldStrip: (block: string) => boolean,
  truncateMalformed: boolean,
  includePrecedingParagraph = false,
): string {
  const open = `<${tag}>`;
  const close = `</${tag}>`;
  let cursor = 0;
  let output = '';
  for (;;) {
    const start = source.indexOf(open, cursor);
    if (start === -1) return `${output}${source.slice(cursor)}`;
    let scan = start + open.length;
    let depth = 1;
    while (depth > 0) {
      const nextOpen = source.indexOf(open, scan);
      const nextClose = source.indexOf(close, scan);
      if (nextClose === -1) {
        const malformed = source.slice(start);
        if (truncateMalformed && shouldStrip(malformed)) {
          return `${output}${source.slice(cursor, paragraphStart(source, start, includePrecedingParagraph))}`.trimEnd();
        }
        return `${output}${source.slice(cursor)}`;
      }
      if (nextOpen !== -1 && nextOpen < nextClose) {
        depth += 1;
        scan = nextOpen + open.length;
      } else {
        depth -= 1;
        scan = nextClose + close.length;
      }
    }
    const block = source.slice(start, scan);
    if (!shouldStrip(block)) {
      output += source.slice(cursor, scan);
      cursor = scan;
      continue;
    }
    const removalStart = paragraphStart(source, start, includePrecedingParagraph);
    output += source.slice(cursor, removalStart);
    cursor = scan;
    if (source[cursor] === '\n') cursor += 1;
  }
}

function paragraphStart(source: string, blockStart: number, includePrecedingParagraph: boolean): number {
  if (!includePrecedingParagraph) return blockStart;
  const beforeBlock = source.lastIndexOf('\n\n', blockStart - 1);
  if (beforeBlock === -1) return blockStart;
  const beforeGuidance = source.lastIndexOf('\n\n', beforeBlock - 1);
  return beforeGuidance === -1 ? beforeBlock + 2 : beforeGuidance + 2;
}

export function composeSystemPrompt(opts: {
  piSystemPrompt: string;
  octocodePrompt: string;
  promptMode: PromptMode;
}): string {
  const addendum = renderSystemPromptAddendum(opts.octocodePrompt);
  if (opts.promptMode === 'octocode-first') {
    return `${addendum}\n\n${opts.piSystemPrompt}`;
  }
  return `${opts.piSystemPrompt}\n\n${addendum}`;
}
