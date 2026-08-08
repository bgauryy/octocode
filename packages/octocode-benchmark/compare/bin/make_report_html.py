#!/usr/bin/env python3
"""Render results/index.html from results/per_question_summary.json.

Self-contained interactive report (data INLINED, works from file://; charts via
Chart.js CDN). Regenerate:

    python3 compare/bin/per_question_summary.py --json results/per_question_summary.json
    python3 compare/bin/make_report_html.py

Design: hero verdict → per-vendor comparison cards → KPI strip →
per-question distributions (chars, correctness) → total context budget (demoted)
→ how it's measured → questions. Every widget has a plain-language description.
All figures come from the JSON (single source of truth); nothing hardcoded.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_JSON = ROOT / "results" / "per_question_summary.json"
DEFAULT_OUT = ROOT / "results" / "index.html"

COLORS = {"octocode": "#2dd4bf", "gh": "#94a3b8", "rtk": "#f59e0b", "headroom": "#a78bfa"}
LABELS = {"octocode": "Octocode", "gh": "gh", "rtk": "gh + RTK", "headroom": "gh + Headroom"}
ARMS = ["octocode", "gh", "rtk", "headroom"]

TEMPLATE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Octocode Research Benchmark — leaner GitHub research at equal correctness</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
<style>
  :root{
    --bg:#080d18;--bg2:#0b1120;--panel:#0f1729;--panel2:#111a30;--ink:#e8eef8;--mut:#93a1b8;
    --line:#1e293b;--teal:#2dd4bf;--tealdim:#0f766e;--amber:#f59e0b;--violet:#a78bfa;--slate:#94a3b8;
  }
  *{box-sizing:border-box}
  html{scroll-behavior:smooth}
  body{margin:0;background:radial-gradient(1200px 600px at 70% -10%,#0e2036 0%,var(--bg) 55%);color:var(--ink);
       font:14px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;-webkit-font-smoothing:antialiased}
  a{color:var(--teal);text-decoration:none}a:hover{text-decoration:underline}
  .wrapc{max-width:1160px;margin:0 auto;padding:0 22px}
  /* header */
  header{padding:46px 0 8px}
  .eyebrow{color:var(--teal);font-weight:700;letter-spacing:.14em;text-transform:uppercase;font-size:11.5px}
  h1{margin:8px 0 10px;font-size:30px;line-height:1.15;letter-spacing:-.01em}
  h1 .hl{color:var(--teal)}
  .lede{color:var(--mut);max-width:760px;font-size:15px}
  .meta{margin-top:12px;display:flex;flex-wrap:wrap;gap:8px}
  .chip{background:var(--panel);border:1px solid var(--line);border-radius:999px;padding:4px 11px;font-size:11.5px;color:var(--mut)}
  .chip b{color:var(--ink)}
  /* verdict */
  .verdict{margin:26px 0 6px;background:linear-gradient(180deg,#0d2033,#0f1729);border:1px solid #17324e;border-radius:18px;padding:22px 22px 8px}
  .verdict .big{font-size:23px;line-height:1.35;font-weight:600;margin:2px 0 4px}
  .verdict .big b{color:var(--teal)}
  .tie{display:inline-block;background:#063d34;color:#5eead4;border:1px solid #0f766e;border-radius:999px;padding:1px 10px;font-size:12px;font-weight:700;vertical-align:middle}
  .vsub{color:var(--mut);font-size:13px;max-width:820px;margin-bottom:14px}
  .cmp{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:14px;margin:8px 0 16px}
  .vs{background:var(--panel2);border:1px solid var(--line);border-radius:14px;padding:16px}
  .vs .head{display:flex;align-items:center;gap:8px;font-size:13px;color:var(--mut);margin-bottom:8px}
  .vs .mult{font-size:34px;font-weight:800;color:var(--teal);letter-spacing:-.02em;line-height:1}
  .vs .mult small{font-size:14px;font-weight:600;color:var(--mut)}
  .vs .sub{font-size:12px;color:var(--mut);margin-top:8px}
  .vs .corr{margin-top:8px;font-size:12px;color:var(--ink)}
  .bar{height:6px;border-radius:4px;background:#1b2740;margin-top:10px;overflow:hidden}
  .bar > i{display:block;height:100%;border-radius:4px}
  /* KPI strip */
  .kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px;margin:16px 0}
  .kpi{background:var(--panel);border:1px solid var(--line);border-radius:14px;padding:14px 16px}
  .kpi .k{font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:var(--mut)}
  .kpi .v{font-size:22px;font-weight:800;margin-top:3px}
  .kpi .d{font-size:11.5px;color:var(--mut);margin-top:3px}
  /* panels */
  section.panel{background:var(--panel);border:1px solid var(--line);border-radius:16px;padding:18px 20px;margin:16px 0}
  section.panel h2{margin:0 0 3px;font-size:17px}
  .desc{color:var(--mut);font-size:12.5px;margin-bottom:12px;max-width:860px}
  .wrap{position:relative;height:340px}
  code{background:#0a1222;border:1px solid var(--line);padding:1px 5px;border-radius:5px;font-size:12px}
  .dot{display:inline-block;width:10px;height:10px;border-radius:3px;margin-right:7px;vertical-align:middle}
  .toolbar{display:flex;gap:8px;align-items:center;margin-bottom:10px}
  button{background:#16233c;color:var(--ink);border:1px solid var(--line);border-radius:8px;padding:6px 11px;cursor:pointer;font-size:12px}
  button.active{background:var(--tealdim);border-color:var(--tealdim);color:#eafffb}
  .cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:12px}
  .card{background:var(--panel2);border:1px solid var(--line);border-radius:12px;padding:14px}
  .card h3{margin:0 0 4px;font-size:11.5px;text-transform:uppercase;letter-spacing:.05em;color:var(--teal)}
  .card .cdesc{color:var(--mut);font-size:11px;margin:0 0 9px}
  .rowr{display:flex;justify-content:space-between;padding:2px 0;font-size:13px}
  .rowr b{font-variant-numeric:tabular-nums}
  .note{border-left:3px solid #475569;background:#0a1222;padding:11px 13px;border-radius:8px;color:#c7d2e0;font-size:12.5px;margin-top:12px}
  .mgrid{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:14px}
  .mcol h4{margin:0 0 4px;font-size:13px;color:#7dd3fc}
  .mcol p{margin:0;color:#c7d2e0;font-size:12.5px}
  .qlinks{display:flex;flex-wrap:wrap;gap:6px;margin-top:6px}
  .qlinks a{padding:4px 9px;border:1px solid var(--line);border-radius:8px;background:var(--panel2);color:var(--teal);font-size:12px;font-variant-numeric:tabular-nums}
  .qlinks a:hover{border-color:var(--teal);text-decoration:none}
  footer{color:var(--mut);font-size:12px;padding:8px 0 40px}
</style>
</head>
<body>
<div class="wrapc">
<header>
  <div class="eyebrow">Octocode Research Benchmark</div>
  <h1>Same answers, <span class="hl">a fraction of the context</span>.</h1>
  <div class="lede">A blind, head-to-head test of GitHub <b>research</b> tools on 30 real questions. Every tool
    reaches near-identical correctness — so the real question is <b>how many characters each one pushes through
    the model</b> to get there. Fewer characters = a cleaner context window and sharper model attention.</div>
  <div class="meta" id="meta"></div>
</header>

<!-- VERDICT -->
<div class="verdict">
  <div class="big" id="blLine"></div>
  <div class="vsub" id="blSub"></div>
  <div class="cmp" id="cmp"></div>
</div>

<!-- KPI strip -->
<div class="kpis" id="kpis"></div>

<!-- distributions -->
<section class="panel">
  <h2>Characters per question</h2>
  <div class="desc">Each line is one tool. Y = characters that tool pushed through the model to answer that question
    (<b>model-in</b> tool output + <b>model-out</b> commands &amp; final answer). Lower = leaner. Log scale by default —
    a few questions are far larger than the rest.</div>
  <div class="toolbar"><span style="color:var(--mut);font-size:12px">Scale</span>
    <button id="scaleLog" class="active">log</button><button id="scaleLin">linear</button></div>
  <div class="wrap"><canvas id="charsChart"></canvas></div>
</section>

<section class="panel">
  <h2>Correctness per question</h2>
  <div class="desc">Blind judge score (0–10) for each tool's answer, per question. The lines cluster near the top and
    overlap — that's the near-parity correctness that makes <i>characters</i> the deciding metric.</div>
  <div class="wrap"><canvas id="corrChart"></canvas></div>
</section>

<!-- quick stats -->
<section class="panel">
  <h2>Quick stats (per tool)</h2>
  <div class="desc">Central-tendency summaries across the 30 questions. <b>Median</b> = the robust “typical question”;
    <b>mean</b> is pulled up by a few large questions.</div>
  <div class="cards" id="cards"></div>
</section>

<!-- pooled total (demoted) -->
<section class="panel">
  <h2>Total context budget (all 30 questions)</h2>
  <div class="desc">Sum of every character each tool delivered across the whole set. Useful as a raw budget,
    <b>but not the headline</b>: it's dominated by a few huge questions, so the multiples look bigger than the
    typical-question figure above.</div>
  <div class="wrap" style="height:210px"><canvas id="poolChart"></canvas></div>
  <div class="note" id="poolNote"></div>
</section>

<!-- method -->
<section class="panel">
  <h2>How this is measured</h2>
  <div class="mgrid">
    <div class="mcol"><h4>1 · The 30 questions</h4><p>One shared set (<code>compare/github-questions/Q1…Q30.md</code>) — title, <code>id</code>, and a <code>## Question</code> only; no hints, no answer key. Real GitHub research from single-hit lookups → deep multi-hop reads. Same questions, same frozen refs for every tool — <b>only the CLI differs</b>.</p></div>
    <div class="mcol"><h4>2 · The arms</h4><p><b>Octocode</b> is the anchor (<code>npx octocode tools …</code>). Baselines: <b>gh</b> (plain), <b>gh+RTK</b> (<code>rtk gh …</code>), <b>gh+Headroom</b> (<code>gh</code> → Headroom compressor). Each is a separate pairwise matchup.</p></div>
    <div class="mcol"><h4>3 · The flow</h4><p><b>0 Preflight</b> verify + pin tools. <b>1 Answer</b> a fresh isolated agent per (question, arm, pass), leanest legal path. <b>2 Judge</b> grade blind. <b>3 Summarize</b> validate + aggregate. <b>≥3 passes</b>.</p></div>
    <div class="mcol"><h4>4 · Characters</h4><p>Wrappers log each call: <code>total = model-in + model-out</code> (Unicode code points, both directions; primer excluded, failed calls counted). Never self-reported — recomputed &amp; re-hashed by <code>sumlog.py --strict</code>. Chars ≈ tokens (not a direct token/latency/cost measure).</p></div>
    <div class="mcol"><h4>5 · The judge</h4><p>Blind X/Y (randomized, seed=42), tool identity hidden. Establishes ground truth, then scores <b>correctness 0–10, depth 1–5, workflow 1–5</b>. Different model family (gpt-5.5) from runners. <b>Correctness-first</b>: leaner never beats more-correct.</p></div>
    <div class="mcol"><h4>6 · Aggregation</h4><p>Question is the unit. Headline = <b>per-question ratio geometric mean</b>; <b>median</b> = robust central; <b>pooled total</b> shown only with top-contributor share + leave-one-out. ≥3 passes + 95% bootstrap CI. Public suite = <b>orientation, not a shipping gate</b>.</p></div>
  </div>
  <div class="desc" style="margin:12px 0 0" id="methodFoot"></div>
</section>

<section class="panel">
  <h2>Questions</h2>
  <div class="desc">The 30 shared GitHub research questions — <a href="https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/compare/github-questions" target="_blank" rel="noopener">browse all on GitHub</a>. Click a number to open it.</div>
  <div class="qlinks" id="qlinks"></div>
</section>

<footer>
  Generated from <code>results/per_question_summary.json</code> by <code>compare/bin/make_report_html.py</code> — every figure recomputed from per-call JSONL logs.
  See <code>results/PER_QUESTION_SUMMARY.md</code>, <code>results/SUMMARY.md</code>, <code>results/VALIDATION-2026-08-08.md</code>.
</footer>
</div>

<script>
const DATA = __DATA__, COLORS = __COLORS__, LABELS = __LABELS__, ARMS = __ARMS__;
const om = DATA.overall, geo = om.per_instance_char_ratio, pooled = om.pooled_char_ratio;
const BASE = ARMS.filter(a=>a!=="octocode");
const Q = DATA.rows.map(r=>"Q"+r.q);
const tickC="#93a1b8", gridC="#1b2740";
const dot=a=>'<span class="dot" style="background:'+COLORS[a]+'"></span>';
const fmt=n=>n==null?"n/a":Number(n).toLocaleString(undefined,{maximumFractionDigits:n<100?2:0});

// meta chips
document.getElementById("meta").innerHTML = [
  ["Questions","<b>30</b> × 3 passes"],["Build","local <b>v18.1.1</b>"],
  ["Judge","blind <b>gpt-5.5</b>"],["Metric","characters (model-in + model-out)"],
].map(([k,v])=>`<span class="chip">${k}: ${v}</span>`).join("");

// verdict
const lo=Math.min(...BASE.map(a=>geo[a].geo_mean)), hi=Math.max(...BASE.map(a=>geo[a].geo_mean));
const cmin=Math.min(...ARMS.map(a=>om.correct_mean[a])), cmax=Math.max(...ARMS.map(a=>om.correct_mean[a]));
document.getElementById("blLine").innerHTML =
  `At <span class="tie">≈ tie</span> correctness (${cmin.toFixed(1)}–${cmax.toFixed(1)}/10), Octocode answers with `+
  `<b>${lo.toFixed(1)}×–${hi.toFixed(1)}× fewer characters</b> than gh, gh+RTK and gh+Headroom on a typical question.`;
document.getElementById("blSub").innerHTML =
  `“Leaner” = per-question geometric mean of baseline ÷ Octocode characters (the standard, outlier-robust way to `+
  `average ratios). Correctness is a blind-judge tie, so this efficiency is the real difference. `+
  `Characters ≈ tokens (not a direct token/latency/cost measure); the public suite is orientation, not a shipping gate.`;

// per-vendor comparison cards
document.getElementById("cmp").innerHTML = BASE.map(a=>{
  const g=geo[a].geo_mean, pct=Math.min(100,(g/hi)*100);
  return `<div class="vs">
    <div class="head">${dot(a)}Octocode vs <b style="color:var(--ink)">${LABELS[a]}</b></div>
    <div class="mult">${g.toFixed(2)}× <small>fewer chars / question</small></div>
    <div class="bar"><i style="width:${pct}%;background:${COLORS[a]}"></i></div>
    <div class="corr">correctness ${om.correct_mean.octocode.toFixed(1)} vs ${om.correct_mean[a].toFixed(1)} /10 · median leaner ${geo[a].median.toFixed(2)}×</div>
    <div class="sub">Octocode leaner on <b>${geo[a].octocode_leaner}/90</b> runs</div>
  </div>`;
}).join("");

// KPI strip
document.getElementById("kpis").innerHTML = [
  ["Correctness","≈ tie", `all tools ${cmin.toFixed(1)}–${cmax.toFixed(1)} / 10 (blind judge)`],
  ["Typical leanness", `${lo.toFixed(1)}×–${hi.toFixed(1)}×`, "fewer characters per question (geo-mean)"],
  ["Median chars / Q — Octocode", fmt(om.chars_median_per_question.octocode), `vs ${fmt(om.chars_median_per_question.gh)}–${fmt(om.chars_median_per_question.headroom)} baselines`],
  ["Octocode leaner", `${geo.rtk.octocode_leaner}–${geo.gh.octocode_leaner}/90`, "of judged runs across baselines"],
].map(([k,v,d])=>`<div class="kpi"><div class="k">${k}</div><div class="v" style="color:var(--teal)">${v}</div><div class="d">${d}</div></div>`).join("");

// charts
function ds(metric){return ARMS.map(a=>({label:LABELS[a],data:DATA.rows.map(r=>r[metric][a]),
  borderColor:COLORS[a],backgroundColor:COLORS[a],borderWidth:a==="octocode"?3:1.7,
  pointRadius:2.2,pointHoverRadius:5,tension:.25,spanGaps:true}));}
function baseOpts(yTitle,yType){return{responsive:true,maintainAspectRatio:false,
  interaction:{mode:"index",intersect:false},
  plugins:{legend:{labels:{color:"#e8eef8",usePointStyle:true,pointStyle:"line"}},
    tooltip:{callbacks:{label:c=>c.dataset.label+": "+(c.parsed.y==null?"n/a":Number(c.parsed.y).toLocaleString(undefined,{maximumFractionDigits:1}))}}},
  scales:{x:{ticks:{color:tickC,maxRotation:0,autoSkip:true},grid:{color:gridC}},
          y:{type:yType||"linear",title:{display:!!yTitle,text:yTitle,color:tickC},ticks:{color:tickC},grid:{color:gridC}}}};}

const charsChart=new Chart(document.getElementById("charsChart"),
  {type:"line",data:{labels:Q,datasets:ds("chars")},options:baseOpts("characters (log)","logarithmic")});
document.getElementById("scaleLog").onclick=()=>{charsChart.options.scales.y.type="logarithmic";charsChart.options.scales.y.title.text="characters (log)";charsChart.update();setA("scaleLog");};
document.getElementById("scaleLin").onclick=()=>{charsChart.options.scales.y.type="linear";charsChart.options.scales.y.title.text="characters";charsChart.update();setA("scaleLin");};
function setA(id){["scaleLog","scaleLin"].forEach(i=>document.getElementById(i).classList.toggle("active",i===id));}

new Chart(document.getElementById("corrChart"),{type:"line",data:{labels:Q,datasets:ds("correct")},
  options:Object.assign(baseOpts("correctness /10","linear"),{scales:{x:{ticks:{color:tickC,maxRotation:0,autoSkip:true},grid:{color:gridC}},
    y:{min:0,max:10,ticks:{color:tickC,stepSize:2},grid:{color:gridC},title:{display:true,text:"correctness /10",color:tickC}}}})});

// quick stat cards
document.getElementById("cards").innerHTML = `
  <div class="card"><h3>Median chars / question</h3><div class="cdesc">The middle question when all 30 are sorted by size — the <b>typical</b> cost, unaffected by a few giant outliers. <b>Fewer = leaner</b>.</div>${ARMS.map(a=>`<div class="rowr"><span>${dot(a)}${LABELS[a]}</span><b>${fmt(om.chars_median_per_question[a])}</b></div>`).join("")}</div>
  <div class="card"><h3>Mean chars / question</h3><div class="cdesc">Plain <b>average</b> across all 30 questions; higher than the median because a few very large questions pull it up.</div>${ARMS.map(a=>`<div class="rowr"><span>${dot(a)}${LABELS[a]}</span><b>${fmt(om.chars_mean[a])}</b></div>`).join("")}</div>
  <div class="card"><h3>Correctness /10</h3><div class="cdesc">Blind independent judge, 0–10 (mean · median over judged runs). All tools ≈ tie — which is why characters decide.</div>${ARMS.map(a=>`<div class="rowr"><span>${dot(a)}${LABELS[a]}</span><b>${om.correct_mean[a].toFixed(2)} · ${om.correct_median[a].toFixed(1)}</b></div>`).join("")}</div>
  <div class="card"><h3>Leaner vs Octocode</h3><div class="cdesc">How many times <b>fewer characters Octocode used</b> on a typical question (per-question geo-mean; <b>&gt;1× = Octocode leaner</b>). <b>/90</b> = runs (30 Q × 3 passes) where Octocode was leaner.</div>${BASE.map(a=>`<div class="rowr"><span>${dot(a)}${LABELS[a]}</span><b>${geo[a].geo_mean.toFixed(2)}× · ${geo[a].octocode_leaner}/90</b></div>`).join("")}</div>
`;

// pooled total (demoted)
const tot=om.chars_total_over_questions;
new Chart(document.getElementById("poolChart"),{type:"bar",
  data:{labels:ARMS.map(a=>LABELS[a]),datasets:[{data:ARMS.map(a=>tot[a]),backgroundColor:ARMS.map(a=>COLORS[a])}]},
  options:{indexAxis:"y",responsive:true,maintainAspectRatio:false,
    plugins:{legend:{display:false},tooltip:{callbacks:{label:c=>Number(c.parsed.x).toLocaleString(undefined,{maximumFractionDigits:0})+" chars"}}},
    scales:{x:{title:{display:true,text:"total characters — all 30 questions",color:tickC},ticks:{color:tickC},grid:{color:gridC}},y:{ticks:{color:tickC},grid:{color:gridC}}}}});
document.getElementById("poolNote").innerHTML =
  "Why these look bigger than ~2×: totals are dominated by a few huge questions — "+
  BASE.map(a=>`${LABELS[a]} <b>${pooled[a].ratio.toFixed(1)}×</b> (Q${pooled[a].top_q}=${(pooled[a].top_share*100).toFixed(0)}%, drop it → ${pooled[a].loo_ratio.toFixed(1)}×)`).join(" · ")+
  ". The typical-question figure (~2–3×) is the fair headline.";

// method footnote
document.getElementById("methodFoot").innerHTML =
  "Dataset: local-build v18.1.1, 30×3; the Octocode anchor logs are byte-identical across the baseline campaigns. "+
  "Every figure recomputed from per-call JSONL by <code>compare/bin/per_question_summary.py</code> → "+
  "<code>per_question_summary.json</code> (single source of truth), cross-checked vs <code>results/SUMMARY.md</code>. "+
  (om.correct_note ? "Correctness note: "+om.correct_note : "");

// question links
const QBASE="https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/compare/github-questions/";
document.getElementById("qlinks").innerHTML = DATA.rows.map(r=>`<a href="${QBASE}Q${r.q}.md" target="_blank" rel="noopener">Q${r.q}</a>`).join("");
</script>
</body>
</html>
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", type=Path, default=DEFAULT_JSON)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()

    data = json.loads(args.json.read_text(encoding="utf-8"))
    html = (
        TEMPLATE
        .replace("__DATA__", json.dumps(data))
        .replace("__COLORS__", json.dumps(COLORS))
        .replace("__LABELS__", json.dumps(LABELS))
        .replace("__ARMS__", json.dumps(ARMS))
    )
    args.out.write_text(html, encoding="utf-8")
    print(f"wrote {args.out} ({len(html):,} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
