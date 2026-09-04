import { describe, expect, it, vi } from "vitest";
import { Type } from "typebox";
import {
  buildQueryEnvelopeSchema,
  executeQueryBatch,
  prepareQueryBatch,
  QueryBatchError,
} from "../src/tools/query-envelope.js";
import type { ToolCallResult } from "../src/types.js";

const typeBuilder = Type as unknown as (typeof import("typebox"))["Type"];

function textResult(
  text: string,
  details?: unknown,
  isError = false,
): ToolCallResult {
  return { content: [{ type: "text", text }], details, isError };
}

describe("query envelope", () => {
  it("builds the query contract with an explicit sequential execution policy", () => {
    const schema = buildQueryEnvelopeSchema(
      typeBuilder,
      Type.Object({ value: Type.String() }, { additionalProperties: false }),
    ) as {
      properties?: {
        queries?: {
          minItems?: number;
          maxItems?: number;
          items?: {
            properties?: Record<
              string,
              { minLength?: number; maxLength?: number }
            >;
            required?: string[];
          };
        };
        queryRunType?: {
          default?: string;
          enum?: string[];
          description?: string;
        };
      };
      required?: string[];
      additionalProperties?: boolean;
    };

    expect(Object.keys(schema.properties ?? {})).toEqual([
      "queries",
      "queryRunType",
    ]);
    expect(schema.required).toContain("queries");
    expect(schema.additionalProperties).toBe(false);
    expect(schema.properties?.queries?.minItems).toBe(1);
    expect(schema.properties?.queries?.maxItems).toBe(100);
    expect(
      schema.properties?.queries?.items?.properties?.reasoning,
    ).toMatchObject({
      minLength: 1,
      maxLength: 240,
    });
    expect(schema.properties?.queries?.items?.required).toEqual([
      "reasoning",
      "value",
    ]);
    expect(schema.properties?.queries?.items?.properties).toHaveProperty(
      "value",
    );
    expect(schema.properties?.queryRunType).toMatchObject({
      default: "sequential",
      enum: ["sequential"],
    });
    expect(schema.properties?.queryRunType?.description).toMatch(/one-by-one/i);
  });

  it("exposes parallel execution only when the tool opts in", () => {
    const schema = buildQueryEnvelopeSchema(
      typeBuilder,
      Type.Object({ value: Type.String() }, { additionalProperties: false }),
      { allowParallel: true },
    ) as { properties?: { queryRunType?: { enum?: string[] } } };

    expect(schema.properties?.queryRunType?.enum).toEqual([
      "sequential",
      "parallel",
    ]);
  });

  it("preflights every query before execution and rejects invalid reasoning", async () => {
    const preflight = vi.fn(
      async (query: Record<string, unknown>, index: number) => {
        if (query.value === "bad") throw new Error(`bad ${index}`);
      },
    );

    await expect(
      prepareQueryBatch(
        {
          queries: [
            { reasoning: "first", value: "ok" },
            { reasoning: "second", value: "bad" },
          ],
        },
        { preflight },
      ),
    ).rejects.toThrow(/queries\[1\].*bad 1/);
    expect(preflight).toHaveBeenCalledTimes(2);

    await expect(
      prepareQueryBatch({ queries: [{ reasoning: "   ", value: "ok" }] }),
    ).rejects.toThrow(/queries\[0\].*reasoning/);

    await expect(
      prepareQueryBatch({
        queries: [{ reasoning: "x".repeat(241), value: "ok" }],
      }),
    ).rejects.toThrow(/at most 240/);
  });

  it("executes prepared queries in order and returns every child content block to the agent by default", async () => {
    const events: unknown[] = [];
    const execute = vi.fn(
      async (
        query: Record<string, unknown>,
        index: number,
        _itemId: string,
      ) => {
        return textResult(`done ${String(query.value)}`, { index });
      },
    );

    const result = await executeQueryBatch({
      toolCallId: "call-1",
      raw: {
        queries: [
          { reasoning: "run one", value: "a" },
          { reasoning: "run two", value: "b" },
        ],
      },
      execute,
      onUpdate: (update) => events.push(update),
    });

    expect(
      execute.mock.calls.map((call) => [call[0].value, call[1], call[2]]),
    ).toEqual([
      ["a", 0, "call-1:0"],
      ["b", 1, "call-1:1"],
    ]);
    expect(events).toHaveLength(2);
    expect(
      (
        result.details as {
          results: Array<{
            index: number;
            reasoning: string;
            status: string;
            summary: string;
          }>;
        }
      ).results,
    ).toMatchObject([
      { index: 0, reasoning: "run one", status: "success", summary: "done a" },
      { index: 1, reasoning: "run two", status: "success", summary: "done b" },
    ]);
    expect(result.details).toMatchObject({ queryRunType: "sequential" });
    expect(result.content).toEqual([
      {
        type: "text",
        text: "2 queries succeeded · sequential.\n✓ [0] done a\n✓ [1] done b",
      },
      { type: "text", text: "done a" },
      { type: "text", text: "done b" },
    ]);
  });

  it("preserves non-text child content in a batch instead of replacing it with receipts", async () => {
    const result = await executeQueryBatch({
      toolCallId: "call-media",
      raw: {
        queries: [
          { reasoning: "return text", value: "text" },
          { reasoning: "return image", value: "image" },
        ],
      },
      execute: async (_query, index) =>
        index === 0
          ? textResult("full text result")
          : {
              content: [
                { type: "image", data: "aGVsbG8=", mimeType: "image/png" },
              ],
            },
    });

    expect(result.content).toEqual([
      {
        type: "text",
        text: "2 queries succeeded · sequential.\n✓ [0] full text result\n✓ [1] ok",
      },
      { type: "text", text: "full text result" },
      { type: "image", data: "aGVsbG8=", mimeType: "image/png" },
    ]);
  });

  it("caps aggregate model-visible batch text instead of multiplying child limits", async () => {
    const result = await executeQueryBatch({
      toolCallId: "call-large-batch",
      raw: {
        queries: Array.from({ length: 10 }, (_, index) => ({ reasoning: `read ${index}`, value: index })),
      },
      execute: async (_query, index) => textResult(`${index}:${"x".repeat(8_000)}`),
    });

    const visibleChars = result.content.reduce(
      (sum, part) => sum + (part.type === "text" ? part.text.length : 0),
      0,
    );
    expect(visibleChars).toBeLessThanOrEqual(5_000);
    expect((result.content.at(-1) as { text: string }).text).toMatch(/heavy tool output referenced/i);
  });

  it("runs opted-in parallel queries concurrently while returning source-ordered receipts", async () => {
    const release: Array<() => void> = [];
    const started: number[] = [];
    const execution = executeQueryBatch({
      toolCallId: "call-parallel",
      raw: {
        queryRunType: "parallel",
        queries: [
          { reasoning: "read first", value: "a" },
          { reasoning: "read second", value: "b" },
        ],
      },
      allowParallel: true,
      execute: async (_query, index) => {
        started.push(index);
        await new Promise<void>((resolve) => (release[index] = resolve));
        return textResult(`done ${index}`);
      },
    });

    await vi.waitFor(() => expect(started).toEqual([0, 1]));
    release[1]!();
    release[0]!();
    const result = await execution;
    expect(result.details).toMatchObject({
      queryRunType: "parallel",
      results: [
        { index: 0, summary: "done 0" },
        { index: 1, summary: "done 1" },
      ],
    });
  });

  it("caps parallel fan-out at four queries by default", async () => {
    let active = 0;
    let maximum = 0;
    const releases: Array<() => void> = [];
    const execution = executeQueryBatch({
      toolCallId: "call-bounded-parallel",
      raw: {
        queryRunType: "parallel",
        queries: Array.from({ length: 6 }, (_, index) => ({
          reasoning: `read ${index}`,
          value: index,
        })),
      },
      allowParallel: true,
      execute: async (_query, index) => {
        active += 1;
        maximum = Math.max(maximum, active);
        await new Promise<void>((resolve) => {
          releases[index] = resolve;
        });
        active -= 1;
        return textResult(`done ${index}`);
      },
    });

    await vi.waitFor(() => expect(releases.filter(Boolean)).toHaveLength(4));
    expect(maximum).toBe(4);
    releases.slice(0, 4).forEach((release) => release());
    await vi.waitFor(() => expect(releases.filter(Boolean)).toHaveLength(6));
    releases.slice(4).forEach((release) => release());
    await expect(execution).resolves.toMatchObject({
      details: { queryRunType: "parallel" },
    });
    expect(maximum).toBe(4);
  });

  it("rejects parallel execution for tools whose schema is sequential-only", async () => {
    await expect(
      executeQueryBatch({
        toolCallId: "call-unsafe-parallel",
        raw: {
          queryRunType: "parallel",
          queries: [{ reasoning: "mutate", value: "x" }],
        },
        execute: async () => textResult("no"),
      }),
    ).rejects.toThrow(/parallel.*not supported/i);
  });

  it("stops on the first runtime failure and retains success, failure, and not-run rows", async () => {
    const applied: string[] = [];
    let failure: QueryBatchError | undefined;

    try {
      await executeQueryBatch({
        toolCallId: "call-2",
        raw: {
          queries: [
            { reasoning: "apply first", value: "a" },
            { reasoning: "fail second", value: "b" },
            { reasoning: "never third", value: "c" },
          ],
        },
        execute: async (query, index) => {
          if (index === 1) throw new Error("boom");
          applied.push(String(query.value));
          return textResult("first applied");
        },
      });
    } catch (error) {
      failure = error as QueryBatchError;
    }

    expect(failure).toBeInstanceOf(QueryBatchError);
    expect(failure).toMatchObject({
      name: "QueryBatchError",
      failedIndex: 1,
      completedCount: 1,
      rows: [
        {
          index: 0,
          reasoning: "apply first",
          status: "success",
          summary: "first applied",
        },
        {
          index: 1,
          reasoning: "fail second",
          status: "failed",
          summary: "boom",
        },
        {
          index: 2,
          reasoning: "never third",
          status: "not-run",
          summary: "not run",
        },
      ],
    });
    expect(failure?.message).toMatch(
      /\[0\].*first applied[\s\S]*\[1\].*boom[\s\S]*\[2\].*not run/,
    );
    expect(applied).toEqual(["a"]);
  });

  it("reports the real structured failure instead of the first successful progress line", async () => {
    let failure: QueryBatchError | undefined;
    try {
      await executeQueryBatch({
        toolCallId: "call-3",
        raw: {
          queries: [
            { reasoning: "measure source", value: "wc" },
            { reasoning: "build package", value: "build" },
          ],
        },
        execute: async (_query, index) =>
          index === 0
            ? textResult("118 15059 prompt.ts")
            : textResult(
                "Synced 11 skill(s) into skills\nTS2322: actual compilation failure\n(exit 2)",
                {
                  code: 2,
                  stdout: "Synced 11 skill(s) into skills\n",
                  stderr:
                    "building package\nTS2322: actual compilation failure",
                },
                true,
              ),
      });
    } catch (error) {
      failure = error as QueryBatchError;
    }

    expect(failure).toBeInstanceOf(QueryBatchError);
    expect(failure?.rows[1]).toMatchObject({
      index: 1,
      status: "failed",
      summary: "TS2322: actual compilation failure",
    });
    expect(failure?.message).toContain("TS2322: actual compilation failure");
    expect(failure?.message).not.toMatch(/failed[^\n]*Synced 11 skill/);
  });

  it("preserves the concise single-query error contract", async () => {
    try {
      await executeQueryBatch({
        toolCallId: "call-4",
        raw: { queries: [{ reasoning: "fail now", value: "x" }] },
        execute: async () => {
          throw new Error("bad");
        },
      });
    } catch (error) {
      expect(error).toBeInstanceOf(QueryBatchError);
      expect((error as Error).message).toMatch(
        /queries\[0\] failed after 0 prior queries succeeded: bad/,
      );
    }
  });

  it("does not execute when already aborted", async () => {
    const controller = new AbortController();
    controller.abort();
    const execute = vi.fn(async () => textResult("no"));

    await expect(
      executeQueryBatch({
        toolCallId: "call-4",
        raw: { queries: [{ reasoning: "would run", value: "x" }] },
        signal: controller.signal,
        execute,
      }),
    ).rejects.toThrow(/aborted/i);
    expect(execute).not.toHaveBeenCalled();
  });
});
