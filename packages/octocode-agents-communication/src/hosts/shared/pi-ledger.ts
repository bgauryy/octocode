import { fstatSync, readSync } from 'node:fs';
import { isObject, type JsonObject } from '../../runtime/validation.js';

const SCAN_CHUNK = 1024 * 1024;

/** Receipts read so far from one Pi session ledger: the byte offset scanned, the unfinished last line, and the lines skipped as malformed. */
interface LedgerScan {
  identity: string;
  offset: number;
  rest: Buffer;
  receipts: Set<string>;
  header: boolean;
  skipped: number;
}

const parseLedgerLine = (line: string): JsonObject | undefined => {
  try {
    const row: unknown = JSON.parse(line);
    return isObject(row) ? row : undefined;
  } catch { return undefined; }
};

/**
 * The durable receipts of a Pi session ledger. The ledger is append-only JSONL: each read parses only bytes added since the
 * last scan, and a replaced or shrunken file is rescanned from its header.
 */
export class LedgerReceipts {
  private scan: LedgerScan | undefined;

  /** `receiptsOf` names the receipts a page of ledger rows holds; `onMalformed` reports the first malformed line of a file. */
  constructor(private readonly receiptsOf: (rows: JsonObject[]) => Set<string>, private readonly onMalformed: (file: string) => void) {}

  reset(): void {
    this.scan = undefined;
  }

  /** The receipts in the open ledger `fd` of `file`, or null while it has no complete header (receipts are unknown, not absent). */
  read(fd: number, file: string, vendorSession: string): Set<string> | null {
    const metadata = fstatSync(fd);
    const identity = `${file}:${metadata.dev}:${metadata.ino}`;
    if (this.scan?.identity !== identity || metadata.size < this.scan.offset) this.scan = { identity, offset: 0, rest: Buffer.alloc(0), receipts: new Set(), header: false, skipped: 0 };
    const scan = this.scan;
    if (metadata.size === scan.offset) return scan.header ? scan.receipts : null;
    // Scan the unread tail in bounded chunks: memory stays at one chunk plus the unfinished last line, however large the ledger.
    while (scan.offset < metadata.size) {
      const bytes = Buffer.alloc(Math.min(SCAN_CHUNK, metadata.size - scan.offset));
      let read = 0;
      while (read < bytes.length) {
        const count = readSync(fd, bytes, read, bytes.length - read, scan.offset + read);
        if (!count) break;
        read += count;
      }
      this.consume(scan, Buffer.concat([scan.rest, bytes.subarray(0, read)]), file, vendorSession);
      scan.offset += read;
      if (read < bytes.length) break;
    }
    return scan.header ? scan.receipts : null;
  }

  /** Adds the receipts of the complete lines in `chunk` and keeps its unfinished last line for the next scan. */
  private consume(scan: LedgerScan, chunk: Buffer, file: string, vendorSession: string): void {
    const end = chunk.lastIndexOf(0x0a) + 1;
    const lines = chunk.subarray(0, end).toString('utf8').split('\n').filter(Boolean);
    const parsed = lines.map(parseLedgerLine);
    const header = parsed[0];
    if (!scan.header && lines.length && (header?.type !== 'session' || header.id !== vendorSession)) throw new Error('Pi ledger header does not match the bound session');
    // A malformed entry carries no readable receipt: skip it, so one bad line cannot block every later scan. Its message stays unconfirmed and is retried.
    const rows = parsed.filter((row): row is JsonObject => row !== undefined);
    if (rows.length < parsed.length && !scan.skipped) this.onMalformed(file);
    scan.skipped += parsed.length - rows.length;
    for (const receipt of this.receiptsOf(rows)) scan.receipts.add(receipt);
    scan.header ||= lines.length > 0;
    scan.rest = Buffer.from(chunk.subarray(end));
  }
}
