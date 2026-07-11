import type { MarketplaceSource } from '../../configs/skills-marketplace.js';
import { MAX_CONTENT_SIZE_BYTES } from './constants.js';
import type { GitHubTreeResponse, GitHubTreeItem } from './types.js';

export async function fetchMarketplaceTree(
  source: MarketplaceSource
): Promise<GitHubTreeItem[]> {
  const apiUrl = `https://api.github.com/repos/${source.owner}/${source.repo}/git/trees/${source.branch}?recursive=1`;

  const response = await fetch(apiUrl, {
    headers: {
      Accept: 'application/vnd.github.v3+json',
      'User-Agent': 'octocode',
    },
  });

  if (!response.ok) {
    if (response.status === 403) {
      throw new Error('GitHub API rate limit exceeded. Try again later.');
    }
    throw new Error(`Failed to fetch marketplace: ${response.statusText}`);
  }

  const data = (await response.json()) as GitHubTreeResponse;

  if (data.truncated) {
    console.warn(
      `[octocode] GitHub tree response was truncated for ${source.owner}/${source.repo}. Some skills may be missing.`
    );
  }

  return data.tree;
}

export async function fetchRawContent(
  source: MarketplaceSource,
  path: string
): Promise<string> {
  const rawUrl = `https://raw.githubusercontent.com/${source.owner}/${source.repo}/${source.branch}/${path}`;

  const response = await fetch(rawUrl, {
    headers: {
      'User-Agent': 'octocode',
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch content: ${response.statusText}`);
  }

  const contentLength = response.headers?.get?.('Content-Length');
  const contentLengthBytes = contentLength ? Number(contentLength) : NaN;
  if (
    Number.isFinite(contentLengthBytes) &&
    contentLengthBytes > MAX_CONTENT_SIZE_BYTES
  ) {
    throw new Error(
      `Content too large: ${contentLengthBytes} bytes exceeds ${MAX_CONTENT_SIZE_BYTES} byte limit`
    );
  }

  const content = await response.text();
  const contentSizeBytes = Buffer.byteLength(content, 'utf8');

  if (contentSizeBytes > MAX_CONTENT_SIZE_BYTES) {
    throw new Error(
      `Content too large: ${contentSizeBytes} bytes exceeds ${MAX_CONTENT_SIZE_BYTES} byte limit`
    );
  }

  return content;
}
