// Socket cursors index the event log. A reconnect supplies a sequence number because
// replay logs can have gaps. A slow client resumes directly from the log on drain.
import { constants, gzipSync } from 'node:zlib';

function firstIndex(log, since) {
  let lo = 0, hi = log.length;
  while (lo < hi) {
    const mid = lo + Math.floor((hi - lo) / 2);
    if (log[mid].seq < since) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

function batch(log, start, end, replay, byteTarget) {
  const parts = [];
  let next = start, bytes = 0;
  while (next < end && (!parts.length || bytes < byteTarget)) {
    const item = JSON.stringify(log[next++]);
    parts.push(item);
    bytes += Buffer.byteLength(item) + 1;
  }
  const header = replay ? '{"type":"batch","replay":true,"events":[' : '{"type":"batch","events":[';
  // On the loopback connection compression speed matters more than the last few bytes.
  return { next, payload: gzipSync(`${header}${parts.join(',')}]}`, { level: constants.Z_BEST_SPEED }) };
}

export function eventStream(log) {
  const clients = new Set();
  const cursors = new WeakMap();
  let scheduled = false;

  function schedule() {
    if (!scheduled && clients.size) {
      scheduled = true;
      setImmediate(flush);
    }
  }
  function flush() {
    scheduled = false;
    const batches = new Map();
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

  function hello(client, since) {
    cursors.set(client, { next: firstIndex(log, since), replayEnd: log.length });
    schedule();
  }
  return { clients, schedule, hello };
}
