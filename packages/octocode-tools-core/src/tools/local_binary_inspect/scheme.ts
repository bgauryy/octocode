import { z } from 'zod';
import { LocalBinaryInspectQuerySchema as CoreSchema } from '@octocodeai/octocode-core/schemas';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { bulkOutputEnvelopeFields } from '../../scheme/responseEnvelope.js';
import {
  CharPaginationSchema,
  ItemPaginationSchema,
} from '../../scheme/pagination.js';

// Override fields that need tighter bounds at the MCP layer. Each carries a
// description so the override doesn't blank out the core field's docs in
// `--scheme`.
const queryOverrides = {
  entryPageNumber: relaxedPageNumberField
    .default(1)
    .describe(
      'list mode: 1-based page over archive entries when an archive has many files.'
    ),
  matchStringContextLines: clampedInt(0, 50)
    .default(3)
    .describe(
      'Lines of context to keep around each matchString hit (strings/decompress/extract).'
    ),
  charLength: clampedInt(1, 50_000)
    .optional()
    .describe(
      'Max chars of inline content for this window (paired with charOffset). Omit for the default window; follow pagination.next to page losslessly.'
    ),
  page: relaxedPageNumberField
    .default(1)
    .describe('1-based page for paginated entry/content listings.'),
  detailed: z
    .boolean()
    .optional()
    .describe('inspect: include full symbols/imports/exports/sections arrays.'),
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

// ---------------------------------------------------------------------------
// Output schema — describes what localBinaryInspect returns per mode.
//
// Mode-specific data shapes:
//   inspect  → binary metadata (format, size, sections, symbols)
//   list     → archive entries with item pagination
//   extract  → text content with char pagination
//   decompress → text content with char pagination
//   strings  → extracted strings with scan offset
//   unpack   → localPath to unpacked directory
// ---------------------------------------------------------------------------

// inspect mode
const BinaryInspectDataSchema = z.object({
  path: z.string(),
  mode: z.literal('inspect'),
  format: z.string().optional(),
  size: z.number().optional(),
  isText: z.boolean().optional(),
  encoding: z.string().optional(),
  symbols: z.array(z.string()).optional(),
  imports: z.array(z.string()).optional(),
  exports: z.array(z.string()).optional(),
  sections: z.array(z.string()).optional(),
  warnings: z.array(z.string()).optional(),
});

// list mode
const ArchiveEntrySchema = z.object({
  name: z.string(),
  size: z.number().optional(),
  compressedSize: z.number().optional(),
  isDir: z.boolean().optional(),
  modified: z.string().optional(),
});

const BinaryListDataSchema = z.object({
  path: z.string(),
  mode: z.literal('list'),
  entries: z.array(ArchiveEntrySchema).optional(),
  // Total entry count is in pagination.totalItems — no separate alias here.
  pagination: ItemPaginationSchema.optional(),
  warnings: z.array(z.string()).optional(),
});

// extract / decompress modes
const BinaryContentDataSchema = z.object({
  path: z.string(),
  mode: z.enum(['extract', 'decompress']),
  archiveFile: z.string().optional(),
  content: z.string().optional(),
  isPartial: z.boolean().optional(),
  pagination: CharPaginationSchema.optional(),
  warnings: z.array(z.string()).optional(),
});

// strings mode
const BinaryStringsDataSchema = z.object({
  path: z.string(),
  mode: z.literal('strings'),
  strings: z
    .array(
      z.object({
        value: z.string(),
        offset: z.number().optional(),
      })
    )
    .optional(),
  content: z.string().optional(),
  totalStrings: z.number().optional(),
  scanOffset: z.number().optional(),
  nextScanOffset: z.number().optional(),
  hasMore: z.boolean().optional(),
  warnings: z.array(z.string()).optional(),
});

// unpack mode
const BinaryUnpackDataSchema = z.object({
  path: z.string(),
  mode: z.literal('unpack'),
  localPath: z.string().optional(),
  fileCount: z.number().optional(),
  totalSize: z.number().optional(),
  warnings: z.array(z.string()).optional(),
});

// Union of all mode data shapes
const LocalBinaryInspectDataSchema = z.union([
  BinaryInspectDataSchema,
  BinaryListDataSchema,
  BinaryContentDataSchema,
  BinaryStringsDataSchema,
  BinaryUnpackDataSchema,
]);

export const LocalBinaryInspectOutputSchema = z
  .object({
    results: z.array(
      z.object({
        id: z.string(),
        // binaryInspector emits a 'success' status on completed inspections (in
        // addition to the empty/error the bulk layer sets), so the row status
        // enum must accept it.
        status: z.enum(['empty', 'error', 'success']).optional(),
        data: LocalBinaryInspectDataSchema,
      })
    ),
  })
  .extend(bulkOutputEnvelopeFields);

export type LocalBinaryInspectOutput = z.infer<
  typeof LocalBinaryInspectOutputSchema
>;
