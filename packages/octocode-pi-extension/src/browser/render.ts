import type { Theme } from '@earendil-works/pi-coding-agent';
import type { Component } from '@earendil-works/pi-tui';
import { clip, resultBlock, resultText, spillPath, toolHeader } from '../shared/render.js';
import { timingOf, type RenderContext } from '../shared/tool-timing.js';
import { redactSecrets } from '../shared/sanitize.js';
import { clipText } from '../shared/util.js';
import { snapshotFileOf } from './snapshot.js';

/** The browser arguments the header shows. */
interface BrowserCallArgs {
  action?: string;
  url?: string;
  ref?: number;
  key?: string;
  expression?: string;
  text?: string;
}

/** `● Browser(navigate https://example.com)`: the action and its target (URL, element, key, or expression). */
export function browserCallHeader(args: BrowserCallArgs, theme: Theme, context: RenderContext): Component {
  const target = args.url ?? (args.ref !== undefined ? `[${args.ref}]` : undefined) ?? args.key ?? (args.expression ? clip(args.expression, 80) : undefined);
  const typed = args.action === 'type' && args.text ? ` ${JSON.stringify(clipText(redactSecrets(args.text), 60))}` : '';
  return toolHeader(theme, context, 'Browser', `${args.action ?? ''}${target ? ` ${target}` : ''}${typed}`);
}

/** Screenshots are drawn by Pi from the image content; snapshots collapse to `title — url`. */
export function browserResult(result: { content: Array<{ type: string; text?: string }>; details?: unknown }, theme: Theme, context: RenderContext): Component {
  const text = resultText(result).replace(/^\[image\]$/m, '').trim();
  const { summary, body } = text ? browserSummary(text) : { summary: 'Screenshot', body: '' };
  return resultBlock(theme, context, { summary, body, spill: snapshotFileOf(text) ?? spillPath(text), ...timingOf(result.details) });
}

/**
 * The `⎿` line of a result: `title — url` for a snapshot (`# title` then the URL), else its first line; the rest is the
 * body.
 */
export function browserSummary(text: string): { summary: string; body: string } {
  const lines = text.split('\n');
  // A snapshot starts with its title; a result that opens with a note (a wait, a filled-fields count) keeps the note.
  if (lines[0]?.startsWith('# ') && lines[1]?.trim()) return { summary: `${lines[0].slice(2).trim() || '(untitled)'} — ${lines[1].trim()}`, body: lines.slice(2).join('\n') };
  return { summary: lines[0] ?? '', body: lines.slice(1).join('\n') };
}
