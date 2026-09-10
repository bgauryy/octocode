import { stripVTControlCharacters } from 'node:util';
import { wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { truncateToWidth } from '../tui/width.js';
import { BRAND_DIAMOND, SEP, paint, sanitizeLine } from '../tui/palette.js';
import { renderFrame } from '../tui/components.js';
import type { PiTheme } from '../types.js';
import { isExecutionEvent, type ExecutionEvent } from './execution-events.js';

function header(title: string, label: string, theme: PiTheme | undefined): string {
  return `${paint(theme, 'brand', BRAND_DIAMOND)} ${paint(theme, 'title', title)}${paint(theme, 'dim', SEP)}${paint(theme, 'muted', label)}`;
}

function eventBody(
  event: ExecutionEvent,
  expanded: boolean,
  theme: PiTheme | undefined,
): { title: string; body: string[]; footer?: string; warning?: boolean } | undefined {
  if (event.type === 'plan.updated') {
    const tasks = event.payload.tasks;
    const done = tasks.filter(task => task.status === 'done').length;
    const active = tasks.filter(task => task.status === 'doing').length;
    const blocked = tasks.filter(task => task.status === 'blocked').length;
    const ready = tasks.filter(task => task.status === 'todo').length;
    const counts = `${done} done${active ? `${SEP}${active} active` : ''}${ready ? `${SEP}${ready} ready` : ''}${blocked ? `${SEP}${blocked} blocked` : ''}`;
    const marks: Record<string, string> = {
      done: '✓',
      doing: '⟳',
      todo: '○',
      blocked: '!',
    };
    return {
      title: header('Plan', sanitizeLine(event.payload.phase.replaceAll('_', ' ')), theme),
      body: [
        paint(theme, blocked ? 'warning' : 'muted', counts),
        ...(expanded
          ? tasks.map(task =>
              `${paint(theme, task.status === 'done' ? 'success' : task.status === 'blocked' ? 'warning' : task.status === 'doing' ? 'brand' : 'dim', marks[task.status] ?? '○')} ${paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(task.title)))}`
            )
          : []),
      ],
      footer: expanded ? 'Plan progress' : undefined,
      warning: blocked > 0,
    };
  }
  if (event.type === 'agent.message') {
    const direction = event.payload.direction === 'from-agent' ? '←' : '→';
    return {
      title: header(
        'Agent message',
        `${sanitizeLine(event.payload.name)}${SEP}${event.payload.action}`,
        theme,
      ),
      body: [
        `${paint(theme, 'brand', direction)} ${paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(event.payload.preview)))}`,
        ...(expanded && event.payload.planStep
          ? [paint(theme, 'muted', `plan: ${sanitizeLine(event.payload.planStep)}`)]
          : []),
      ],
      footer: expanded ? 'Worker communication' : undefined,
    };
  }
  if (event.type === 'agent.transition') {
    const warning = ['blocked', 'failed'].includes(event.payload.to);
    const transition = event.payload.from
      ? `${event.payload.from} → ${event.payload.to}`
      : event.payload.to;
    const state = paint(
      theme,
      warning ? 'warning' : event.payload.to === 'done' ? 'success' : 'muted',
      transition,
    );
    return {
      title: header(
        'Agent',
        `${sanitizeLine(event.payload.name)}${SEP}${sanitizeLine(event.payload.to)}`,
        theme,
      ),
      body: [
        ...(event.payload.summary
          ? [paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(event.payload.summary)))]
          : [state]),
        ...(expanded && event.payload.summary ? [state] : []),
        ...(expanded && event.payload.planStep
          ? [paint(theme, 'muted', `plan: ${sanitizeLine(event.payload.planStep)}`)]
          : []),
      ],
      footer: expanded ? 'Worker state' : undefined,
      warning,
    };
  }
  return undefined;
}

/** Render durable execution facts as transcript cards without adding model context. */
export function buildExecutionEventCard(
  value: unknown,
  expanded: boolean,
  theme: PiTheme | undefined,
  width: number,
): string[] {
  if (!isExecutionEvent(value)) return [];
  const card = eventBody(value, expanded, theme);
  if (!card) return [];
  if (!expanded)
    return [card.title, ...card.body.slice(0, 1).map(line => `  ${line}`)].map(
      line => truncateToWidth(line, Math.max(1, width)),
    );
  return renderFrame(
    {
      title: card.title,
      body: card.body.flatMap(line =>
        wrapTextWithAnsi(line, Math.max(1, width - 3))
      ),
      footer: card.footer,
      borderToken: card.warning ? 'warning' : 'dim',
    },
    { width, theme },
  );
}
