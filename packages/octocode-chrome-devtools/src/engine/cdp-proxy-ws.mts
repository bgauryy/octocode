// Minimal RFC 6455 server side for the CDP proxy: the upgrade handshake and text frames, no dependency
// (the published package ships without `ws`). Clients are CDP tools, so only text messages are carried.
import { createHash } from 'node:crypto';
import type { IncomingMessage } from 'node:http';
import type { Duplex } from 'node:stream';

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
/** A CDP message (a screenshot, a DOM snapshot) can be large; a frame past this closes with 1009. */
export const MAX_MESSAGE_BYTES = 256 * 1024 * 1024;

/** One accepted WebSocket client. */
export type Peer = {
  send(text: string): void;
  close(code?: number): void;
  readonly closed: boolean;
};
export type PeerHandlers = {
  message(text: string): void;
  close(): void;
};

const frame = (opcode: number, payload: Buffer) => {
  const n = payload.length;
  const head =
    n < 126
      ? Buffer.from([0x80 | opcode, n])
      : n < 65536
        ? Buffer.from([0x80 | opcode, 126, n >> 8, n & 255])
        : (() => {
            const h = Buffer.alloc(10);
            h[0] = 0x80 | opcode;
            h[1] = 127;
            h.writeBigUInt64BE(BigInt(n), 2);
            return h;
          })();
  return Buffer.concat([head, payload]);
};

/** Completes the handshake on an already validated upgrade request and starts framing. */
export function acceptWebSocket(
  req: IncomingMessage,
  socket: Duplex,
  head: Buffer,
  handlers: PeerHandlers
): Peer | null {
  const key = req.headers['sec-websocket-key'];
  if (
    typeof key !== 'string' ||
    req.headers['sec-websocket-version'] !== '13' ||
    String(req.headers.upgrade).toLowerCase() !== 'websocket'
  ) {
    socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n');
    return null;
  }
  const accept = createHash('sha1')
    .update(key + GUID)
    .digest('base64');
  socket.write(
    `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`
  );
  let closed = false,
    buffer: Buffer = Buffer.alloc(0),
    parts: Buffer[] = [],
    size = 0;
  const finish = () => {
    if (closed) return;
    closed = true;
    handlers.close();
  };
  const close = (code = 1000) => {
    if (socket.destroyed || socket.writableEnded) return finish();
    const payload = Buffer.alloc(2);
    payload.writeUInt16BE(code);
    socket.end(frame(0x8, payload));
    finish();
  };
  const peer: Peer = {
    send(text) {
      if (!closed && !socket.destroyed)
        socket.write(frame(0x1, Buffer.from(text)));
    },
    close,
    get closed() {
      return closed;
    },
  };
  const parse = () => {
    for (;;) {
      if (buffer.length < 2) return;
      const first = buffer[0],
        second = buffer[1];
      const opcode = first & 0x0f,
        masked = (second & 0x80) !== 0;
      let length = second & 0x7f,
        offset = 2;
      if (length === 126) {
        if (buffer.length < 4) return;
        length = buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        if (buffer.length < 10) return;
        const big = buffer.readBigUInt64BE(2);
        if (big > BigInt(MAX_MESSAGE_BYTES)) return close(1009);
        length = Number(big);
        offset = 10;
      }
      if (!masked) return close(1002);
      if (size + length > MAX_MESSAGE_BYTES) return close(1009);
      if (buffer.length < offset + 4 + length) return;
      const mask = buffer.subarray(offset, offset + 4);
      const payload = Buffer.from(
        buffer.subarray(offset + 4, offset + 4 + length)
      );
      for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i & 3];
      buffer = buffer.subarray(offset + 4 + length);
      if (opcode === 0x8) return close(1000);
      if (opcode === 0x9) {
        socket.write(frame(0xa, payload));
        continue;
      }
      if (opcode === 0xa) continue;
      if (opcode === 0x1 || opcode === 0x0) {
        parts.push(payload);
        size += payload.length;
        if (first & 0x80) {
          const text = Buffer.concat(parts).toString('utf8');
          parts = [];
          size = 0;
          handlers.message(text);
        }
        continue;
      }
      return close(1003);
    }
  };
  socket.on('data', chunk => {
    buffer = buffer.length ? Buffer.concat([buffer, chunk]) : chunk;
    parse();
  });
  socket.on('close', finish);
  socket.on('error', finish);
  if (head.length) {
    buffer = head;
    // The caller assigns the returned peer before the first message is delivered.
    queueMicrotask(parse);
  }
  return peer;
}
