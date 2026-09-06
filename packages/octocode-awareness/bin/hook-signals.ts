export interface HookSignalItem {
  kind: string;
}

export function briefingChangeSignal(items: HookSignalItem[], maintenanceChanged = false): string {
  const counts = new Map<string, number>();
  for (const item of items) {
    const category = item.kind === 'notification'
      ? 'messages'
      : item.kind === 'memory'
        ? 'memory'
        : item.kind === 'refinement'
          ? 'refinements'
          : 'maintenance';
    counts.set(category, (counts.get(category) ?? 0) + 1);
  }
  if (maintenanceChanged) counts.set('maintenance', Math.max(1, counts.get('maintenance') ?? 0));
  const detail = ['messages', 'memory', 'refinements', 'maintenance']
    .flatMap((key) => counts.has(key) ? [`${key} ${counts.get(key)}`] : []);
  return detail.length ? `Awareness: ${detail.join(', ')}.` : 'Awareness: state changed.';
}

export function overlapChangeSignal(pathCount: number): string {
  return `Awareness: overlap changed (${pathCount} path${pathCount === 1 ? '' : 's'}).`;
}

export function verificationDebtSignal(count: number): string {
  return `Awareness: verification debt (${count}).`;
}
