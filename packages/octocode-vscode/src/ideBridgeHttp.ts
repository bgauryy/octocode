import * as crypto from 'crypto';
import type * as http from 'http';

/** Largest accepted request body. */
export const MAX_BODY_BYTES = 1024 * 1024;

/** An error that maps to an HTTP status and a `{error, reason}` body. */
export class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly reason?: string,
    readonly headers: http.OutgoingHttpHeaders = {}
  ) {
    super(message);
  }
}

/** Constant-time comparison; digests equalize lengths so none leaks. */
export function tokensEqual(expected: string, supplied: string): boolean {
  const digest = (value: Buffer | string) =>
    crypto.createHash('sha256').update(value).digest();
  return crypto.timingSafeEqual(digest(expected), digest(supplied));
}

export function isJsonContentType(header: string | undefined): boolean {
  return /^application\/json\s*(;|$)/i.test(header ?? '');
}

function bodyTooLarge(): HttpError {
  return new HttpError(
    413,
    'request body too large',
    `limit is ${MAX_BODY_BYTES} bytes`
  );
}

/**
 * Reads the body. An oversized body is discarded as it arrives (never
 * buffered) and answered with a 413 once the upload ends, so the client reads
 * the status instead of a connection reset.
 */
export function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    let tooLarge = Number(req.headers['content-length']) > MAX_BODY_BYTES;
    req.on('data', (chunk: Buffer) => {
      if (tooLarge) return;
      size += chunk.length;
      if (size > MAX_BODY_BYTES) {
        tooLarge = true;
        chunks.length = 0;
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      if (tooLarge) reject(bodyTooLarge());
      else resolve(Buffer.concat(chunks).toString('utf8'));
    });
    req.on('error', reject);
  });
}
