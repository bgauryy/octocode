#!/usr/bin/env node
process.removeAllListeners("warning");
process.on("warning", (warning) => {
  if (warning?.name === "ExperimentalWarning" && String(warning?.message).includes("SQLite")) return;
  console.error(warning?.stack ?? String(warning));
});

import { spawnSync } from "node:child_process";
import { appendFile, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const { DatabaseSync } = await import("node:sqlite");

const scriptDir = dirname(fileURLToPath(import.meta.url));
const awareness = join(scriptDir, "awareness.mjs");
const args = process.argv.slice(2);
const allowed = new Set(["--help", "-h"]);

function printHelp() {
  console.log(`Usage: node scripts/smoke-multi-agent.mjs [--help]

Run an end-to-end smoke test for two agents sharing the awareness store.
The script creates a temporary workspace and database, then exercises advisory
overlap, exclusive conflict, verification, signals, stale-prune, and final status.

Options:
  --help, -h  Show this help.`);
}

const unknown = args.filter((arg) => !allowed.has(arg));
if (args.includes("--help") || args.includes("-h")) {
  printHelp();
  process.exit(0);
}
if (unknown.length) {
  console.error(`Unknown option(s): ${unknown.join(", ")}`);
  printHelp();
  process.exit(2);
}

const workspace = await mkdtemp(join(tmpdir(), "octocode-awareness-agents-"));
const db = join(workspace, "awareness.sqlite3");
const target = join(workspace, "shared.txt");
const artifact = "smoke-service";
const staleTarget = join(workspace, "stale.txt");

await writeFile(target, "seed\n", "utf8");
await writeFile(staleTarget, "stale seed\n", "utf8");

function log(title, value = "") {
  console.log(`\n[smoke] ${title}`);
  if (value) console.log(value);
}

function run(label, cmdArgs, { expect = [0] } = {}) {
  const effectiveArgs = cmdArgs.includes("--compact") ? cmdArgs : [...cmdArgs, "--compact"];
  console.log(`[${label}] node awareness.mjs ${effectiveArgs.join(" ")}`);
  const done = spawnSync(process.execPath, [awareness, "--db", db, ...effectiveArgs], {
    cwd: workspace,
    encoding: "utf8",
  });
  if (done.stdout.trim()) console.log(`[${label}] stdout:\n${done.stdout.trim()}`);
  if (done.stderr.trim()) console.log(`[${label}] stderr:\n${done.stderr.trim()}`);
  if (!expect.includes(done.status ?? 1)) {
    throw new Error(`${label} exited ${done.status}; expected ${expect.join("|")}`);
  }
  return done.stdout.trim() ? JSON.parse(done.stdout) : {};
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

log("workspace", workspace);
log("phase 1: ordinary advisory work overlaps and remains visible");
const workA = run("work-agent-a", [
  "work", "start", "--compact",
  "--agent-id", "agent-a", "--workspace", workspace, "--artifact", artifact,
  "--file", target, "--rationale", "smoke: advisory edit A", "--test-plan", "smoke reads final file",
]);
const workB = run("work-agent-b", [
  "work", "start", "--compact",
  "--agent-id", "agent-b", "--workspace", workspace, "--artifact", artifact,
  "--file", target, "--rationale", "smoke: advisory edit B", "--test-plan", "smoke reads final file",
]);
const workRunA = workA.run_id ?? workA.run?.run_id;
const workRunB = workB.run_id ?? workB.run?.run_id;
assert(workRunA && workRunB, "both advisory workers should get runs");
assert(workB.peer_count === 1, "second advisory worker should see the first peer");
const visibleWork = run("work-show", ["work", "show", "--compact", "--workspace", workspace, "--file", target]);
assert(visibleWork.count === 2, "both advisory workers should be visible");
for (const [agent, runId] of [["agent-a", workRunA], ["agent-b", workRunB]]) {
  run(`work-end-${agent}`, ["work", "end", "--compact", "--agent-id", agent, "--run-id", runId]);
  run(`work-verify-${agent}`, ["verify", "mark", "--compact", "--agent-id", agent, "--run-id", runId, "--message", "advisory smoke passed"]);
}

log("phase 2: agent-a acquires exclusivity and edits the temp file");
const claimA = run("agent-a", [
  "lock", "acquire",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--artifact", artifact,
  "--rationale", "smoke: agent-a edits shared file first",
  "--target-file", target,
  "--test-plan", "smoke reads final file",
  "--ttl-minutes", "10",
]);
assert(claimA.run?.run_id, "agent-a should get a standalone run_id");
await appendFile(target, "agent-a wrote while holding the lock\n", "utf8");

log("phase 3: agent-b collides on the live lock");
const blockedB = run(
  "agent-b",
  [
    "lock", "acquire",
    "--agent-id", "agent-b",
    "--workspace", workspace,
    "--artifact", artifact,
    "--rationale", "smoke: agent-b tries same file",
    "--target-file", target,
    "--test-plan", "smoke reads final file",
  ],
  { expect: [2] },
);
assert(blockedB.conflicts?.length === 1, "agent-b should see one lock conflict");

log("phase 4: pending verification is visible, then cleared");
run("agent-a", [
  "lock", "release",
  "--agent-id", "agent-a",
  "--run-id", claimA.run.run_id,
  "--status", "PENDING",
]);
const auditPending = run(
  "audit-pending",
  ["verify", "audit", "--agent-id", "agent-a", "--workspace", workspace, "--artifact", artifact],
  { expect: [1] },
);
assert(auditPending.count === 1, "agent-a should have one pending verification");
const verifiedA = run("verify-agent-a", [
  "verify", "mark",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--artifact", artifact,
  "--all-pending",
  "--message", "smoke read the file after agent-a edit",
]);
assert(verifiedA.count === 1, "verify --all-pending should clear one run");
const auditClear = run("audit-clear", ["verify", "audit", "--agent-id", "agent-a", "--workspace", workspace, "--artifact", artifact]);
assert(auditClear.count === 0, "agent-a pending verification should be clear");

log("phase 5: repo signals deliver once, resolve, and dry-run prune");
const signal = run("signal-publish", [
  "signal", "publish",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--artifact", artifact,
  "--kind", "blocker",
  "--subject", "smoke: shared file was edited",
  "--body", "agent-a finished its verified edit; agent-b may continue",
  "--file", target,
  "--ref-id", claimA.run.run_id,
  "--importance", "7",
]);
assert(signal.signal_id, "signal publish should create a signal id");
const inbox = run("signal-list", [
  "signal", "list",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--artifact", artifact,
  "--mark-read",
]);
assert(inbox.count === 1, "agent-b should receive one unread message");
assert(inbox.signals?.[0]?.subject === "smoke: shared file was edited", "signal subject should round-trip");
const inboxAgain = run("signal-list-again", ["signal", "list", "--agent-id", "agent-b", "--workspace", workspace, "--artifact", artifact]);
assert(inboxAgain.count === 0, "mark-read should prevent duplicate delivery");
const resolved = run("signal-resolve", [
  "signal", "resolve",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--artifact", artifact,
  "--thread-id", signal.thread_id,
]);
assert(resolved.resolved === 1, "signal resolve should close the thread");
const prunePreview = run("signal-prune-dry-run", [
  "signal", "prune",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--artifact", artifact,
  "--resolved",
  "--older-than-days", "1",
  "--dry-run",
]);
assert(prunePreview.would_delete === 0, "fresh resolved messages must survive age-gated prune");

log("phase 6: agent-b acquires exclusivity after release and verifies");
const claimB = run("agent-b", [
  "lock", "acquire",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--artifact", artifact,
  "--rationale", "smoke: agent-b edits after release",
  "--target-file", target,
  "--test-plan", "smoke reads final file",
]);
assert(claimB.run?.run_id, "agent-b should now get a standalone run");
await appendFile(target, "agent-b wrote after receiving release\n", "utf8");
run("agent-b", [
  "lock", "release",
  "--agent-id", "agent-b",
  "--run-id", claimB.run.run_id,
  "--status", "PENDING",
]);
run("agent-b", [
  "verify", "mark",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--run-id", claimB.run.run_id,
  "--status", "SUCCESS",
  "--message", "smoke read final file after agent-b edit",
]);

log("phase 7: stale-lock janitor removes exclusion without ending live work");
const stale = run("agent-stale", [
  "lock", "acquire",
  "--agent-id", "agent-stale",
  "--workspace", workspace,
  "--artifact", artifact,
  "--rationale", "smoke: stale lock owner disappeared",
  "--target-file", staleTarget,
  "--test-plan", "smoke janitor releases it",
  "--ttl-minutes", "1",
]);
assert(stale.run?.run_id, "agent-stale should get a run_id");

const staleDb = new DatabaseSync(db);
const pastTime = new Date(Date.now() - 35 * 60000).toISOString().replace(/\.\d{3}Z$/, "Z");
staleDb.prepare("UPDATE locks SET expires_at = ? WHERE run_id = ?").run(pastTime, stale.run.run_id);
staleDb.close();
console.log(`[age-stale-lock] set expires_at to ${pastTime}`);

const pruned = run("janitor", ["lock", "prune", "--workspace", workspace, "--artifact", artifact]);
assert(pruned.pruned_locks >= 1, `janitor should prune expired lock, got: ${JSON.stringify(pruned)}`);
const afterPruneDb = new DatabaseSync(db);
const afterPrune = afterPruneDb.prepare("SELECT status FROM task_runs WHERE run_id = ?").get(stale.run.run_id);
afterPruneDb.close();
assert(afterPrune?.status === "ACTIVE", "lock expiry must leave live WORK active, not claim completion or debt");
run("end-stale-work", ["work", "end", "--compact", "--agent-id", "agent-stale", "--run-id", stale.run.run_id]);
const staleAudit = run(
  "audit-stale",
  ["verify", "audit", "--agent-id", "agent-stale", "--workspace", workspace, "--artifact", artifact],
  { expect: [1] },
);
assert(staleAudit.count === 1, "explicitly ended stale work should remain pending, not successful");
run("verify-stale", [
  "verify", "mark",
  "--agent-id", "agent-stale",
  "--workspace", workspace,
  "--artifact", artifact,
  "--all-pending",
  "--status", "FAILED",
  "--message", "smoke intentionally failed stale owner after prune",
]);

log("phase 8: final DB and file assertions");
const status = run("status", ["workspace", "status", "--workspace", workspace, "--artifact", artifact]);
assert(status.locks.length === 0, "final status should have no live locks");
const finalAudit = run("audit-final", ["verify", "audit", "--workspace", workspace, "--artifact", artifact]);
assert(finalAudit.count === 0, "final audit should have no pending verification");
const finalText = await readFile(target, "utf8");
assert(finalText.includes("agent-a wrote"), "final file missing agent-a edit");
assert(finalText.includes("agent-b wrote"), "final file missing agent-b edit");

log("PASS", JSON.stringify({ workspace, db, target }, null, 2));
