import { contextUtils } from '../contextUtils.js';

const MAX_SEMANTIC_EXTENSION = 8_000;
const GENERIC_BOUNDARY_FILE = '__octocode_generic__.unknown';

function resolveBoundaryFilePath(filePath: string | undefined): string {
  return filePath && filePath.trim().length > 0
    ? filePath
    : GENERIC_BOUNDARY_FILE;
}

async function getSemanticBoundaries(
  content: string,
  filePath?: string
): Promise<number[]> {
  const offsets = await contextUtils.getSemanticBoundaryOffsets(
    content,
    resolveBoundaryFilePath(filePath)
  );
  return offsets.filter(
    (offset): offset is number =>
      Number.isInteger(offset) && offset >= 0 && offset <= content.length
  );
}

function nextLineStart(content: string, fromChar: number): number | undefined {
  const lineBreak = content.indexOf('\n', fromChar);
  return lineBreak === -1 ? undefined : lineBreak + 1;
}

export function isMidBlockCut(paginatedContent: string): boolean {
  const lastMeaningfulLine =
    paginatedContent.trimEnd().split('\n').at(-1) ?? '';
  return (
    lastMeaningfulLine.length > 0 &&
    (lastMeaningfulLine[0] === ' ' || lastMeaningfulLine[0] === '\t')
  );
}

export async function findNextBlockBoundary(
  content: string,
  fromChar: number,
  filePath?: string
): Promise<number | undefined> {
  const searchStart = nextLineStart(content, Math.max(0, fromChar));
  if (searchStart === undefined) return undefined;
  const boundaries = await getSemanticBoundaries(content, filePath);
  return boundaries.find(offset => offset >= searchStart && offset > fromChar);
}

export async function buildBlockBoundaryHint(
  paginatedContent: string,
  fullContent: string,
  cutPos: number,
  currentCharLength: number,
  filePath?: string
): Promise<{ nextBlockChar: number; hint: string } | undefined> {
  if (!isMidBlockCut(paginatedContent)) return undefined;

  const nextBlockChar = await findNextBlockBoundary(
    fullContent,
    cutPos,
    filePath
  );
  if (nextBlockChar === undefined) return undefined;

  const extendBy = nextBlockChar - cutPos;
  const hint =
    `Page cut mid-block at char ${cutPos}. ` +
    `Next top-level definition at char ${nextBlockChar}. ` +
    `Re-request with charLength=${currentCharLength + extendBy} to extend this page to the next boundary, ` +
    `or use charOffset=${cutPos} to continue page-by-page.`;

  return { nextBlockChar, hint };
}

export type ChunkMode = 'semantic' | 'char-limit';

export async function snapToSemanticBoundary(
  content: string,
  charOffset: number,
  charLength: number,
  filePath?: string
): Promise<{ length: number; chunkMode: ChunkMode }> {
  const safeOffset = Math.min(Math.max(0, charOffset), content.length);
  const safeLength = Math.max(1, charLength);
  const idealEnd = safeOffset + safeLength;

  if (idealEnd >= content.length) {
    return { length: content.length - safeOffset, chunkMode: 'char-limit' };
  }

  const boundaries = await getSemanticBoundaries(content, filePath);
  if (boundaries.length === 0) {
    return { length: safeLength, chunkMode: 'char-limit' };
  }

  const nextBoundary = boundaries.find(b => b > idealEnd);

  if (nextBoundary === undefined) {
    return { length: safeLength, chunkMode: 'char-limit' };
  }

  const extension = nextBoundary - idealEnd;

  if (extension <= MAX_SEMANTIC_EXTENSION) {
    return { length: nextBoundary - safeOffset, chunkMode: 'semantic' };
  }

  return { length: safeLength, chunkMode: 'char-limit' };
}
