import type { UxPriority, UxSnapshotV1 } from '../tools/ux-snapshot.js';
import { formatDurationShort } from '../ui-extras.js';
import type { InlineSegment } from './components.js';

export interface StatusCandidate {
  id: string;
  priority: UxPriority;
  order: number;
  segments: InlineSegment[];
  detailRoute?: string;
}

export function backgroundCandidates(snapshot: UxSnapshotV1): StatusCandidate[] {
  const running = snapshot.backgroundJobs.filter((job) => job.status === 'running');
  if (running.length === 0) return [];
  const first = running[0]!;
  return [{
    id: 'background:running',
    priority: 'P2',
    order: 1_900,
    segments: [
      { text: `${running.length} background job${running.length === 1 ? '' : 's'} running`, token: 'brand' },
      { text: first.title, token: 'muted' },
      ...(running.length > 1 ? [{ text: `+${running.length - 1}`, token: 'dim' as const }] : []),
      { text: formatDurationShort(first.elapsedMs), token: 'dim' },
      { text: '/octocode-status', token: 'link' },
    ],
    detailRoute: '/octocode-status',
  }];
}
