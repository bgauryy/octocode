import { z } from 'zod';

interface FileContentExtractionFields {
  fullContent?: boolean;
  matchString?: string;
  startLine?: number;
  endLine?: number;
}

export function validateFileContentExtractionMode(
  data: FileContentExtractionFields,
  ctx: z.RefinementCtx
): void {
  const hasFullContent = data.fullContent === true;
  const hasMatchString = data.matchString !== undefined;
  const hasLineRange =
    data.startLine !== undefined || data.endLine !== undefined;

  if (hasFullContent && hasMatchString) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use fullContent with matchString — these are mutually exclusive extraction methods. Choose ONE: fullContent=true to read the entire file, OR matchString to extract matching sections, OR startLine+endLine for a known line range.',
      path: ['matchString'],
    });
  }

  if (hasFullContent && hasLineRange) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use fullContent with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: fullContent=true to read the entire file, OR startLine+endLine for a known line range, OR matchString to extract matching sections.',
      path: ['startLine'],
    });
  }

  if (hasMatchString && hasLineRange) {
    ctx.addIssue({
      code: 'custom',
      message:
        'Cannot use matchString with startLine/endLine — these are mutually exclusive extraction methods. Choose ONE: matchString to extract matching sections, OR startLine+endLine for a known line range, OR fullContent=true to read the entire file.',
      path: ['startLine'],
    });
  }
}
