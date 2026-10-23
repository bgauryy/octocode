// The top-level page that owns a target: iframe targets are followed up their parentId chain.

/** The page target owning start (start itself when it is a page); null when the chain ends elsewhere. */
export function pageOwner(targetInfos, start) {
  const byId = new Map(targetInfos.map(info => [info.targetId, info]));
  const seen = new Set();
  let owner = start;
  while (owner?.type === 'iframe' && !seen.has(owner.targetId)) {
    seen.add(owner.targetId);
    owner = byId.get(owner.parentId);
  }
  return owner?.type === 'page' ? owner : null;
}
