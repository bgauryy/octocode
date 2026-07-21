#!/usr/bin/env node
/**
 * Eval harness for octocode-orchestrator-local-worker (full agentic surface suite).
 * Usage: node scripts/eval-skill.mjs [--skip-live] [--only id1,id2]
 */
import { spawnSync } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { getOctocodeHome, propagateOctocodeEnv } from "@octocodeai/config";
import { fileURLToPath } from "node:url";

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

function octocodeOutputBase() {
  const workspace = resolve(process.cwd(), ".octocode");
  try {
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
    return workspace;
  } catch {
    const home = getOctocodeHome();
    mkdirSync(home, { recursive: true, mode: 0o700 });
    return home;
  }
}

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const OUTPUT_BASE = octocodeOutputBase();
const cases = JSON.parse(readFileSync(join(ROOT, "evals/cases.json"), "utf8"));
const kpi = JSON.parse(readFileSync(join(ROOT, "evals/kpi-contract.json"), "utf8"));

const args = process.argv.slice(2);
const skipLive = args.includes("--skip-live");
const onlyIdx = args.indexOf("--only");
const only = onlyIdx >= 0 ? new Set(args[onlyIdx + 1].split(",")) : null;

function readSkill() {
  return readFileSync(join(ROOT, "SKILL.md"), "utf8");
}

