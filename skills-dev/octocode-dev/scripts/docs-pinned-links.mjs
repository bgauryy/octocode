import { execFileSync } from 'node:child_process';

// A link pinned to a commit of this repository must name a commit and path
// that exist in the local history. Other repositories' pins are not checkable here.
const PINNED_REPO_LINK = /^https:\/\/github\.com\/bgauryy\/octocode(?:-mcp)?\/blob\/([0-9a-f]{7,40})\/([^#?]+)/;

export function gitHasPath(root, sha, filePath) {
  try {
    execFileSync('git', ['cat-file', '-e', `${sha}:${filePath}`], { cwd: root, stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

// Failure messages for the pinned links among `links` of `relativeFile`.
export function pinnedLinkFailures(relativeFile, links, hasPath) {
  const failures = [];
  for (const link of links) {
    const match = PINNED_REPO_LINK.exec(link);
    if (!match) continue;
    const [, sha, filePath] = match;
    const decoded = decodeURIComponent(filePath).replace(/\/$/, '');
    if (!hasPath(sha, decoded)) {
      failures.push(`${relativeFile} pins "${decoded}" at commit ${sha}, which is not in the local history`);
    }
  }
  return failures;
}
