import { createHash } from 'node:crypto';
import type { FileSnapshot } from '@octocodeai/octocode-extension-rust';
import { forgetFileReadState, recordFileReadStateFromContent } from './file-state.js';
import { markOwnWrite } from './peer-wip.js';

export interface FileMutationReceiptV1 {
  version: 1;
  classification: 'applied' | 'noop';
  preFingerprint: string;
  postFingerprint: string;
  bytesBefore: number;
  bytesAfter: number;
  bytesChanged: number;
  linesAdded: number;
  linesDeleted: number;
  diffTruncated: boolean;
  patchTruncated: boolean;
}

function contentFingerprint(content: string): string {
  return `sha256:${createHash('sha256').update(content, 'utf8').digest('hex')}`;
}

function snapshotFingerprint(snapshot: FileSnapshot): string {
  if (!snapshot.exists) return 'missing';
  return snapshot.digest ? `sha256:${snapshot.digest}` : `version:${snapshot.version}`;
}

export function countMutationLines(content: string): number {
  if (content.length === 0) return 0;
  return content.endsWith('\n') ? content.split('\n').length - 1 : content.split('\n').length;
}

export function createCommittedMutationReceipt(args: {
  snapshot: FileSnapshot;
  nextContent?: string;
  previousLines?: number;
  linesAdded?: number;
  linesDeleted?: number;
  bytesChanged?: number;
  diffTruncated?: boolean;
  patchTruncated?: boolean;
}): FileMutationReceiptV1 {
  const preFingerprint = snapshotFingerprint(args.snapshot);
  const postFingerprint = args.nextContent === undefined ? 'missing' : contentFingerprint(args.nextContent);
  const bytesBefore = args.snapshot.size;
  const bytesAfter = args.nextContent === undefined ? 0 : Buffer.byteLength(args.nextContent, 'utf8');
  const classification = preFingerprint === postFingerprint ? 'noop' : 'applied';
  return {
    version: 1,
    classification,
    preFingerprint,
    postFingerprint,
    bytesBefore,
    bytesAfter,
    bytesChanged: classification === 'noop'
      ? 0
      : args.bytesChanged ?? (bytesBefore + bytesAfter),
    linesAdded: classification === 'noop'
      ? 0
      : args.linesAdded ?? (args.nextContent === undefined ? 0 : countMutationLines(args.nextContent)),
    linesDeleted: classification === 'noop'
      ? 0
      : args.linesDeleted ?? args.previousLines ?? 0,
    diffTruncated: args.diffTruncated ?? false,
    patchTruncated: args.patchTruncated ?? false,
  };
}

/** Bookkeeping cannot turn a committed filesystem mutation into an uncommitted failure. */
export async function finishFileMutation(absolutePath: string, content?: string): Promise<string[]> {
  const warnings: string[] = [];
  try {
    forgetFileReadState(absolutePath);
    if (content !== undefined) await recordFileReadStateFromContent(absolutePath, content);
  } catch (error) { warnings.push(`File committed; read state refresh failed: ${error instanceof Error ? error.message : String(error)}`); }
  try { markOwnWrite(absolutePath); }
  catch (error) { warnings.push(`File committed; status update failed: ${error instanceof Error ? error.message : String(error)}`); }
  return warnings;
}
