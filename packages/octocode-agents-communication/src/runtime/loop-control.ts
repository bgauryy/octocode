/** Delivery loop control: SIGINT/SIGTERM ownership, abortable waits and the asynchronous presence beat. */
import { setTimeout as delay } from 'node:timers/promises';
import { Heartbeat } from './timing.js';

/** SIGINT and SIGTERM end a delivery loop cleanly instead of killing the process mid-offer. */
interface StopEvent {
  readonly signal: AbortSignal;
  dispose(): void;
}

/** The loops that currently stop themselves on SIGINT and SIGTERM; the process-wide signal guard (`cli.exitOnSignal`) leaves them alone. */
const activeLoops = new Set<StopEvent>();

/** Whether a loop is running that handles SIGINT and SIGTERM itself. */
export function loopActive(): boolean {
  return activeLoops.size > 0;
}

/** Registers the calling loop as the owner of SIGINT and SIGTERM until `dispose`; the signal aborts `signal`. */
export function stopEvent(): StopEvent {
  const controller = new AbortController();
  const stop = (): void => controller.abort();
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  const event: StopEvent = {
    signal: controller.signal,
    dispose() {
      activeLoops.delete(event);
      process.off('SIGINT', stop);
      process.off('SIGTERM', stop);
    },
  };
  activeLoops.add(event);
  return event;
}

/** Waits `milliseconds`, or less when `signal` aborts. */
export async function sleep(milliseconds: number, signal: AbortSignal): Promise<void> {
  if (milliseconds <= 0 || signal.aborted) return;
  await delay(milliseconds, undefined, { signal }).catch(() => undefined);
}

/** `Heartbeat` for an asynchronous beat: the cadence restarts when the beat has finished. */
export class Presence {
  private readonly heartbeat: Heartbeat;
  private running: Promise<unknown> = Promise.resolve();

  constructor(beat: () => Promise<unknown>, dueNow = false) {
    this.heartbeat = new Heartbeat(() => {
      this.running = beat();
    }, dueNow);
  }

  reset(): void {
    this.heartbeat.reset();
  }

  async tick(force = false): Promise<boolean> {
    if (!this.heartbeat.tick(force)) return false;
    await this.running;
    this.heartbeat.reset();
    return true;
  }
}
