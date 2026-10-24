// The top-level page that owns a target: iframe targets are followed up their parentId chain.

/** The fields of a CDP Target.TargetInfo the owner walk reads. */
type TargetInfoRow = { targetId: string; type: string; parentId?: string };

/** The page target owning start (start itself when it is a page); null when the chain ends elsewhere. */
export function pageOwner<T extends TargetInfoRow>(
  targetInfos: T[],
  start: T | undefined
): T | null {
  const byId = new Map<string | undefined, T>(
    targetInfos.map(info => [info.targetId, info])
  );
  const seen = new Set<string>();
  let owner = start;
  while (owner?.type === 'iframe' && !seen.has(owner.targetId)) {
    seen.add(owner.targetId);
    owner = byId.get(owner.parentId);
  }
  return owner?.type === 'page' ? owner : null;
}
