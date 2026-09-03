import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  createPiSdkScenarioSuite,
  type ProductionPiScenarioId,
} from "../src/adapters/pi-production-probe.js";
import {
  APPROVED_PI_HOST_VERSION,
  createOctocodePiExtension,
} from "../src/index.js";

const roots: string[] = [];

function fixture(): string {
  const root = fs.mkdtempSync(
    path.join(os.tmpdir(), "octocode-pi-production-probe-"),
  );
  roots.push(root);
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0))
    fs.rmSync(root, { recursive: true, force: true });
});

describe("Pi production scenario probe", () => {
  it("publishes strict executable coverage and explicit public-SDK differences", () => {
    const suite = createPiSdkScenarioSuite(fixture(), async () => undefined);
    expect(Object.keys(suite.scenarioProbes).sort()).toEqual([
      "cancellation-boundaries",
      "compaction-matrix",
      "deterministic-model-turn",
      "persistence-restart",
      "policy-denial-matrix",
      "session-lifecycle",
      "steer-and-follow-up",
      "streaming-tool-flow",
      "tool-failure-matrix",
      "transport-corpus",
      "ui-semantics",
    ] satisfies ProductionPiScenarioId[]);
    expect(suite.unsupportedReasons).toEqual({
      "codex-hook-lifecycle": expect.stringMatching(
        /host-neutral.*Codex.*Pi extension/i,
      ),
      "plugin-lifecycle": expect.stringMatching(
        /transactional.*grant.*lease.*disable.*unload/i,
      ),
    });
    expect(
      new Set([
        ...Object.keys(suite.scenarioProbes),
        ...Object.keys(suite.unsupportedReasons),
      ]),
    ).toHaveLength(13);
  });

  it("drives a deterministic turn and streamed tool flow through the real Pi SDK", async () => {
    const suite = createPiSdkScenarioSuite(fixture(), async () => undefined);
    const controller = new AbortController();
    const deterministic = await suite.scenarioProbes[
      "deterministic-model-turn"
    ]!({
      scenario: { id: "deterministic-model-turn" },
      signal: controller.signal,
    });
    expect(deterministic.source).toBe("installed-pi-sdk");
    expect(deterministic.events.map(({ kind }) => kind)).toEqual([
      "turn.started",
      "stream.text",
      "turn.completed",
      "session.snapshot",
    ]);
    expect(JSON.stringify(deterministic)).not.toMatch(
      /probe-secret|api[_-]?key/i,
    );

    const streaming = await suite.scenarioProbes["streaming-tool-flow"]!({
      scenario: { id: "streaming-tool-flow" },
      signal: controller.signal,
    });
    expect(streaming.events.map(({ kind }) => kind)).toEqual(
      expect.arrayContaining([
        "stream.text",
        "stream.thinking",
        "stream.tool-arguments",
        "stream.tool-update",
        "stream.tool-result",
      ]),
    );
    expect(streaming.effects).toEqual([
      expect.objectContaining({ kind: "tool.execution", effectful: false }),
    ]);
  }, 20_000);

  it("executes real denial, failure, and transport boundaries", async () => {
    const suite = createPiSdkScenarioSuite(fixture(), async () => undefined);
    const policy = await suite.scenarioProbes["policy-denial-matrix"]!({
      scenario: { id: "policy-denial-matrix" },
      signal: new AbortController().signal,
    });
    expect(
      policy.events.filter(({ kind }) => kind === "policy.denied"),
    ).toHaveLength(4);
    expect(policy.effects).toHaveLength(0);

    const failures = await suite.scenarioProbes["tool-failure-matrix"]!({
      scenario: { id: "tool-failure-matrix" },
      signal: new AbortController().signal,
    });
    expect(
      failures.events.filter(({ kind }) => kind === "tool.failure"),
    ).toHaveLength(3);

    const transports = await suite.scenarioProbes["transport-corpus"]!({
      scenario: { id: "transport-corpus" },
      signal: new AbortController().signal,
    });
    expect(transports.events.map(({ kind }) => kind)).toEqual([
      "transport.print",
      "transport.json",
      "transport.rpc",
    ]);
  }, 30_000);

  it("composes the deterministic probe with the production Octocode Pi extension", async () => {
    const suite = createPiSdkScenarioSuite(
      fixture(),
      createOctocodePiExtension({ hostVersion: APPROVED_PI_HOST_VERSION }),
    );
    const receipt = await suite.scenarioProbes["deterministic-model-turn"]!({
      scenario: { id: "deterministic-model-turn" },
      signal: new AbortController().signal,
    });
    expect(receipt.events.map(({ kind }) => kind)).toEqual([
      "turn.started",
      "stream.text",
      "turn.completed",
      "session.snapshot",
    ]);
  }, 20_000);

  it("preserves the production extension durable entry count outside parity events", async () => {
    const suite = createPiSdkScenarioSuite(
      fixture(),
      createOctocodePiExtension({ hostVersion: APPROVED_PI_HOST_VERSION }),
    );
    const receipt = await suite.scenarioProbes["persistence-restart"]!({
      scenario: { id: "persistence-restart" },
      signal: new AbortController().signal,
    });

    expect(receipt.events).toEqual([
      {
        kind: "persistence.restarted",
        data: {
          deterministicProjection: true,
        },
      },
    ]);
    expect(receipt.observations).toEqual([
      {
        kind: "persistence.durable-entry-count",
        data: { count: 7, recoveredCustomEntry: true },
      },
    ]);
  }, 20_000);

  it("rejects an already-aborted conformance signal before opening a Pi session", async () => {
    const root = fixture();
    const suite = createPiSdkScenarioSuite(root, async () => undefined);
    const controller = new AbortController();
    controller.abort(new Error("cancel before submit"));

    await expect(
      suite.scenarioProbes["cancellation-boundaries"]!({
        scenario: { id: "cancellation-boundaries" },
        signal: controller.signal,
      }),
    ).rejects.toThrow("cancel before submit");
    expect(fs.readdirSync(root)).toHaveLength(0);
  });

  it("uses real queue, session, compaction, UI, cancellation, and persistence APIs", async () => {
    const suite = createPiSdkScenarioSuite(fixture(), async () => undefined);
    for (const id of [
      "steer-and-follow-up",
      "session-lifecycle",
      "compaction-matrix",
      "ui-semantics",
      "cancellation-boundaries",
      "persistence-restart",
    ] as const) {
      const receipt = await suite.scenarioProbes[id]!({
        scenario: { id },
        signal: new AbortController().signal,
      });
      expect(receipt.source).toBe("installed-pi-sdk");
      expect(receipt.events.length).toBeGreaterThan(0);
      expect(receipt.events.length).toBeLessThanOrEqual(256);
      expect(JSON.stringify(receipt)).not.toMatch(/probe-secret/);

      if (id === "compaction-matrix") {
        expect(receipt.events).toEqual(
          expect.arrayContaining([
            expect.objectContaining({
              kind: "compaction.completed",
              data: expect.objectContaining({ reason: "manual" }),
            }),
            expect.objectContaining({
              kind: "compaction.completed",
              data: expect.objectContaining({ reason: "threshold" }),
            }),
            expect.objectContaining({
              kind: "compaction.completed",
              data: expect.objectContaining({
                reason: "overflow",
                willRetry: false,
              }),
            }),
            expect.objectContaining({
              kind: "compaction.completed",
              data: expect.objectContaining({
                reason: "overflow",
                willRetry: true,
              }),
            }),
            expect.objectContaining({ kind: "compaction.failed-retry" }),
          ]),
        );
        expect(receipt.events).not.toContainEqual(
          expect.objectContaining({ kind: "scenario.difference" }),
        );
      }

      if (id === "ui-semantics") {
        expect(receipt.events).toEqual(
          expect.arrayContaining([
            expect.objectContaining({
              kind: "ui.headless",
              data: expect.objectContaining({
                mode: "json",
                rejected: false,
                providerCalls: 0,
                hasUI: false,
                dialogsReturnedValues: false,
              }),
            }),
          ]),
        );
        expect(receipt.events).not.toContainEqual(
          expect.objectContaining({ kind: "ui.headless-difference" }),
        );
      }

      if (id === "cancellation-boundaries") {
        expect(receipt.events).toEqual(
          expect.arrayContaining([
            expect.objectContaining({ kind: "cancellation.model-stream" }),
            expect.objectContaining({ kind: "cancellation.tool-work" }),
            expect.objectContaining({
              kind: "compaction.cancelled",
              data: expect.objectContaining({ reason: "manual" }),
            }),
            expect.objectContaining({
              kind: "cancellation.before-submit",
              data: expect.objectContaining({
                enforced: true,
                sdkCallAvoided: true,
              }),
            }),
          ]),
        );
      }

      if (id === "persistence-restart") {
        expect(receipt.events).toEqual([
          {
            kind: "persistence.restarted",
            data: {
              deterministicProjection: true,
            },
          },
        ]);
        expect(receipt.observations).toEqual([
          {
            kind: "persistence.durable-entry-count",
            data: { count: 5, recoveredCustomEntry: true },
          },
        ]);
        expect(receipt.events[0]?.data).not.toHaveProperty("entryCount");
      }
    }
  }, 30_000);
});
