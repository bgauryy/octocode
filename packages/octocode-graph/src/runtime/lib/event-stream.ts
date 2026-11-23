// Socket cursors index the event log. A reconnect supplies a sequence number because
// replay logs can have gaps. A slow client resumes directly from the log on drain.
import { constants, gzipSync } from 'node:zlib';

/** A log row: every event the stream sends carries its sequence number. */
export interface Sequenced {
  readonly seq: number;
}

/** A socket the stream writes to: whether it takes more now, the bytes per batch, and the binary send. */
export interface StreamClient {
  readonly writable: boolean;
  readonly byteTarget: number;
  sendBinary(payload: Buffer): void;
}

/** Where a client is in the log; `replayEnd` marks the end of its first (replay) pass. */
interface Cursor {
  next: number;
  replayEnd: number | null;
}

function firstIndex(log: readonly Sequenced[], since: number) {
  let lo = 0,
    hi = log.length;
  while (lo < hi) {
    const mid = lo + Math.floor((hi - lo) / 2);
    if (log[mid].seq < since) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

function batch(
  log: readonly Sequenced[],
  start: number,
  end: number,
  replay: boolean,
  byteTarget: number
) {
  const parts: string[] = [];
  let next = start,
    bytes = 0;
  while (next < end && (!parts.length || bytes < byteTarget)) {
    const item = JSON.stringify(log[next++]);
    parts.push(item);
    bytes += Buffer.byteLength(item) + 1;
  }
  const header = replay
    ? '{"type":"batch","replay":true,"events":['
    : '{"type":"batch","events":[';
  // On the loopback connection compression speed matters more than the last few bytes.
  return {
    next,
    payload: gzipSync(`${header}${parts.join(',')}]}`, {
      level: constants.Z_BEST_SPEED,
    }),
  };
}

export function eventStream<C extends StreamClient>(log: readonly Sequenced[]) {
  const clients = new Set<C>();
  const cursors = new WeakMap<C, Cursor>();
  let scheduled = false;

  function schedule() {
    if (!scheduled && clients.size) {
      scheduled = true;
      setImmediate(flush);
    }
  }
  function flush() {
    scheduled = false;
    const batches = new Map<string, ReturnType<typeof batch>>();
    for (const client of clients) {
      const cursor = cursors.get(client);
      if (!cursor || !client.writable) continue;
      const replay = cursor.replayEnd !== null;
      const end = cursor.replayEnd ?? log.length;
      if (!replay && cursor.next >= end) continue;
      // Clients at the same position can share compression. One complete event
      // may exceed the socket's write target and still travels intact.
      const key = [cursor.next, end, replay, client.byteTarget].join('/');
      let page = batches.get(key);
      if (!page) {
        page = batch(log, cursor.next, end, replay, client.byteTarget);
        batches.set(key, page);
      }
      client.sendBinary(page.payload);
      cursor.next = page.next;
      if (cursor.next === end) cursor.replayEnd = null;
      if (cursor.next < log.length && client.writable) schedule();
    }
  }

  function hello(client: C, since: number) {
    cursors.set(client, {
      next: firstIndex(log, since),
      replayEnd: log.length,
    });
    schedule();
  }
  return { clients, schedule, hello };
}
