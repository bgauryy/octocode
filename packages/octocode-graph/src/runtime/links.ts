// Every link beside the code graph, assembled in one place for a build and a refresh: docs (links-docs.ts),
// manifests and tsconfig (links-files.ts), contract entities (links-contracts.ts) and agents' proposals
// (links-proposed.ts), each checked against the files again. The metrics never count links.
import { contractEntities, contractLinks } from './links-contracts.ts';
import { docLinks } from './links-docs.ts';
import { fileLinks } from './links-files.ts';
import { asLinks, recheckProposed } from './links-proposed.ts';
import type { GraphData, Link, UnresolvedLink } from './types.ts';

/** The links and unresolved references of a graph's listing; `previous` keeps a citation's staleness across runs. */
export async function readLinks(
  D: GraphData,
  root: string,
  listing: readonly string[],
  previous: readonly Link[],
  out: string
): Promise<{ links: Link[]; unresolved: UnresolvedLink[] }> {
  const entities = contractEntities(root, listing);
  const docs = docLinks(D, root, listing, previous, entities);
  const manifests = fileLinks(root, listing);
  const contracts = await contractLinks(
    root,
    new Set(D.files.map(f => f.id)),
    entities
  );
  return {
    links: [
      ...docs.links,
      ...manifests.links,
      ...contracts.links,
      ...asLinks(recheckProposed(out, root)),
    ],
    unresolved: [
      ...docs.unresolved,
      ...manifests.unresolved,
      ...contracts.unresolved,
    ],
  };
}
