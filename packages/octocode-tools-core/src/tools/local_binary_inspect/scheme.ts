import { z } from 'zod';
import { LocalBinaryInspectQuerySchema as CoreSchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';

// Override fields that need tighter bounds at the MCP layer
const queryOverrides = {
  entryPageNumber: relaxedPageNumberField.default(1),
  matchStringContextLines: clampedInt(0, 50).default(3),
  charLength: clampedInt(1, 50_000).optional(),
  page: relaxedPageNumberField.default(1),
} as const;

// Build the shape as a plain ZodObject (no superRefine) for bulk schema use
const LocalBinaryInspectQueryShape = z.object({
  ...CoreSchema.shape,
  ...queryOverrides,
});

// Full schema with superRefine from core re-applied
export const LocalBinaryInspectQuerySchema =
  LocalBinaryInspectQueryShape.strict().superRefine((q, ctx) => {
    if (q.mode === 'extract' && !q.archiveFile) {
      ctx.addIssue({
        code: 'custom',
        path: ['archiveFile'],
        message:
          'archiveFile is required for mode="extract" — run mode="list" first to get exact entry names',
      });
    }
    if (q.archiveFile?.startsWith('-')) {
      ctx.addIssue({
        code: 'custom',
        path: ['archiveFile'],
        message:
          'archiveFile must not start with "-" (prevents flag injection into backend CLIs)',
      });
    }
    const ARCHIVE_EXTS = [
      '.tar.gz',
      '.tgz',
      '.tar.bz2',
      '.tbz2',
      '.tbz',
      '.tar.xz',
      '.txz',
      '.tar.zst',
      '.tzst',
      '.zip',
      '.jar',
      '.war',
      '.apk',
      '.7z',
    ];
    if (
      q.mode === 'decompress' &&
      ARCHIVE_EXTS.some(ext => q.path.toLowerCase().endsWith(ext))
    ) {
      ctx.addIssue({
        code: 'custom',
        path: ['mode'],
        message:
          'This path looks like a multi-entry archive — use mode="list" or mode="extract" instead of mode="decompress".',
      });
    }
  });

export type BinaryInspectQuery = z.infer<typeof LocalBinaryInspectQuerySchema>;

export const LocalBinaryInspectBulkQuerySchema = createRelaxedBulkQuerySchema(
  LocalBinaryInspectQueryShape,
  { maxQueries: 5 }
);
