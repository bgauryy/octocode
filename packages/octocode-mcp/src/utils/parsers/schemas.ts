import { z } from 'zod';

const RipgrepPathSchema = z.object({ text: z.string() });

const RipgrepSubmatchSchema = z.object({
  match: z.object({ text: z.string() }).optional(),
  start: z.number(),
  end: z.number(),
});

const RipgrepJsonMatchSchema = z.object({
  type: z.literal('match'),
  data: z.object({
    path: RipgrepPathSchema,
    lines: z.object({ text: z.string() }),
    line_number: z.number(),
    absolute_offset: z.number(),
    submatches: z.array(RipgrepSubmatchSchema),
  }),
});

const RipgrepJsonContextSchema = z.object({
  type: z.literal('context'),
  data: z.object({
    path: RipgrepPathSchema,
    lines: z.object({ text: z.string() }),
    line_number: z.number(),
    absolute_offset: z.number(),
  }),
});

const RipgrepJsonBeginSchema = z.object({
  type: z.literal('begin'),
  data: z.object({ path: RipgrepPathSchema }),
});

const RipgrepJsonEndSchema = z.object({
  type: z.literal('end'),
  data: z.object({
    path: RipgrepPathSchema,
    stats: z
      .object({
        elapsed: z.object({ human: z.string() }),
        searches: z.number(),
        searches_with_match: z.number(),
      })
      .optional(),
  }),
});

const RipgrepJsonSummarySchema = z.object({
  type: z.literal('summary'),
  data: z.object({
    elapsed_total: z.object({ human: z.string() }),
    stats: z.object({
      elapsed: z.object({ human: z.string() }),
      searches: z.number(),
      searches_with_match: z.number(),
      bytes_searched: z.number(),
      bytes_printed: z.number(),
      matched_lines: z.number(),
      matches: z.number(),
    }),
  }),
});

export const RipgrepJsonMessageSchema = z.discriminatedUnion('type', [
  RipgrepJsonMatchSchema,
  RipgrepJsonContextSchema,
  RipgrepJsonBeginSchema,
  RipgrepJsonEndSchema,
  RipgrepJsonSummarySchema,
]);
