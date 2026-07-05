#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { appendFile, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const awareness = join(scriptDir, "awareness.mjs");
const args = process.argv.slice(2);
const allowed = new Set(["--help", "-h"]);

function printHelp() {
  console.log(`Usage: node scripts/smoke-multi-agent.mjs [--help]

Run an end-to-end smoke test for two agents sharing the awareness store.
The script creates a temporary workspace and database, then exercises claim,
conflict, pending verification, notifications, release, re-claim,
stale-prune, and final status flows.

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
const staleTarget = join(workspace, "stale.txt");

await writeFile(target, "seed\n", "utf8");
await writeFile(staleTarget, "stale seed\n", "utf8");

function log(title, value = "") {
  console.log(`\n[smoke] ${title}`);
  if (value) console.log(value);
}

function run(label, cmdArgs, { expect = [0] } = {}) {
  console.log(`[${label}] node awareness.mjs ${cmdArgs.join(" ")}`);
  const done = spawnSync(process.execPath, [awareness, "--db", db, ...cmdArgs], {
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
log("phase 1: agent-a claims and edits the temp file");
const claimA = run("agent-a", [
  "pre-flight-intent",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--rationale", "smoke: agent-a edits shared file first",
  "--target-file", target,
  "--test-plan", "smoke reads final file",
  "--ttl-minutes", "20",
]);
assert(claimA.intent?.intent_id, "agent-a should get an intent_id");
await appendFile(target, "agent-a wrote while holding the lock\n", "utf8");

log("phase 2: agent-b collides on the live lock");
const blockedB = run(
  "agent-b",
  [
    "pre-flight-intent",
    "--agent-id", "agent-b",
    "--workspace", workspace,
    "--rationale", "smoke: agent-b tries same file",
    "--target-file", target,
    "--test-plan", "smoke reads final file",
  ],
  { expect: [2] },
);
assert(blockedB.conflicts?.length === 1, "agent-b should see one lock conflict");

log("phase 3: pending verification is visible, then cleared");
run("agent-a", [
  "release-file-lock",
  "--agent-id", "agent-a",
  "--intent-id", claimA.intent.intent_id,
  "--status", "PENDING",
]);
const auditPending = run(
  "audit-pending",
  ["audit-unverified", "--agent-id", "agent-a", "--workspace", workspace],
  { expect: [1] },
);
assert(auditPending.count === 1, "agent-a should have one pending verification");
const verifiedA = run("verify-agent-a", [
  "verify",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--all-pending",
  "--message", "smoke read the file after agent-a edit",
]);
assert(verifiedA.count === 1, "verify --all-pending should clear one intent");
const auditClear = run("audit-clear", ["audit-unverified", "--agent-id", "agent-a", "--workspace", workspace]);
assert(auditClear.count === 0, "agent-a pending verification should be clear");

log("phase 4: repo notifications deliver once, resolve, and dry-run prune");
const notification = run("notify", [
  "notify",
  "--agent-id", "agent-a",
  "--workspace", workspace,
  "--kind", "blocker",
  "--subject", "smoke: shared file was edited",
  "--body", "agent-a finished its verified edit; agent-b may continue",
  "--file", target,
  "--ref-id", claimA.intent.intent_id,
  "--importance", "7",
]);
assert(notification.notification_id, "notify should create a notification id");
const inbox = run("notify-get", [
  "notify-get",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--mark-read",
]);
assert(inbox.count === 1, "agent-b should receive one unread message");
assert(inbox.notifications?.[0]?.subject === "smoke: shared file was edited", "notification subject should round-trip");
const inboxAgain = run("notify-get-again", ["notify-get", "--agent-id", "agent-b", "--workspace", workspace]);
assert(inboxAgain.count === 0, "mark-read should prevent duplicate delivery");
const resolved = run("notify-resolve", [
  "notify-resolve",
  "--workspace", workspace,
  "--thread-id", notification.thread_id,
]);
assert(resolved.resolved === 1, "notify-resolve should close the thread");
const prunePreview = run("notify-prune-dry-run", [
  "notify-prune",
  "--workspace", workspace,
  "--resolved",
  "--dry-run",
]);
assert(prunePreview.would_delete >= 1, "notify-prune --dry-run should find the resolved message");

log("phase 5: agent-b re-claims, edits, and releases with verification");
const claimB = run("agent-b", [
  "pre-flight-intent",
  "--agent-id", "agent-b",
  "--workspace", workspace,
  "--rationale", "smoke: agent-b edits after release",
  "--target-file", target,
  "--test-plan", "smoke reads final file",
]);
assert(claimB.intent?.intent_id, "agent-b should now get a claim");
await appendFile(target, "agent-b wrote after receiving release\n", "utf8");
run("agent-b", [
  "release-file-lock",
  "--agent-id", "agent-b",
  "--intent-id", claimB.intent.intent_id,
  "--status", "SUCCESS",
  "--verified",
  "--verified-note", "smoke read final file after agent-b edit",
]);

log("phase 6: stale-lock janitor prunes an aged lock without claiming success");
const stale = run("agent-stale", [
  "pre-flight-intent",
  "--agent-id", "agent-stale",
  "--workspace", workspace,
  "--rationale", "smoke: stale lock owner disappeared",
  "--target-file", staleTarget,
  "--test-plan", "smoke janitor releases it",
  "--ttl-minutes", "1",
]);
assert(stale.intent?.intent_id, "agent-stale should get an intent_id");

const staleDb = new DatabaseSync(db);
const pastTime = new Date(Date.now() - 35 * 60000).toISOString().replace(/\.\d{3}Z$/, "Z");
staleDb.prepare("UPDATE file_locks SET expires_at = ? WHERE intent_id = ?").run(pastTime, stale.intent.intent_id);
staleDb.close();
console.log(`[age-stale-lock] set expires_at to ${pastTime}`);

const pruned = run("janitor", ["prune-stale-locks"]);
assert(pruned.pruned_locks >= 1, `janitor should prune expired lock, got: ${JSON.stringify(pruned)}`);
const staleAudit = run(
  "audit-stale",
  ["audit-unverified", "--agent-id", "agent-stale", "--workspace", workspace],
  { expect: [1] },
);
assert(staleAudit.count === 1, "stale-pruned work should remain pending, not successful");
run("verify-stale", [
  "verify",
  "--agent-id", "agent-stale",
  "--workspace", workspace,
  "--all-pending",
  "--status", "FAILED",
  "--message", "smoke intentionally failed stale owner after prune",
]);

log("phase 7: final DB and file assertions");
const status = run("status", ["status", "--workspace", workspace]);
assert(status.locks.length === 0, "final status should have no live locks");
const finalAudit = run("audit-final", ["audit-unverified", "--workspace", workspace]);
assert(finalAudit.count === 0, "final audit should have no pending verification");
const finalText = await readFile(target, "utf8");
assert(finalText.includes("agent-a wrote"), "final file missing agent-a edit");
assert(finalText.includes("agent-b wrote"), "final file missing agent-b edit");

log("PASS", JSON.stringify({ workspace, db, target }, null, 2));
