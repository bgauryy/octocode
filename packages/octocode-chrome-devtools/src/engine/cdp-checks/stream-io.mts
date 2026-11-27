// Evidence streams to disk: a sync writer that keeps every chunk in order, and a CDP IO.read pump.
import { closeSync, openSync, writeSync } from 'node:fs';
import type { Session } from '../cdp-connection.mjs';

/** A file written in call order; sync writes keep event-handler chunks in the order they arrive. */
export function openSink(file: string) {
  const fd = openSync(file, 'wx', 0o600);
  let bytes = 0;
  let closed = false;
  const write = (data: Buffer) => {
    for (let at = 0; at < data.length;) {
      at += writeSync(fd, data, at, data.length - at);
    }
    bytes += data.length;
  };
  return {
    write,
    writeText: (text: string) => write(Buffer.from(text, 'utf8')),
    get bytes() {
      return bytes;
    },
    close() {
      if (closed) return;
      closed = true;
      closeSync(fd);
    },
  };
}

/**
 * Reads a CDP stream handle to the end into file, chunk by chunk, and closes the handle.
 * Returns the byte count and chunk count written.
 */
export async function pumpStream(
  cdp: Session,
  handle: string,
  file: string,
  chunkSize = 1 << 20
) {
  const sink = openSink(file);
  let chunks = 0;
  try {
    for (;;) {
      const chunk = await cdp.send('IO.read', { handle, size: chunkSize });
      chunks++;
      const data = String(chunk.data ?? '');
      sink.write(Buffer.from(data, chunk.base64Encoded ? 'base64' : 'utf8'));
      if (chunk.eof) break;
    }
  } finally {
    sink.close();
    await cdp.send('IO.close', { handle }).catch(() => {});
  }
  return { bytes: sink.bytes, chunks };
}
