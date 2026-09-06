import { PRCommentItem, PRReviewInfo, IssueComment } from '../githubAPI.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { OctokitWithThrottling } from '../client.js';
import type { AuthInfo } from '@modelcontextprotocol/server';
import { isBotAuthor } from '../botFilter.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import {
  fetchCollectionPage,
  type CollectionArray,
} from './collectionPaging.js';

export async function fetchPRComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false,
  authInfo?: AuthInfo,
  collectionPage = 1
): Promise<{ comments: CollectionArray<PRCommentItem>; note?: string }> {
  const {
    items: raw,
    rawResponseChars,
    collectionState,
  } = await fetchCollectionPage<IssueComment>(
    { owner, repo, prNumber, surface: 'discussion' },
    collectionPage,
    page =>
      octokit.rest.issues.listComments({
        owner,
        repo,
        issue_number: prNumber,
        per_page: 100,
        page,
      }),
    authInfo
  );

  const kept = includeBots
    ? raw
    : raw.filter((c: IssueComment) => !isBotAuthor(c.user?.login ?? ''));
  const botsDropped = raw.length - kept.length;

  const comments = kept.map((comment: IssueComment): PRCommentItem => {
    return {
      id: String(comment.id),
      user: comment.user?.login ?? 'unknown',
      body: ContentSanitizer.sanitizeContent(comment.body ?? '').content,
      createdAt: comment.created_at ?? '',
      updatedAt: comment.updated_at ?? '',
      commentType: 'discussion',
    };
  });

  const notes: string[] = [];
  if (botsDropped > 0) {
    notes.push(
      `${botsDropped} bot comment(s) hidden (set content.comments.includeBots:true to include)`
    );
  }

  return {
    comments: attachRawResponseChars(
      Object.assign(comments, { collectionState }),
      rawResponseChars
    ),
    note: notes.length > 0 ? notes.join('; ') : undefined,
  };
}

export async function fetchPRReviews(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo,
  collectionPage = 1
): Promise<CollectionArray<PRReviewInfo>> {
  const { items, rawResponseChars, collectionState } =
    await fetchCollectionPage<
      Awaited<ReturnType<typeof octokit.rest.pulls.listReviews>>['data'][number]
    >(
      { owner, repo, prNumber, surface: 'reviews' },
      collectionPage,
      page =>
        octokit.rest.pulls.listReviews({
          owner,
          repo,
          pull_number: prNumber,
          per_page: 100,
          page,
        }) as Promise<{
          data: Awaited<
            ReturnType<typeof octokit.rest.pulls.listReviews>
          >['data'];
        }>,
      authInfo
    );

  return attachRawResponseChars(
    Object.assign(
      items.map(review => ({
        id: String(review.id),
        user: review.user?.login ?? 'unknown',
        state: review.state ?? '',
        body: ContentSanitizer.sanitizeContent(review.body ?? '').content,
        submittedAt: review.submitted_at ?? undefined,
        commitId: review.commit_id ?? undefined,
      })),
      { collectionState }
    ),
    rawResponseChars
  );
}

export async function fetchPRInlineComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false,
  authInfo?: AuthInfo,
  collectionPage = 1
): Promise<{ comments: CollectionArray<PRCommentItem>; note?: string }> {
  type ReviewComment = Awaited<
    ReturnType<typeof octokit.rest.pulls.listReviewComments>
  >['data'][number];

  const {
    items: raw,
    rawResponseChars,
    collectionState,
  } = await fetchCollectionPage<ReviewComment>(
    { owner, repo, prNumber, surface: 'inline' },
    collectionPage,
    page =>
      octokit.rest.pulls.listReviewComments({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      }),
    authInfo
  );

  const kept = includeBots
    ? raw
    : raw.filter((c: ReviewComment) => !isBotAuthor(c.user?.login ?? ''));
  const botsDropped = raw.length - kept.length;

  const comments = kept.map((comment: ReviewComment): PRCommentItem => {
    return {
      id: String(comment.id),
      user: comment.user?.login ?? 'unknown',
      body: ContentSanitizer.sanitizeContent(comment.body ?? '').content,
      createdAt: comment.created_at ?? '',
      updatedAt: comment.updated_at ?? '',
      commentType: 'review_inline',
      path: comment.path,
      line: comment.line ?? comment.original_line ?? undefined,
      ...(comment.in_reply_to_id != null
        ? { inReplyToId: comment.in_reply_to_id }
        : {}),
    };
  });

  const notes: string[] = [];
  if (botsDropped > 0) {
    notes.push(
      `${botsDropped} bot inline comment(s) hidden (set content.comments.includeBots:true to include)`
    );
  }

  return {
    comments: attachRawResponseChars(
      Object.assign(comments, { collectionState }),
      rawResponseChars
    ),
    note: notes.length > 0 ? notes.join('; ') : undefined,
  };
}
