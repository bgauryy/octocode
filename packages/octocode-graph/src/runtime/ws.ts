// Minimal RFC 6455 server side: handshake, text frames out, masked frames in (fragmented, split across
// TCP reads, or several per read), ping/pong and close. No dependencies; local viewer traffic only.
import { createHash } from 'node:crypto';
import type { IncomingMessage } from 'node:http';
import type { Socket } from 'node:net';

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
const OP = {
  continuation: 0,
  text: 1,
  binary: 2,
  close: 8,
  ping: 9,
  pong: 10,
} as const;

/** One frame read from the start of a buffer: the unmasked payload and the bytes it took (`size`). */
export interface Frame {
  fin: boolean;
  opcode: number;
  masked: boolean;
  payload: Buffer;
  size: number;
}

export interface DecoderHandlers {
  onMessage: (text: string) => void;
  onControl: (frame: Frame) => void;
  onError: (error: Error) => void;
}

/** The server end of one socket: JSON text frames, binary frames, and the close handshake. */
export interface WsClient {
  readonly open: boolean;
  readonly writable: boolean;
  readonly byteTarget: number;
  send(value: unknown): boolean;
  sendBinary(buffer: Buffer): boolean;
  close(code?: number): void;
}

export interface AcceptHandlers {
  onMessage?: (text: string, client: WsClient) => void;
  onClose?: () => void;
  onDrain?: () => void;
}

export const acceptKey = (key: string) =>
  createHash('sha1')
    .update(key + GUID)
    .digest('base64');

// One unmasked server frame (servers never mask).
export function encode(
  payload: Buffer | string,
  opcode: number = OP.text
): Buffer {
  const body = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
  const n = body.length;
  let head: Buffer;
  if (n < 126) head = Buffer.from([0x80 | opcode, n]);
  else if (n < 65536) {
    head = Buffer.alloc(4);
    head.writeUInt16BE(n, 2);
  } else {
    head = Buffer.alloc(10);
    head.writeBigUInt64BE(BigInt(n), 2);
  }
  if (n >= 126) head.set([0x80 | opcode, n < 65536 ? 126 : 127], 0);
  return Buffer.concat([head, body]);
}

// The frame at the start of `buf`, or null when more bytes are needed.
export function parseFrame(buf: Buffer): Frame | null {
  if (buf.length < 2) return null;
  let n = buf[1] & 127,
    at = 2;
  if (n === 126) {
    if (buf.length < 4) return null;
    n = buf.readUInt16BE(2);
    at = 4;
  } else if (n === 127) {
    if (buf.length < 10) return null;
    n = Number(buf.readBigUInt64BE(2));
    at = 10;
  }
  const masked = (buf[1] & 0x80) !== 0;
  const end = at + (masked ? 4 : 0) + n;
  if (buf.length < end) return null;
  const payload = Buffer.from(buf.subarray(end - n, end));
  if (masked) {
    const key = buf.subarray(at, at + 4);
    for (let i = 0; i < payload.length; i++) payload[i] ^= key[i & 3];
  }
  return {
    fin: (buf[0] & 0x80) !== 0,
    opcode: buf[0] & 15,
    masked,
    payload,
    size: end,
  };
}

// Feeds TCP chunks; calls onMessage(text) per complete message and onControl(frame) for control frames.
export function decoder({ onMessage, onControl, onError }: DecoderHandlers) {
  let buf: Buffer = Buffer.alloc(0);
  let parts: Buffer[] = [];
  const data = (f: Frame) => {
    parts.push(f.payload);
    if (!f.fin) return;
    const text = Buffer.concat(parts).toString('utf8');
    parts = [];
    onMessage(text);
  };
  return (chunk: Buffer) => {
    buf = buf.length ? Buffer.concat([buf, chunk]) : chunk;
    for (let f = parseFrame(buf); f; f = parseFrame(buf)) {
      buf = buf.subarray(f.size);
      if (!f.masked) {
        onError(new Error('client frames must be masked'));
        break;
      }
      if (f.opcode >= OP.close) onControl(f);
      else data(f);
    }
  };
}

// Completes the upgrade and returns a client: send(object) writes one JSON text frame, sendBinary(buffer) one binary frame.
export function accept(
  req: IncomingMessage,
  socket: Socket,
  {
    onMessage = () => {},
    onClose = () => {},
    onDrain = () => {},
  }: AcceptHandlers = {}
): WsClient {
  socket.write(
    [
      'HTTP/1.1 101 Switching Protocols',
      'Upgrade: websocket',
      'Connection: Upgrade',
      `Sec-WebSocket-Accept: ${acceptKey(String(req.headers['sec-websocket-key']))}`,
      '',
      '',
    ].join('\r\n')
  );
  socket.setNoDelay(true);
  let open = true;
  const close = (code = 1000) => {
    if (!open) return;
    open = false;
    const body = Buffer.alloc(2);
    body.writeUInt16BE(code);
    socket.end(encode(body, OP.close));
    onClose();
  };
  const client: WsClient = {
    get open() {
      return open;
    },
    get writable() {
      return open && !socket.writableNeedDrain;
    },
    get byteTarget() {
      return socket.writableHighWaterMark;
    },
    send: value => open && socket.write(encode(JSON.stringify(value))),
    sendBinary: buffer => open && socket.write(encode(buffer, OP.binary)),
    close,
  };
  const onControl = (f: Frame) => {
    if (f.opcode === OP.ping) socket.write(encode(f.payload, OP.pong));
    else if (f.opcode === OP.close) close();
  };
  socket.on('drain', onDrain);
  socket.on(
    'data',
    decoder({
      onMessage: text => onMessage(text, client),
      onControl,
      onError: () => close(1002),
    })
  );
  socket.on('close', () => {
    if (open) {
      open = false;
      onClose();
    }
  });
  socket.on('error', () => socket.destroy());
  return client;
}
