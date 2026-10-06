import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { plural, statusColor } from '../shared/render.js';

type Send = ExtensionAPI['sendMessage'];
type Message = Parameters<Send>[0];
type Options = Parameters<Send>[1];

export const QUEUED_STATUS = 'octocode-queued';

/**
 * Keeps extension messages (subagent reports, background job reports, teammate messages) that `pi.sendMessage` queued
 * during a run from being lost.
 *
 * While a run is active, a message that may start a turn joins Pi's steering or follow-up queue. Pi shows only the
 * user's own queued text, and when the user presses Esc (or dequeues) it clears the whole queue but puts back only
 * that text. This ledger records each queued message by its `details` object, which Pi keeps on the message it later
 * starts. A message still unstarted when the run settles was dropped. In the TUI, the only place that clears the
 * queue, it is appended to the conversation without starting a turn: the user interrupted, so nothing restarts, but
 * nothing is lost. A status segment shows how many messages wait meanwhile.
 *
 * Installed on `pi` itself, before any feature registers, so every `pi.sendMessage` call goes through it. Messages
 * without a details object, `nextTurn` messages and `triggerTurn: false` messages are not queued this way and pass
 * through untracked.
 */
export class Delivery {
  private readonly waiting = new Map<object, Message>();
  private running = false;
  private ctx: ExtensionContext | undefined;

  constructor(private readonly send: Send) {}

  static install(pi: ExtensionAPI): Delivery {
    const delivery = new Delivery(pi.sendMessage.bind(pi));
    (pi as { sendMessage: Send }).sendMessage = (message, options) => delivery.deliver(message, options);
    pi.on('session_start', async (_event, ctx) => delivery.reset(ctx));
    pi.on('session_shutdown', async () => delivery.reset(undefined));
    pi.on('agent_start', async (_event, ctx) => {
      delivery.ctx = ctx;
      delivery.running = true;
    });
    pi.on('message_start', async (event) => {
      const { message } = event;
      if (message.role === 'custom' && isObject(message.details) && delivery.waiting.delete(message.details)) delivery.paint();
    });
    pi.on('agent_settled', async (_event, ctx) => delivery.settle(ctx));
    return delivery;
  }

  /** Messages queued and not started yet. */
  get pending(): number {
    return this.waiting.size;
  }

  deliver(message: Message, options?: Options): void {
    this.send(message, options);
    const queued = this.running && options?.triggerTurn !== false && options?.deliverAs !== 'nextTurn';
    if (!queued || !isObject(message.details)) return;
    this.waiting.set(message.details, message);
    this.paint();
  }

  /** The run is over: Pi starts queued messages before it settles, so any still waiting were cleared. */
  private settle(ctx: ExtensionContext): void {
    this.running = false;
    this.ctx = ctx;
    const dropped = [...this.waiting.values()];
    this.waiting.clear();
    this.paint();
    // Outside the TUI nothing clears the queue: an aborted run leaves the messages there for the next one.
    if (dropped.length === 0 || ctx.mode !== 'tui') return;
    let kept = 0;
    for (const message of dropped) {
      try {
        this.send(message, { triggerTurn: false });
        kept += 1;
      } catch {
        // The session ended meanwhile.
      }
    }
    if (kept > 0 && ctx.hasUI) ctx.ui.notify(`Interrupt kept ${plural(kept, 'queued message')}: added to the conversation without starting a turn.`, 'info');
  }

  private reset(ctx: ExtensionContext | undefined): void {
    this.waiting.clear();
    this.running = false;
    this.paint();
    this.ctx = ctx;
    this.paint();
  }

  private paint(): void {
    const ctx = this.ctx;
    if (!ctx?.hasUI) return;
    try {
      ctx.ui.setStatus(QUEUED_STATUS, this.waiting.size > 0 ? statusColor(ctx, 'accent', `✉ ${this.waiting.size} queued`) : undefined);
    } catch {
      // The session was replaced.
    }
  }
}

function isObject(value: unknown): value is object {
  return typeof value === 'object' && value !== null;
}
