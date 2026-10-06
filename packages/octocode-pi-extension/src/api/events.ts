import { DEFAULT_EVENT_TYPES, type ApiEvent } from './protocol.js';

const BUFFER = 500;

interface Subscription {
  types: Set<string>;
  deliver(event: ApiEvent): void;
}

/** Recent events with increasing sequence numbers, so a client that reconnects can ask for what it missed. */
export class EventLog {
  private readonly buffer: ApiEvent[] = [];
  private readonly subscribers = new Set<Subscription>();
  private counter = 0;

  get seq(): number {
    return this.counter;
  }

  publish(type: string, data: Record<string, unknown>): ApiEvent {
    const event: ApiEvent = { seq: ++this.counter, at: Date.now(), type, data };
    this.buffer.push(event);
    if (this.buffer.length > BUFFER) this.buffer.shift();
    for (const subscription of this.subscribers) {
      if (!subscription.types.has(event.type)) continue;
      try {
        subscription.deliver(event);
      } catch {
        this.subscribers.delete(subscription);
      }
    }
    return event;
  }

  /** Register a subscriber, first replaying buffered events after `since`. Returns how to remove it and whether events were lost. */
  subscribe(deliver: (event: ApiEvent) => void, options: { since?: number; types?: string[] } = {}): { close(): void; gap: boolean } {
    const subscription: Subscription = { types: new Set(options.types?.length ? options.types : DEFAULT_EVENT_TYPES), deliver };
    let gap = false;
    if (options.since !== undefined) {
      const oldest = this.buffer[0]?.seq ?? this.counter + 1;
      gap = options.since + 1 < oldest;
      for (const event of this.buffer) if (event.seq > options.since && subscription.types.has(event.type)) deliver(event);
    }
    this.subscribers.add(subscription);
    return { gap, close: () => void this.subscribers.delete(subscription) };
  }

  close(): void {
    this.subscribers.clear();
    this.buffer.length = 0;
  }
}
