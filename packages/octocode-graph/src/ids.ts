// Node ids of a saved graph: a file is its root-relative path, a symbol `file#name`, a workspace package
// `pkg:<name>`. One home for the id arithmetic every layer shares (runtime, query engine, viewer).

/** The file a node id belongs to: a symbol's file, or a file's own id. */
export const fileOf = (id: string) => id.split('#')[0];

/** The id prefix of a root-relative directory (a chunk's path): empty for the root, else `path/`. */
export const prefixOf = (path: string) => (path === '.' ? '' : `${path}/`);

/** The directory a saved graph's ids are relative to: its `section` (absolute), or '' for a graph without one. */
export function rootOf(D: { section?: unknown }) {
  if (typeof D.section !== 'string') return '';
  let end = D.section.length;
  while (end > 0 && D.section[end - 1] === '/') end--;
  return D.section.slice(0, end);
}

/** The absolute path of a root-relative path: the saved graph stores each path once, relative to its root. */
export const absOf = (root: string, path: string) =>
  root ? `${root}/${path}` : path;