function extractJson(text) {
  const cleaned = text.replace(/\u001b\[[0-9;]*[a-zA-Z]/g, "").trim();
  const fence = cleaned.match(/```(?:json)?\s*([\s\S]*?)```/);
  const body = fence ? fence[1].trim() : cleaned;
  const start = body.indexOf("{");
  const end = body.lastIndexOf("}");
  if (start >= 0 && end > start) {
    try {
      return JSON.parse(body.slice(start, end + 1));
    } catch {
      return null;
    }
  }
  try {
    return JSON.parse(body);
  } catch {
    return null;
  }
}

function runCmd(cmd, opts = {}) {
  const env = { ...process.env, ...(opts.env || {}) };
  if (opts.envClearModel) delete env.OLLAMA_WORKER_MODEL;
  const r = spawnSync(cmd[0], cmd.slice(1), {
    cwd: ROOT,
    encoding: "utf8",
    env,
    timeout: opts.timeoutMs ?? 180_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  return {
    exit: r.status ?? (r.signal ? 128 : 1),
    stdout: r.stdout || "",
    stderr: r.stderr || "",
    error: r.error ? String(r.error) : null,
  };
}

const LABEL_SET = new Set(["bug", "chore", "question", "risk"]);

function gradeLive(grade, output, meta = {}) {
  const text = output.replace(/\u001b\[[0-9;]*[a-zA-Z]/g, "");
  const obj = extractJson(text);
  switch (grade) {
    case "json_or_labels":
      if (obj?.items?.length) return { pass: true, detail: `items=${obj.items.length}` };
      if (/\b(bug|chore|risk|question)\b/i.test(text) && text.length > 20)
        return { pass: true, detail: "label-like prose" };
      return { pass: false, detail: "no items/labels" };
    case "json_labels_strict": {
      if (!obj?.items?.length) return { pass: false, detail: "no items" };
      const bad = obj.items.filter((it) => !LABEL_SET.has(String(it.label || "").toLowerCase()));
      if (bad.length) return { pass: false, detail: `bad labels: ${bad.map((b) => b.label).join(",")}` };
      return { pass: true, detail: `strict labels n=${obj.items.length}` };
    }
    case "json_has_rows":
      if (obj?.rows?.length) return { pass: true, detail: `rows=${obj.rows.length}` };
      return { pass: false, detail: "missing rows JSON" };
    case "json_has_summary":
      if (typeof obj?.summary === "string" && obj.summary.length > 5)
        return { pass: true, detail: "summary ok" };
      return { pass: false, detail: "missing summary" };
    case "json_code_summary":
      if (typeof obj?.summary === "string" && obj.summary.length > 5)
        return { pass: true, detail: "code summary ok" };
      return { pass: false, detail: "missing code summary" };
    case "json_has_items":
      if (obj?.items?.length) return { pass: true, detail: `items=${obj.items.length}` };
      return { pass: false, detail: "missing items" };
    case "json_has_checks":
      if (obj?.checks?.length) return { pass: true, detail: `checks=${obj.checks.length}` };
      return { pass: false, detail: "missing checks" };
    case "json_mentions_createToken": {
      const blob = JSON.stringify(obj || {}) + text;
      if (obj?.rows?.length && /createToken/i.test(blob))
        return { pass: true, detail: "createToken cited" };
      return { pass: false, detail: "createToken missing" };
    }
    case "json_vision_red": {
      const blob = JSON.stringify(obj || {}).toLowerCase() + text.toLowerCase();
      if (/red/.test(blob) && (obj?.color || obj?.shape || text.length > 10))
        return { pass: true, detail: "vision saw red" };
      return { pass: false, detail: "vision miss red" };
    }
    case "json_translate_he": {
      const t = String(obj?.translation || "");
      // Hebrew letters present and non-trivial length
      if (/[\u0590-\u05FF]/.test(t) && t.length > 8)
        return { pass: true, detail: `he chars n=${t.length}` };
      return { pass: false, detail: "hebrew translation missing" };
    }
    case "json_small_summary":
      if (typeof obj?.summary === "string" && obj.summary.length > 5)
        return { pass: true, detail: "small summary ok" };
      return { pass: false, detail: "missing small summary" };
    case "json_article_grounded": {
      if (!obj || typeof obj.tldr !== "string" || obj.tldr.length < 10)
        return { pass: false, detail: "missing tldr" };
      const claims = Array.isArray(obj.claims) ? obj.claims : [];
      if (!claims.length) return { pass: false, detail: "no claims" };
      const inputRel = meta.input || "evals/fixtures/article-ollama.txt";
      let input = "";
      try {
        input = readFileSync(join(ROOT, inputRel), "utf8");
      } catch {
        return { pass: false, detail: "input missing" };
      }
      const norm = (s) => String(s).replace(/\s+/g, " ").trim();
      const hay = norm(input);
      let ok = 0;
      for (const c of claims) {
        const q = norm(c?.support_quote || "");
        if (q.length >= 8 && hay.includes(q)) ok += 1;
      }
      const rate = ok / claims.length;
      if (rate >= 0.75 && (obj.key_points?.length || 0) >= 1)
        return { pass: true, detail: `grounded ${ok}/${claims.length}` };
      return { pass: false, detail: `grounded ${ok}/${claims.length}` };
    }
    case "nonempty_codeish":
      if (text.length > 40 && /(test\(|it\(|describe\(|expect\()/i.test(text))
        return { pass: true, detail: "codeish test draft" };
      if (text.length > 80) return { pass: true, detail: "nonempty draft" };
      return { pass: false, detail: "empty/weak draft" };
    case "code_has_add":
      if (/function\s+add|const\s+add|export\s+function\s+add/i.test(text) && /\+/.test(text))
        return { pass: true, detail: "add impl present" };
      return { pass: false, detail: "add impl missing" };
    default:
      return { pass: text.trim().length > 10, detail: "fallback nonempty" };
  }
}

function runStatic(c) {
  const skill = readSkill();
  const family = join(ROOT, "references/family-playbooks.md");
  const worker = readFileSync(join(ROOT, "scripts/ollama-worker.sh"), "utf8");
  const selection = readFileSync(join(ROOT, "references/model-selection.md"), "utf8");
  const verify = existsSync(join(ROOT, "references/verify-gate.md"))
    ? readFileSync(join(ROOT, "references/verify-gate.md"), "utf8")
    : "";
  switch (c.check) {
    case "frontmatter": {
      const fm = skill.match(/^---\n([\s\S]*?)\n---/);
      const body = fm?.[1] ?? "";
      const ok =
        /^name:\s*octocode-orchestrator-local-worker\s*$/m.test(body) &&
        /description:/m.test(body) &&
        /Triggers:/i.test(body) &&
        /Do not use/i.test(body) &&
        /RAM kit|capability matrix|capability questions/i.test(body) &&
        !/choose ollama model/i.test(body);
      return {
        pass: ok,
        detail: ok ? "frontmatter ok" : "bad frontmatter (name/desc/triggers)",
      };
    }
    case "refs": {
      const needed = [
        "decision-matrix.md",
        "model-selection.md",
        "family-playbooks.md",
        "ollama-local-models.md",
        "ollama-cli.md",
        "packet-contract.md",
        "ollama-invoke.md",
        "verify-gate.md",
        "usage-matrix.md",
        "references.md",
      ];
      const missing = needed.filter((f) => !existsSync(join(ROOT, "references", f)));
      return {
        pass: missing.length === 0,
        detail: missing.length ? `missing ${missing.join(",")}` : "all refs",
      };
    }
    case "workflow_select_model": {
      const five =
        /GATE\s*→\s*ROUTE\s*→\s*RUN\s*→\s*VERIFY\s*→\s*REPORT/i.test(skill) &&
        /### 2\. ROUTE/i.test(skill);
      return {
        pass: five,
        detail: five ? "5-step ROUTE workflow" : "missing GATE→ROUTE→RUN→VERIFY→REPORT",
      };
    }
    case "workflow_five_steps": {
      const ok =
        /GATE\s*→\s*ROUTE\s*→\s*RUN\s*→\s*VERIFY\s*→\s*REPORT/i.test(skill) &&
        !/GATE\s*→\s*INVENTORY\s*→\s*CLASSIFY/i.test(skill);
      return { pass: ok, detail: ok ? "collapsed to 5 steps" : "still 9-step or missing flow line" };
    }
    case "routine_path_loads": {
      const ok =
        /Routine path loads only:/i.test(skill) &&
        /model-selection\.md/i.test(skill) &&
        /verify-gate\.md/i.test(skill) &&
        /Routine \(default\):/i.test(skill);
      return { pass: ok, detail: ok ? "routine path called out" : "routine path missing" };
    }
    case "catalog_shortcut": {
      const ok =
        /Catalog-only shortcut:/i.test(skill) &&
        /do not run the full offload workflow/i.test(skill) &&
        /Skip GATE/i.test(skill);
      return { pass: ok, detail: ok ? "catalog shortcut present" : "catalog shortcut missing" };
    }
    case "local_models_ref": {
      const path = join(ROOT, "references/ollama-local-models.md");
      const ok =
        existsSync(path) &&
        /When to load|When NOT to load/i.test(readFileSync(path, "utf8")) &&
        /ollama-local-models\.md/.test(skill);
      return { pass: ok, detail: ok ? "local-models gated ref" : "local-models ref weak" };
    }
    case "catalog_load_gated": {
      // Must not instruct unconditional load of the heavy catalog on every ROUTE/SELECT
      const bad =
        /Load `references\/ollama-local-models\.md` for RAM kits/i.test(skill) &&
        !/only for|NOT on routine|not\*\* on routine|Do not\*\* load|Do not load/i.test(skill);
      const good =
        /Do not\*\*? load `?references\/ollama-local-models\.md`? on routine/i.test(skill) ||
        (/ollama-local-models\.md/.test(skill) &&
          /only for RAM kits|not\*\* on routine|NOT on routine|not\*\* routine routing/i.test(skill));
      return {
        pass: good && !bad,
        detail: good && !bad ? "catalog load gated" : "catalog still unconditional",
      };
    }
    case "packet_job_enum": {
      const packet = readFileSync(join(ROOT, "references/packet-contract.md"), "utf8");
      const ok =
        /summarize\s*\|\s*extract\s*\|\s*classify\s*\|\s*draft\s*\|\s*map\s*\|\s*check\s*\|\s*vision\s*\|\s*translate/i.test(
          packet,
        );
      return { pass: ok, detail: ok ? "packet jobs include check|vision|translate" : "packet job enum stale" };
    }
    case "small_task_flow": {
      const ok =
        /Small-task fast path:/i.test(skill) &&
        /Offload OK/i.test(skill) &&
        /tiny job|quick local|small one-shot|including small/i.test(skill);
      return { pass: ok, detail: ok ? "small-task flow present" : "small-task flow missing" };
    }
    case "translate_job": {
      const ok =
        /Translate/i.test(skill) &&
        /translate/.test(worker) &&
        /translate/i.test(
          readFileSync(join(ROOT, "references/packet-contract.md"), "utf8"),
        );
      return { pass: ok, detail: ok ? "translate job wired" : "translate missing" };
    }
    case "usage_matrix": {
      const path = join(ROOT, "references/usage-matrix.md");
      const body = existsSync(path) ? readFileSync(path, "utf8") : "";
      const ok =
        existsSync(path) &&
        /Article \/ internet summarize/i.test(body) &&
        /Research \/ web browse/i.test(body) &&
        /usage-matrix\.md/.test(skill) &&
        /When & how/i.test(skill);
      return { pass: ok, detail: ok ? "usage matrix present" : "usage matrix missing" };
    }
    case "article_offload_pattern": {
      const ok =
        /Worker never browses the web/i.test(skill) &&
        /Summarize article|article \/ web body|already-fetched/i.test(skill) &&
        /support_quote|substring/i.test(skill + verify);
      return { pass: ok, detail: ok ? "article pattern documented" : "article pattern missing" };
    }
    case "job_table_single_owner": {
      const catalog = readFileSync(join(ROOT, "references/ollama-local-models.md"), "utf8");
      const play = readFileSync(family, "utf8");
      const selection = readFileSync(join(ROOT, "references/model-selection.md"), "utf8");
      const portable = /Portable:|capability, not brand|do not assume/i.test(selection);
      const catalogDefers = /model-selection\.md|family-playbooks\.md/i.test(catalog);
      const playOptional = /Not required|optional examples|Do not treat tags/i.test(play);
      return {
        pass: portable && catalogDefers && playOptional,
        detail:
          portable && catalogDefers && playOptional
            ? "portable routing ownership"
            : `portable=${portable} defer=${catalogDefers} optional=${playOptional}`,
      };
    }
    case "not_setup_locked": {
      const bad =
        /Apply family prefs: JSON\/extract → Qwen 7B/i.test(skill) ||
        /suggest `ollama pull gemma4:12b`/i.test(skill);
      const good = /Portable:|examples, not required|size\/capability/i.test(skill);
      return {
        pass: good && !bad,
        detail: good && !bad ? "not setup-locked" : "still setup-locked defaults",
      };
    }
    case "family_playbooks": {
      const ok = existsSync(family) && /Not required|optional examples/i.test(readFileSync(family, "utf8"));
      return { pass: ok, detail: ok ? "family playbooks optional" : "family playbooks missing/rigid" };
    }
    case "forbid_embed": {
      const ok = /embedding-only|\*embed\*|Never use embedding/i.test(skill + selection);
      return { pass: ok, detail: ok ? "embed forbidden" : "no embed forbid" };
    }
    case "blocklist_architecture": {
      const ok = /Architecture/i.test(skill) && /(local NEVER|Blocklist|Keep on orchestrator)/i.test(skill);
      return { pass: ok, detail: ok ? "architecture blocklisted" : "no architecture block" };
    }
    case "blocklist_security": {
      const ok = /security review/i.test(skill);
      return { pass: ok, detail: ok ? "security blocklisted" : "no security block" };
    }
    case "blocklist_tools": {
      const ok = /tool-using agent loop|tool calls/i.test(skill);
      return { pass: ok, detail: ok ? "tool-loop blocked" : "tool-loop not blocked" };
    }
    case "no_image_generation": {
      const ok = /image generation|inventing images|do not generate/i.test(skill + worker);
      return { pass: ok, detail: ok ? "image-gen blocked" : "image-gen not blocked" };
    }
    case "vision_special": {
      const ok = /Vision caption|vision/i.test(skill) && /--image/.test(worker);
      return { pass: ok, detail: ok ? "vision supported" : "vision missing" };
    }
    case "allowlist_jobs": {
      const jobs = [
        "Summarize",
        "Extract",
        "Classify",
        "Draft",
        "Map",
        "Checklist|check",
        "Vision",
        "Translate",
      ];
      const miss = jobs.filter((j) => !new RegExp(j, "i").test(skill));
      const flexible = /not an exclusive whitelist|similar low-risk/i.test(skill);
      const smallOk = /Offload OK|Small-task fast path/i.test(skill);
      return {
        pass: miss.length === 0 && flexible && smallOk,
        detail:
          miss.length === 0 && flexible && smallOk
            ? "job patterns flexible + small OK"
            : `missing=${miss.join(",")} flexible=${flexible} smallOk=${smallOk}`,
      };
    }
    case "verify_gate_doc": {
      const ok = /untrusted|Verify/i.test(verify) && /VERIFY/i.test(skill);
      return { pass: ok, detail: ok ? "verify gate documented" : "verify gate weak" };
    }
    case "no_embed_as_preferred": {
      const play = existsSync(family) ? readFileSync(family, "utf8") : "";
      const bad = /Prefer first[^\n]*nomic-embed|default[^\n]*nomic-embed/i.test(
        play + selection + skill,
      );
      return { pass: !bad, detail: bad ? "embed preferred (bad)" : "embed not preferred" };
    }
    case "think_equals_form": {
      const ok = /--think=\$\{|--think=VALUE|--think=false/.test(worker);
      const bad = /RUN_ARGS\+=\(--think "\$\{/.test(worker);
      return {
        pass: ok && !bad,
        detail: ok && !bad ? "think= form ok" : "think flag form risky",
      };
    }
    case "worker_vision_job": {
      const ok = /vision/.test(worker) && /IMAGE_PATH/.test(worker);
      return { pass: ok, detail: ok ? "worker vision job" : "worker lacks vision" };
    }
    default:
      return { pass: false, detail: `unknown static check ${c.check}` };
  }
}

function runScript(c) {
  const r = runCmd(c.cmd, {
    envClearModel: c.envClearModel,
    timeoutMs: 30_000,
  });
  let pass = r.exit === c.expectExit;
  const details = [`exit=${r.exit} want=${c.expectExit}`];
  const out = `${r.stdout}\n${r.stderr}`;
  if (c.stdoutMustIncludeAny) {
    const hit = c.stdoutMustIncludeAny.some((s) => r.stdout.includes(s));
    pass = pass && hit;
    details.push(hit ? "list hit" : "list miss");
  }
  if (c.stdoutMustIncludeAll) {
    const miss = c.stdoutMustIncludeAll.filter((s) => !r.stdout.includes(s));
    pass = pass && miss.length === 0;
    details.push(miss.length ? `missing ${miss.join("|")}` : "stdout all");
  }
  if (c.stderrMustMatch) {
    const re = new RegExp(c.stderrMustMatch, "i");
    const hit = re.test(out);
    pass = pass && hit;
    details.push(hit ? "stderr match" : "stderr miss");
  }
  return { pass, detail: details.join("; "), raw: { exit: r.exit, stderr: r.stderr.slice(0, 400) } };
}

function runLive(c) {
  const outPath = join(OUTPUT_BASE, "worker", `${c.id}.out`);
  mkdirSync(dirname(outPath), { recursive: true });
  const cmd = ["scripts/ollama-worker.sh", "--model", c.model, "--job", c.job, "--out", outPath];
  if (c.input) cmd.push("--input", c.input);
  if (c.image) cmd.push("--image", c.image);
  if (c.schema) cmd.push("--schema", c.schema);
  if (c.formatJson) cmd.push("--format-json");
  if (c.think != null) cmd.push("--think", String(c.think));
  const r = runCmd(cmd, { timeoutMs: 240_000 });
  if (r.exit !== 0) {
    return {
      pass: false,
      detail: `worker exit ${r.exit}: ${(r.stderr || r.stdout || r.error || "").slice(0, 200)}`,
    };
  }
  const output = existsSync(outPath) ? readFileSync(outPath, "utf8") : r.stdout;
  const g = gradeLive(c.grade, output, { input: c.input });
  return { ...g, outputPreview: output.slice(0, 280) };
}

const selected = cases.filter((c) => {
  if (only && !only.has(c.id)) return false;
  if (skipLive && c.kind === "live") return false;
  return true;
});

const results = [];
for (const c of selected) {
  process.stderr.write(`→ ${c.id} [${c.surface || "?"}] (${c.kind})\n`);
  let res;
  try {
    if (c.kind === "static") res = runStatic(c);
    else if (c.kind === "script") res = runScript(c);
    else if (c.kind === "live") res = runLive(c);
    else res = { pass: false, detail: `unknown kind ${c.kind}` };
  } catch (e) {
    res = { pass: false, detail: String(e) };
  }
  // soft cases: fail counts as WARN not FAIL for primary (still recorded)
  const softFail = c.soft && !res.pass;
  results.push({
    id: c.id,
    kind: c.kind,
    group: c.group,
    surface: c.surface || "unknown",
    heldOut: !!c.heldOut,
    soft: !!c.soft,
    pass: softFail ? true : !!res.pass,
    hardPass: !!res.pass,
    detail: softFail ? `SOFT-FAIL ${res.detail}` : res.detail,
    outputPreview: res.outputPreview,
  });
  const mark = softFail ? "SOFT" : res.pass ? "PASS" : "FAIL";
  process.stderr.write(`  ${mark} — ${res.detail}\n`);
}

const passed = results.filter((r) => r.pass).length;
const total = results.length;
const rate = total ? passed / total : 0;
const hardFails = results.filter((r) => !r.hardPass && !r.soft);
const held = results.filter((r) => r.heldOut);
const heldRate = held.length ? held.filter((r) => r.hardPass).length / held.length : null;
const live = results.filter((r) => r.kind === "live");
const liveRate = live.length ? live.filter((r) => r.hardPass).length / live.length : null;
const script = results.filter((r) => r.kind === "script");
const scriptRate = script.length ? script.filter((r) => r.pass).length / script.length : null;

const bySurface = {};
for (const r of results) {
  const s = r.surface;
  if (!bySurface[s]) bySurface[s] = { pass: 0, total: 0, hardFail: 0 };
  bySurface[s].total += 1;
  if (r.hardPass) bySurface[s].pass += 1;
  if (!r.hardPass && !r.soft) bySurface[s].hardFail += 1;
}

const guardIds = [
  "script-reject-missing-llama32",
  "guard-embed-not-default",
  "guard-think-equals-form",
  "script-reject-fake-model",
  "static-no-image-generation",
  "static-blocklist-tools",
];
const guards = results.filter((r) => guardIds.includes(r.id));
const guardsOk = guards.length > 0 && guards.every((r) => r.hardPass);

const verdict =
  rate >= kpi.primaryKpi.target && guardsOk && hardFails.length === 0
    ? "ACCEPT"
    : rate >= 0.6
      ? "CONTINUE"
      : "REVERT";

const report = {
  goal: kpi.goal,
  kpi: {
    primary: {
      name: kpi.primaryKpi.name,
      baseline: kpi.primaryKpi.baseline,
      result: Number(rate.toFixed(3)),
      target: kpi.primaryKpi.target,
    },
    leading: {
      script_gate_pass_rate: scriptRate,
      live_worker_hard_pass_rate: liveRate,
      held_out_hard_pass_rate: heldRate,
    },
    guardrailsOk: guardsOk,
  },
  surfaces: bySurface,
  budget: kpi.budget,
  decisionRule: kpi.decisionRule,
  verdict,
  totals: { passed, total, rate, hardFails: hardFails.length },
  failures: results.filter((r) => !r.hardPass),
  results,
};

const reportDir = join(OUTPUT_BASE, "orchestrator-local-worker", "evals");
mkdirSync(reportDir, { recursive: true });
writeFileSync(join(reportDir, "last-report.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
process.exit(verdict === "ACCEPT" || (hardFails.length === 0 && rate >= kpi.primaryKpi.target) ? 0 : 1);
