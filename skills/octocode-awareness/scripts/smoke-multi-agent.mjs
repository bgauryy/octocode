#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { appendFile, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const awareness = join(scriptDir, "awareness.py");
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

function run(label, args, { expect = [0] } = {}) {
  console.log(`[${label}] python3 awareness.py ${args.join(" ")}`);
  const done = spawnSync("python3", [awareness, "--db", db, ...args], {
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

function py(label, code) {
  const done = spawnSync("python3", ["-c", code], { encoding: "utf8" });
  if (done.stdout.trim()) console.log(`[${label}] stdout:\n${done.stdout.trim()}`);
  if (done.stderr.trim()) console.log(`[${label}] stderr:\n${done.stderr.trim()}`);
  if (done.status !== 0) throw new Error(`${label} exited ${done.status}`);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

log("workspace", workspace);
log("phase 1: agent-a claims and edits the temp file");
const claimA = run("agent-a", [
  "pre-flight-intent",
  "--agent-id",
  "agent-a",
  "--rationale",
  "smoke: agent-a edits shared file first",
  "--target-file",
  target,
  "--test-plan",
  "smoke reads final file",
  "--ttl-minutes",
  "20",
]);
await appendFile(target, "agent-a wrote while holding the lock\n", "utf8");

log("phase 2: agent-b collides, then sends a typed request");
const blockedB = run(
  "agent-b",
  [
    "pre-flight-intent",
    "--agent-id",
    "agent-b",
    "--rationale",
    "smoke: agent-b tries same file",
    "--target-file",
    target,
    "--test-plan",
    "smoke reads final file",
  ],
  { expect: [2] },
);
assert(blockedB.conflicts?.length === 1, "agent-b should see one lock conflict");

const request = run("agent-b", [
  "notify",
  "--workspace",
  workspace,
  "--agent-id",
  "agent-b",
  "--to",
  "agent-a",
  "--kind",
  "request",
  "--subject",
  "Please release shared.txt",
  "--body",
  "agent-b is waiting for the shared temp file in the smoke test.",
  "--file",
  target,
  "--ref-id",
  claimA.intent.intent_id,
]);

const inboxA = run("agent-a", [
  "notify-get",
  "--workspace",
  workspace,
  "--agent-id",
  "agent-a",
  "--mark-read",
]);
assert(inboxA.count === 1, "agent-a should receive agent-b request");

log("phase 3: agent-a verifies, releases, and replies");
run("agent-a", [
  "release-file-lock",
  "--agent-id",
  "agent-a",
  "--intent-id",
  claimA.intent.intent_id,
  "--status",
  "SUCCESS",
  "--verified",
  "--verified-note",
  "smoke saw agent-a append",
]);
run("agent-a", [
  "notify",
  "--workspace",
  workspace,
  "--agent-id",
  "agent-a",
  "--to",
  "agent-b",
  "--kind",
  "decision",
  "--subject",
  "Released shared.txt",
  "--body",
  "agent-a released the lock; agent-b may claim now.",
  "--file",
  target,
  "--in-reply-to",
  request.notification.notification_id,
]);

const inboxB = run("agent-b", [
  "notify-get",
  "--workspace",
  workspace,
  "--agent-id",
  "agent-b",
  "--mark-read",
]);
assert(inboxB.count === 1, "agent-b should receive agent-a reply");

log("phase 4: agent-b waits, claims, edits, verifies, and releases");
const waitB = run("agent-b", [
  "wait-for-lock",
  "--agent-id",
  "agent-b",
  "--target-file",
  target,
  "--wait-seconds",
  "0",
]);
assert(waitB.status === "released", "agent-b wait should see released file");
const claimB = run("agent-b", [
  "pre-flight-intent",
  "--agent-id",
  "agent-b",
  "--rationale",
  "smoke: agent-b edits after release",
  "--target-file",
  target,
  "--test-plan",
  "smoke reads final file",
]);
await appendFile(target, "agent-b wrote after receiving release\n", "utf8");
run("agent-b", [
  "release-file-lock",
  "--agent-id",
  "agent-b",
  "--intent-id",
  claimB.intent.intent_id,
  "--status",
  "SUCCESS",
  "--verified",
  "--verified-note",
  "smoke saw agent-b append",
]);

log("phase 5: stale-lock janitor dry-runs and prunes an aged lock");
const stale = run("agent-stale", [
  "pre-flight-intent",
  "--agent-id",
  "agent-stale",
  "--rationale",
  "smoke: stale lock owner disappeared",
  "--target-file",
  staleTarget,
  "--test-plan",
  "smoke janitor releases it",
]);
py(
  "age-stale-lock",
  `import sqlite3, datetime
db=${JSON.stringify(db)}
iid=${JSON.stringify(stale.intent.intent_id)}
old=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(minutes=30)).replace(microsecond=0).isoformat().replace("+00:00","Z")
conn=sqlite3.connect(db)
conn.execute("UPDATE file_locks SET acquired_at=?, expires_at=? WHERE intent_id=?", (old, old, iid))
conn.commit()
conn.close()
print(old)`,
);
const dry = run("janitor", ["prune-stale-locks", "--older-than-minutes", "20", "--dry-run"]);
assert(dry.would_prune >= 1, "janitor dry-run should find aged lock");
const pruned = run("janitor", ["prune-stale-locks", "--older-than-minutes", "20"]);
assert(pruned.pruned_count >= 1, "janitor should prune aged lock");

log("phase 6: final DB and file assertions");
const status = run("status", ["status", "--workspace", workspace]);
assert(status.locks.length === 0, "final status should have no live locks");
assert(status.unverified_intents.length === 1, "only stale-pruned intent should remain pending for audit");
const finalText = await readFile(target, "utf8");
assert(finalText.includes("agent-a wrote"), "final file missing agent-a edit");
assert(finalText.includes("agent-b wrote"), "final file missing agent-b edit");
const thread = run("thread", [
  "notify-get",
  "--workspace",
  workspace,
  "--agent-id",
  "agent-a",
  "--thread-id",
  request.notification.thread_id,
]);
assert(thread.count === 2, "notification thread should include request and reply");

log("PASS", JSON.stringify({ workspace, db, target }, null, 2));
