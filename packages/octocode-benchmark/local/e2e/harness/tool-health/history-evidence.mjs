import { inventoryRows, unchangedContext } from '../responses.mjs';
import { canon } from './calls.mjs';

/** A repeated file wrapper is context; numbered patch lines remain evidence. */
export function historyFileRows(data, json, context = new Map()) {
  const row = json?.results?.find(result => result.data === data);
  return (data?.pullRequests ?? []).flatMap(pr => inventoryRows(pr.files).flatMap(file => {
    if (file.patch === undefined) return file.samePatchAs
      ? [`shared-patch:${pr.number}:${file.path}:${file.samePatchAs}`]
      : [`${pr.number}:${file.path}:${file.status}`];
    const { patch, patchPagination: _pagination, ...metadata } = file;
    const key = canon([row?.index ?? 0, pr.owner, pr.repo, pr.number, pr.baseSha, pr.headSha, file.path]);
    const original = context.get(key);
    const continued = file.patchPagination?.offset > 0 || row?.rowPart?.part > 1;
    const repeated = continued && unchangedContext(metadata, original);
    if (!original) context.set(key, metadata);
    return [...(repeated ? [] : [`patch-file:${key}:${canon(metadata)}`]),
      ...patch.split(/(?<=\n)/).filter(Boolean).map(line => `patch-line:${key}:${canon(line)}`)];
  }));
}
