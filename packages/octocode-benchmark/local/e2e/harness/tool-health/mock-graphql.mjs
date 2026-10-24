// The GitHub GraphQL answers of the tool-health mock: the history documents
// ghGetHistoryItem sends, answered from the same model as REST.
import { USER, addedLines, commitByRef, findRepo, textOf } from './mock-model.mjs';

const connection = (first, nodes, after = '0') => {
  const offset = Number(after);
  return { pageInfo: { hasNextPage: nodes.length > offset + first, endCursor: String(offset + first) }, nodes: nodes.slice(offset, offset + first) };
};
const notFound = (kind, number) => ({ type: 'NOT_FOUND', message: `Could not resolve to a${kind === 'Issue' ? 'n' : ''} ${kind} with the number of ${number}.` });

/** A pull request node with only the connections whose page-size variable the document binds. */
function pullRequestNode(model, repo, pr, variables) {
  const changed = pr.head.added.map(fp => ({ path: fp, additions: addedLines(textOf(model, pr.head, fp)).length, deletions: 0, changeType: 'ADDED' }));
  const node = {
    number: pr.number, title: pr.title, url: `https://github.com/${repo.owner}/${repo.name}/pull/${pr.number}`,
    state: pr.merged ? 'MERGED' : pr.state.toUpperCase(), body: pr.body, isDraft: false, author: { login: USER.login, __typename: 'User' },
    labels: { pageInfo: { hasNextPage: false }, nodes: [] },
    baseRefName: 'main', baseRefOid: pr.base.oid, headRefName: `feature-${pr.number}`, headRefOid: pr.head.oid,
    createdAt: pr.date, updatedAt: pr.date, closedAt: pr.state === 'closed' ? pr.date : null, mergedAt: pr.merged ? pr.date : null,
    mergeCommit: pr.merged ? { oid: pr.head.oid } : null,
    comments: { totalCount: 0 }, changedFiles: changed.length, additions: changed.reduce((n, f) => n + f.additions, 0), deletions: 0,
    commitsCount: { totalCount: 1 }, reviewThreads: { totalCount: pr.reviewComments?.length ?? 0 },
  };
  if (variables.files !== undefined) node.files = connection(variables.files, changed);
  if (variables.discussion !== undefined) node.commentsConn = connection(variables.discussion, []);
  if (variables.reviews !== undefined) node.reviews = connection(variables.reviews, (pr.reviews ?? []).map(review => ({ databaseId: review.id, body: review.body, state: review.state, submittedAt: review.date, author: { login: USER.login, __typename: 'User' }, commit: { oid: pr.head.oid } })), variables.after);
  if (variables.commits !== undefined) {
    const c = pr.head;
    node.commits = connection(variables.commits, [{ commit: { oid: c.oid, message: c.message, messageHeadline: c.message.split('\n')[0], authoredDate: c.date, author: { name: 'Octo Dev', user: { login: USER.login } } } }]);
  }
  return node;
}

/** Last path change at the requested immutable commit, from the REST model. */
function pathDates(model, variables) {
  const repo = findRepo(model, variables.owner, variables.name);
  const commit = commitByRef(model, variables.oid);
  if (!repo || !commit) return { data: { repository: repo ? { object: null } : null } };
  const history = [];
  for (let current = commit; current; current = current.parent ? commitByRef(model, current.parent) : null) history.push(current);
  const object = { committedDate: commit.date };
  for (const [alias, path] of Object.entries(variables).filter(([key]) => /^p\d+$/.test(key))) {
    const matches = name => name === path || name.startsWith(`${path}/`);
    const last = repo.files === model.files
      ? history.find(entry => entry.added.some(matches))
      : Object.keys(repo.files).some(matches) ? commit : null;
    object[alias] = { nodes: last ? [{ committedDate: last.date, author: { name: 'Octo Dev', user: { login: USER.login } } }] : [] };
  }
  return { data: { repository: { object } } };
}

/**
 * The answer for one history document: a pull request's first page of
 * collections (`repository.pullRequest`) or an issue's closing references
 * (`closedByPullRequestsReferences`). `null` for any other document (left unhandled).
 */
export function graphqlAnswer(model, query, variables) {
  if (/object\(oid:\$oid\)/.test(query)) return pathDates(model, variables);
  const repo = findRepo(model, variables.owner, variables.repo);
  const demo = repo === model.repos[0];
  if (/pullRequest\(number:\$number\)/.test(query)) {
    const pr = demo && model.pulls.find(x => x.number === variables.number);
    if (!pr) return { data: { repository: repo ? { pullRequest: null } : null }, errors: [notFound('PullRequest', variables.number)] };
    return { data: { repository: { pullRequest: pullRequestNode(model, repo, pr, variables) } } };
  }
  if (/closedByPullRequestsReferences/.test(query)) {
    const issue = demo && model.issues.find(x => x.number === variables.number);
    if (!issue) return { data: { repository: repo ? { issue: null } : null }, errors: [notFound('Issue', variables.number)] };
    // No model pull request closes an issue: an empty, complete reference set.
    const references = variables.first === undefined ? { totalCount: 0 } : { totalCount: 0, pageInfo: { hasNextPage: false }, nodes: [] };
    return { data: { repository: { issue: { closedByPullRequestsReferences: references } } } };
  }
  return null;
}
