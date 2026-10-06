import type { Message } from './model.js';
import { INBOX_BATCH_SIZE, type TeamStore } from './store.js';

/** Paced, ordered delivery. Accepted messages survive an acknowledgement failure without being injected twice. */
export class Inbox {
  private readonly unacked = new Set<number>();
  private stamp = '';
  private readAt = 0;

  constructor(private readonly maxAgeMs: number) {}

  reset(): void {
    this.unacked.clear();
    this.stamp = '';
    this.readAt = 0;
  }

  deliver(db: TeamStore, id: string, stamp: string, send: (message: Message) => boolean): void {
    const now = Date.now();
    if (stamp === this.stamp && now - this.readAt < this.maxAgeMs) return;
    const waiting = db.pending(id);
    let complete = true;
    for (const message of waiting) {
      if (this.unacked.has(message.id)) continue;
      if (!send(message)) {
        complete = false;
        break;
      }
      this.unacked.add(message.id);
    }
    db.ack(id, waiting.filter((message) => this.unacked.has(message.id)).map((message) => message.id));
    this.unacked.clear();
    this.stamp = complete && waiting.length < INBOX_BATCH_SIZE ? stamp : '';
    this.readAt = now;
  }
}
