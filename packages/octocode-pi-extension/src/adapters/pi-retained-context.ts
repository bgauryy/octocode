import { buildSessionContext } from '@earendil-works/pi-coding-agent';
import { contentDigest } from '@octocodeai/octocode-awareness/host';
import type { PiContext } from '../types.js';

export interface RetainedContextDigestOptions {
  /** Current, extension-owned segment bodies keyed by their stable manifest id. */
  knownSegmentContents?: Readonly<Record<string, string>>;
  /** Context delivered outside the persisted branch in the current turn. */
  additionalRetainedContents?: readonly string[];
}

function textBodies(content: unknown): string[] {
  if (typeof content === 'string') return [content];
  if (!Array.isArray(content)) return [];
  return content.flatMap((block) => block && typeof block === 'object'
    && (block as { type?: unknown }).type === 'text'
    && typeof (block as { text?: unknown }).text === 'string'
    ? [(block as { text: string }).text]
    : []);
}

function exactJoinedSegment(messageBody: string, segmentBody: string): boolean {
  let offset = messageBody.indexOf(segmentBody);
  while (offset >= 0) {
    const end = offset + segmentBody.length;
    const startsAtBoundary = offset === 0 || messageBody.slice(offset - 2, offset) === '\n\n';
    const endsAtBoundary = end === messageBody.length || messageBody.slice(end, end + 2) === '\n\n';
    if (startsAtBoundary && endsAtBoundary) return true;
    offset = messageBody.indexOf(segmentBody, offset + 1);
  }
  return false;
}

/**
 * Hash only content Pi actually retains after compaction. The public Pi builder
 * owns branch/compaction/firstKeptEntryId interpretation; full history is never
 * treated as retained context.
 */
export function collectPiRetainedContentDigests(
  ctx: Pick<PiContext, 'sessionManager'>,
  options: RetainedContextDigestOptions = {},
): Set<string> {
  const manager = ctx.sessionManager;
  const branch = manager?.getBranch?.();
  const digests = new Set<string>();
  for (const content of options.additionalRetainedContents ?? []) {
    if (content.length > 0) digests.add(contentDigest(content));
  }
  if (!Array.isArray(branch)) return digests;

  const retained = buildSessionContext(
    branch as Parameters<typeof buildSessionContext>[0],
    manager?.getLeafId?.() ?? undefined,
  ).messages;
  for (const message of retained as Array<{ content?: unknown; customType?: unknown; details?: unknown }>) {
    const bodies = textBodies(message.content).filter((body) => body.length > 0);
    for (const body of bodies) digests.add(contentDigest(body));

    // Combined Octocode context messages can contain several segment bodies.
    // Trust only our typed manifest plus current owner bytes, and require the
    // bytes to occupy an exact joined-segment slot rather than any substring.
    if (message.customType !== 'octocode-context-update' || !message.details || typeof message.details !== 'object') continue;
    const segments = (message.details as { segments?: unknown }).segments;
    if (!Array.isArray(segments)) continue;
    for (const segment of segments) {
      if (!segment || typeof segment !== 'object') continue;
      const id = (segment as { id?: unknown }).id;
      const digest = (segment as { digest?: unknown }).digest;
      if (typeof id !== 'string' || typeof digest !== 'string') continue;
      const known = options.knownSegmentContents?.[id];
      if (typeof known !== 'string' || contentDigest(known) !== digest) continue;
      if (bodies.some((body) => exactJoinedSegment(body, known))) digests.add(digest);
    }
  }
  return digests;
}
