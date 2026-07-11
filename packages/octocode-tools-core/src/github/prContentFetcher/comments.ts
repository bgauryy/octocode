import { PRCommentItem, PRReviewInfo, IssueComment } from '../githubAPI.js';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { contextUtils } from '../../utils/contextUtils.js';
import { OctokitWithThrottling } from '../client.js';
import { isBotAuthor } from '../botFilter.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../utils/response/charSavings.js';
import { stripMachineBlobs } from './flags.js';
import { fetchAllPaginated } from './commits.js';

export async function fetchPRComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false
): Promise<{ comments: PRCommentItem[]; note?: string }> {
  try {
    const raw: IssueComment[] = [];
    let rawResponseChars = 0;
    let page = 1;
    let keepFetching = true;
    do {
      const commentsResult = await octokit.rest.issues.listComments({
        owner,
        repo,
        issue_number: prNumber,
        per_page: 100,
        page,
      });
      rawResponseChars += countSerializedChars(commentsResult.data);
      raw.push(...commentsResult.data);
      keepFetching = commentsResult.data.length === 100;
      page++;
    } while (keepFetching);

    const kept = includeBots
      ? raw
      : raw.filter((c: IssueComment) => !isBotAuthor(c.user?.login ?? ''));
    const botsDropped = raw.length - kept.length;

    const comments = kept.map((comment: IssueComment): PRCommentItem => {
      const stripped = contextUtils.minifyMarkdownCore(
        stripMachineBlobs(comment.body ?? '')
      );
      return {
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(stripped).content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
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
      comments: attachRawResponseChars(comments, rawResponseChars),
      note: notes.length > 0 ? notes.join('; ') : undefined,
    };
  } catch {
    return { comments: attachRawResponseChars([], 0) };
  }
}

export async function fetchPRReviews(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number
): Promise<PRReviewInfo[]> {
  try {
    const { items, rawResponseChars } = await fetchAllPaginated<
      Awaited<ReturnType<typeof octokit.rest.pulls.listReviews>>['data'][number]
    >(
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
        }>
    );

    return attachRawResponseChars(
      items.map(review => ({
        id: String(review.id),
        user: review.user?.login ?? 'unknown',
        state: review.state ?? '',
        body: ContentSanitizer.sanitizeContent(
          contextUtils.minifyMarkdownCore(stripMachineBlobs(review.body ?? ''))
        ).content,
        submitted_at: review.submitted_at ?? undefined,
        commit_id: review.commit_id ?? undefined,
      })),
      rawResponseChars
    );
  } catch {
    return attachRawResponseChars([], 0);
  }
}

export async function fetchPRInlineComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false
): Promise<{ comments: PRCommentItem[]; note?: string }> {
  try {
    type ReviewComment = Awaited<
      ReturnType<typeof octokit.rest.pulls.listReviewComments>
    >['data'][number];

    const raw: ReviewComment[] = [];
    let rawResponseChars = 0;
    let page = 1;
    let keepFetching = true;
    do {
      const result = await octokit.rest.pulls.listReviewComments({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      });
      rawResponseChars += countSerializedChars(result.data);
      raw.push(...result.data);
      keepFetching = result.data.length === 100;
      page++;
    } while (keepFetching);

    const kept = includeBots
      ? raw
      : raw.filter((c: ReviewComment) => !isBotAuthor(c.user?.login ?? ''));
    const botsDropped = raw.length - kept.length;

    const comments = kept.map((comment: ReviewComment): PRCommentItem => {
      const stripped = contextUtils.minifyMarkdownCore(
        stripMachineBlobs(comment.body ?? '')
      );
      return {
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(stripped).content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
        commentType: 'review_inline',
        path: comment.path,
        line: comment.line ?? comment.original_line ?? undefined,
        ...(comment.in_reply_to_id != null
          ? { in_reply_to_id: comment.in_reply_to_id }
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
      comments: attachRawResponseChars(comments, rawResponseChars),
      note: notes.length > 0 ? notes.join('; ') : undefined,
    };
  } catch {
    return { comments: attachRawResponseChars([], 0) };
  }
}
