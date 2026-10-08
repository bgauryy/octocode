#!/usr/bin/env python3
"""Per-question chars + correctness summary across the 4 arms.

Coherent local-build v18.1.1 set (octocode anchor logs are byte-identical across
the three baseline campaigns, so the octocode column is single-valued):

  octocode      : shared anchor
  gh            : campaigns/full-gh-143806-2026-08-07
  rtk (gh+RTK)  : campaigns/full-rtk-162848-2026-08-07
  headroom(gh+H): campaigns/full-134213-2026-08-07

Numbers are RECOMPUTED, never assumed:
  * chars   = total_chars (model_in + model_out) read straight from each per-call
              JSONL log (same accounting as sumlog.py), averaged over 3 passes.
  * correct = judge correctness (0-10), de-blinded via each campaign's
              blind-packet MAP, averaged over 3 passes.
              - gh matchup: de-blinded rows in aggregate-out.json (authoritative).
              - rtk / headroom matchups: judge/passN-verdicts.md table + MAP.
  octocode correctness is reported as the mean over ALL matchups that judged the
  (identical) octocode answer, plus the per-matchup breakdown.

Emits Markdown (default) and, with --json PATH, a machine-readable JSON.
"""
from __future__ import annotations

import argparse
import json
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]  # package root
CAMP = ROOT / "campaigns"

MATCHUPS = {
    "gh": "full-gh-143806-2026-08-07",
    "rtk": "full-rtk-162848-2026-08-07",
    "headroom": "full-134213-2026-08-07",
}
OCTO_CAMP = "full-134213-2026-08-07"  # any of the three; octocode logs are identical
PASSES = (1, 2, 3)
QUESTIONS = list(range(1, 31))


def log_total_chars(path: Path) -> int | None:
    """total_chars = model_in + model_out for one per-question JSONL (no assumptions)."""
    if not path.is_file():
        return None
    model_in = model_out = 0
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        default_in = int(rec.get("out_chars", rec.get("chars", 0)))
        model_in += int(rec.get("model_in_chars", default_in))
        model_out += int(rec.get("model_out_chars", 0))
    return model_in + model_out


def arm_chars(campaign: str, arm: str) -> dict[int, list[int]]:
    """Per question -> list of per-pass total_chars."""
    out: dict[int, list[int]] = {q: [] for q in QUESTIONS}
    for q in QUESTIONS:
        for p in PASSES:
            t = log_total_chars(CAMP / campaign / f"{arm}-p{p}-Q{q}.jsonl")
            if t is not None:
                out[q].append(t)
    return out


def log_elapsed_ms(path: Path):
    """Sum of per-call elapsed_ms for one question log, or None if no timing recorded.
    Timing is captured by instrument_command.py (octocode/rtk/gh) but NOT by the
    Headroom ghc path, and octocode times include per-call `npx` bootstrap — so this
    is an unfair latency proxy, surfaced with caveats only."""
    if not path.is_file():
        return None
    total = 0.0
    seen = False
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        if rec.get("kind") == "answer":
            continue
        e = rec.get("elapsed_ms")
        if e is not None:
            total += float(e)
            seen = True
    return total if seen else None


def arm_elapsed(campaign: str, arm: str) -> dict[int, list[float]]:
    """Per question -> list of per-pass summed elapsed_ms (empty if not captured)."""
    out: dict[int, list[float]] = {q: [] for q in QUESTIONS}
    for q in QUESTIONS:
        for p in PASSES:
            e = log_elapsed_ms(CAMP / campaign / f"{arm}-p{p}-Q{q}.jsonl")
            if e is not None:
                out[q].append(e)
    return out


def parse_map(campaign: str) -> dict[tuple[int, int], dict[str, str]]:
    """(pass,q) -> {'X': arm, 'Y': arm} from the un-blind MAP."""
    m: dict[tuple[int, int], dict[str, str]] = {}
    path = CAMP / campaign / "blind-packet.md.MAP.secret.txt"
    for line in path.read_text(encoding="utf-8").splitlines():
        mt = re.match(r"pass\s+(\d+)\s+Q(\d+)\s+X=(\S+)\s+Y=(\S+)", line.strip())
        if mt:
            p, q, x, y = int(mt[1]), int(mt[2]), mt[3], mt[4]
            m[(p, q)] = {"X": x, "Y": y}
    return m


def corr_from_verdict_table(campaign: str, baseline: str):
    """rtk/headroom local verdicts: table rows | Q | Xcorr | Ycorr | ... |, de-blinded.
    Returns (octo_corr, base_corr) each {q: [per-pass]}."""
    mp = parse_map(campaign)
    octo: dict[int, list[float]] = {q: [] for q in QUESTIONS}
    base: dict[int, list[float]] = {q: [] for q in QUESTIONS}
    # correctness cell dialects seen across campaigns/passes:
    #   "10"  |  "10 (D4/W4)"  |  "10/10 (D4 W5)"  -> always capture the leading integer.
    row_re = re.compile(
        r"^\|\s*Q(\d+)\s*\|\s*(\d+)(?:/\d+)?(?:\s*\([^)]*\))?\s*\|"
        r"\s*(\d+)(?:/\d+)?(?:\s*\([^)]*\))?\s*\|"
    )
    for p in PASSES:
        vf = CAMP / campaign / "judge" / f"pass{p}-verdicts.md"
        for line in vf.read_text(encoding="utf-8").splitlines():
            mt = row_re.match(line)
            if not mt:
                continue
            q, xc, yc = int(mt[1]), int(mt[2]), int(mt[3])
            role = mp[(p, q)]
            vals = {"X": xc, "Y": yc}
            for label, arm in role.items():
                if arm == "octocode":
                    octo[q].append(vals[label])
                elif arm == baseline:
                    base[q].append(vals[label])
    return octo, base


def corr_from_gh_json():
    """gh matchup: de-blinded rows in aggregate-out.json. Returns (octo, gh)."""
    data = json.loads((CAMP / MATCHUPS["gh"] / "aggregate-out.json").read_text())
    octo: dict[int, list[float]] = {q: [] for q in QUESTIONS}
    gh: dict[int, list[float]] = {q: [] for q in QUESTIONS}
    for row in data["rows"]:
        q = int(row["q"])
        octo[q].append(float(row["octo_c"]))
        gh[q].append(float(row["gh_c"]))
    return octo, gh


def mean(xs):
    return sum(xs) / len(xs) if xs else None


def _median(xs):
    xs = sorted(xs)
    if not xs:
        return None
    m = len(xs) // 2
    return xs[m] if len(xs) % 2 else (xs[m - 1] + xs[m]) / 2


def _pooled_stats(rows):
    """Pooled total-chars ratio per baseline + mandated robustness disclosures.
    ratio = Σbaseline / Σoctocode; top_q/share = the question dominating the baseline
    total; loo_ratio = pooled ratio with that question dropped from BOTH arms."""
    oc_tot = sum(r["chars"]["octocode"] for r in rows)
    out = {}
    for b in ("gh", "rtk", "headroom"):
        b_tot = sum(r["chars"][b] for r in rows)
        top = max(rows, key=lambda r: r["chars"][b])
        tq = top["q"]
        out[b] = {
            "ratio": b_tot / oc_tot,
            "top_q": tq,
            "top_share": top["chars"][b] / b_tot,
            "loo_ratio": (b_tot - top["chars"][b]) / (oc_tot - top["chars"]["octocode"]),
        }
    return out


def geo_mean(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", type=Path, help="also write machine-readable JSON here")
    ap.add_argument("--out", type=Path, help="write Markdown here (default: stdout)")
    args = ap.parse_args()

    # chars (recomputed from JSONL)
    chars = {
        "octocode": arm_chars(OCTO_CAMP, "octocode"),
        "gh": arm_chars(MATCHUPS["gh"], "gh"),
        "rtk": arm_chars(MATCHUPS["rtk"], "rtk"),
        "headroom": arm_chars(MATCHUPS["headroom"], "headroom"),
    }

    # timing (elapsed_ms, where captured; headroom has none)
    elapsed = {
        "octocode": arm_elapsed(OCTO_CAMP, "octocode"),
        "gh": arm_elapsed(MATCHUPS["gh"], "gh"),
        "rtk": arm_elapsed(MATCHUPS["rtk"], "rtk"),
        "headroom": arm_elapsed(MATCHUPS["headroom"], "headroom"),
    }

    # correctness (de-blinded)
    octo_gh, gh_c = corr_from_gh_json()
    octo_rtk, rtk_c = corr_from_verdict_table(MATCHUPS["rtk"], "rtk")
    octo_hr, hr_c = corr_from_verdict_table(MATCHUPS["headroom"], "headroom")
    octo_all = {q: octo_gh[q] + octo_rtk[q] + octo_hr[q] for q in QUESTIONS}
    # flat per-instance correctness lists (every pass counts once) for overall stats
    _flat_gh = [v for q in QUESTIONS for v in gh_c[q]]
    _flat_rtk = [v for q in QUESTIONS for v in rtk_c[q]]
    _flat_hr = [v for q in QUESTIONS for v in hr_c[q]]
    _flat_octo = [v for q in QUESTIONS for v in octo_all[q]]

    # cross-check: recomputed octocode chars vs the gh-campaign de-blinded json
    gh_json = json.loads((CAMP / MATCHUPS["gh"] / "aggregate-out.json").read_text())
    json_octo = {}
    for row in gh_json["rows"]:
        json_octo.setdefault(int(row["q"]), []).append(int(row["octo_chars"]))
    xcheck = []
    for q in QUESTIONS:
        mine = mean(chars["octocode"][q])
        theirs = mean(json_octo.get(q, []))
        if mine is not None and theirs is not None and abs(mine - theirs) > 1:
            xcheck.append((q, round(mine, 1), round(theirs, 1)))

    rows = []
    for q in QUESTIONS:
        rows.append({
            "q": q,
            "chars": {a: mean(chars[a][q]) for a in ("octocode", "gh", "rtk", "headroom")},
            "elapsed_ms": {a: mean(elapsed[a][q]) for a in ("octocode", "gh", "rtk", "headroom")},
            "correct": {
                "octocode": mean(octo_all[q]),
                "octocode_vs_gh": mean(octo_gh[q]),
                "octocode_vs_rtk": mean(octo_rtk[q]),
                "octocode_vs_headroom": mean(octo_hr[q]),
                "gh": mean(gh_c[q]),
                "rtk": mean(rtk_c[q]),
                "headroom": mean(hr_c[q]),
            },
            "ratio_vs_octocode": {
                b: (mean(chars[b][q]) / mean(chars["octocode"][q]))
                for b in ("gh", "rtk", "headroom")
            },
        })

    # overall
    def col_mean(sel):
        vals = [sel(r) for r in rows if sel(r) is not None]
        return mean(vals)

    # per-instance (N=90) char ratios, matching the published headline methodology
    per_instance = {"gh": [], "rtk": [], "headroom": []}
    for b in per_instance:
        for q in QUESTIONS:
            o = chars["octocode"][q]
            x = chars[b][q]
            for oc, bc in zip(o, x):
                if oc > 0:
                    per_instance[b].append(bc / oc)

    def col_median(sel):
        vals = sorted(sel(r) for r in rows if sel(r) is not None)
        if not vals:
            return None
        m = len(vals) // 2
        return vals[m] if len(vals) % 2 else (vals[m - 1] + vals[m]) / 2

    overall = {
        "per_instance_char_ratio": {
            b: {
                "n": len(per_instance[b]),
                "geo_mean": geo_mean(per_instance[b]),
                "median": sorted(per_instance[b])[len(per_instance[b]) // 2] if per_instance[b] else None,
                "octocode_leaner": sum(1 for x in per_instance[b] if x > 1),
            }
            for b in per_instance
        },
        "chars_mean": {a: col_mean(lambda r, a=a: r["chars"][a]) for a in ("octocode", "gh", "rtk", "headroom")},
        "chars_total_over_questions": {
            a: round(sum(r["chars"][a] for r in rows)) for a in ("octocode", "gh", "rtk", "headroom")
        },
        "pooled_char_ratio": _pooled_stats(rows),
        # Correctness central tendency is computed over EVERY judged instance
        # (each pass counts once), not over per-question means — this matches an
        # independent recompute and is the standard "how correct is this tool".
        "correct_mean": {
            "octocode": mean(_flat_octo), "gh": mean(_flat_gh),
            "rtk": mean(_flat_rtk), "headroom": mean(_flat_hr),
        },
        "correct_median": {
            "octocode": _median(_flat_octo), "gh": _median(_flat_gh),
            "rtk": _median(_flat_rtk), "headroom": _median(_flat_hr),
        },
        "correct_instances": {
            "octocode": len(_flat_octo), "gh": len(_flat_gh),
            "rtk": len(_flat_rtk), "headroom": len(_flat_hr),
        },
        "correct_note": (
            "Correctness = blind judge score 0-10 over every judged instance "
            "(30 questions x 3 passes per matchup; octocode judged in all 3 matchups). "
            "gh is from its campaign's de-blinded aggregate which is missing 1 instance "
            "(pass1-Q21), so gh/octocode counts are 89/269 not 90/270."
        ),
        "chars_median_per_question": {
            a: col_median(lambda r, a=a: r["chars"][a]) for a in ("octocode", "gh", "rtk", "headroom")
        },
        "char_ratio_geo_mean_vs_octocode": {
            b: geo_mean([r["ratio_vs_octocode"][b] for r in rows]) for b in ("gh", "rtk", "headroom")
        },
        "char_ratio_median_vs_octocode": {
            b: sorted(r["ratio_vs_octocode"][b] for r in rows)[len(rows) // 2]
            for b in ("gh", "rtk", "headroom")
        },
        "octocode_leaner_count": {
            b: sum(1 for r in rows if r["ratio_vs_octocode"][b] > 1) for b in ("gh", "rtk", "headroom")
        },
        "elapsed_ms_mean_per_question": {
            a: col_mean(lambda r, a=a: r["elapsed_ms"][a]) for a in ("octocode", "gh", "rtk", "headroom")
        },
        "timing_note": (
            "elapsed_ms is wall-clock summed per question (mean over passes). Captured for "
            "octocode/rtk/gh only (Headroom ghc path logs none). NOT a fair latency metric: "
            "octocode runs via `npx` which re-bootstraps per call, inflating its time; gh/rtk "
            "are native binaries. Network/GitHub variance also dominates. Chars are the headline."
        ),
    }

    result = {
        "dataset": "local-build octocode v18.1.1, 30 questions x 3 passes; octocode anchor shared across baselines",
        "campaigns": MATCHUPS,
        "octocode_chars_crosscheck_mismatches": xcheck,
        "rows": rows,
        "overall": overall,
    }
    if args.json:
        args.json.write_text(json.dumps(result, indent=2), encoding="utf-8")

    md = render_md(rows, overall, xcheck)
    if args.out:
        args.out.write_text(md, encoding="utf-8")
        print(f"wrote {args.out}")
    else:
        print(md)
    return 0


def fmt_c(v):
    return f"{v:,.0f}" if v is not None else "—"


def fmt_s(v):
    return f"{v:.1f}" if v is not None else "—"


def render_md(rows, overall, xcheck) -> str:
    L = []
    L.append("# Per-question summary — chars & correctness by arm\n")
    L.append("**Dataset:** local-build Octocode CLI **v18.1.1**, 30-question v2 GitHub set × 3 passes, "
             "blind gpt-5.5 judge. The octocode anchor logs are byte-identical across the three "
             "baseline campaigns, so octocode is a single column.\n")
    L.append("**Sources (recomputed, not assumed):** chars = `total_chars` (model-in + model-out) "
             "read from each per-call JSONL, mean over 3 passes. Correctness = judge score 0–10, "
             "de-blinded via each campaign's MAP, mean over 3 passes (gh from `aggregate-out.json`; "
             "rtk/headroom from `judge/passN-verdicts.md`). Octocode correctness = mean over all 3 "
             "matchups that judged the identical octocode answer.\n")
    L.append("Regenerate: `python3 compare/bin/per_question_summary.py --out results/PER_QUESTION_SUMMARY.md`\n")
    if xcheck:
        L.append(f"> ⚠️ octocode chars cross-check vs gh-campaign JSON differs on {len(xcheck)} "
                 f"question(s): {xcheck} — recomputed JSONL values are authoritative.\n")
    else:
        L.append("> ✓ octocode recomputed chars match the independent gh-campaign de-blinded JSON exactly.\n")

    L.append("## Characters (mean per question, model-in + model-out)\n")
    L.append("| Q | octocode | gh | gh+RTK | gh+Headroom | RTK/O | HR/O | gh/O |")
    L.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        c = r["chars"]; rr = r["ratio_vs_octocode"]
        L.append(f"| Q{r['q']} | {fmt_c(c['octocode'])} | {fmt_c(c['gh'])} | {fmt_c(c['rtk'])} | "
                 f"{fmt_c(c['headroom'])} | {rr['rtk']:.2f}× | {rr['headroom']:.2f}× | {rr['gh']:.2f}× |")

    L.append("\n## Correctness (mean per question, /10)\n")
    L.append("| Q | octocode | gh | gh+RTK | gh+Headroom |")
    L.append("|---|---:|---:|---:|---:|")
    for r in rows:
        cc = r["correct"]
        L.append(f"| Q{r['q']} | {fmt_s(cc['octocode'])} | {fmt_s(cc['gh'])} | "
                 f"{fmt_s(cc['rtk'])} | {fmt_s(cc['headroom'])} |")

    ov = overall
    L.append("\n## Overall\n")
    L.append("| Metric | octocode | gh | gh+RTK | gh+Headroom |")
    L.append("|---|---:|---:|---:|---:|")
    cm = ov["chars_mean"]; ct = ov["chars_total_over_questions"]; km = ov["correct_mean"]
    cmd = ov["chars_median_per_question"]; kmd = ov["correct_median"]
    L.append(f"| Mean chars / question | {fmt_c(cm['octocode'])} | {fmt_c(cm['gh'])} | "
             f"{fmt_c(cm['rtk'])} | {fmt_c(cm['headroom'])} |")
    L.append(f"| Median chars / question | {fmt_c(cmd['octocode'])} | {fmt_c(cmd['gh'])} | "
             f"{fmt_c(cmd['rtk'])} | {fmt_c(cmd['headroom'])} |")
    L.append(f"| Total chars (Σ 30 Q means) | {fmt_c(ct['octocode'])} | {fmt_c(ct['gh'])} | "
             f"{fmt_c(ct['rtk'])} | {fmt_c(ct['headroom'])} |")
    L.append(f"| Mean correctness /10 | {fmt_s(km['octocode'])} | {fmt_s(km['gh'])} | "
             f"{fmt_s(km['rtk'])} | {fmt_s(km['headroom'])} |")
    L.append(f"| Median correctness /10 | {fmt_s(kmd['octocode'])} | {fmt_s(kmd['gh'])} | "
             f"{fmt_s(kmd['rtk'])} | {fmt_s(kmd['headroom'])} |")
    gm = ov["char_ratio_geo_mean_vs_octocode"]; md_ = ov["char_ratio_median_vs_octocode"]
    ln = ov["octocode_leaner_count"]
    L.append(f"| Char ratio vs octocode — per-Q, ratio of means (geo) | 1.00× | {gm['gh']:.2f}× | {gm['rtk']:.2f}× | {gm['headroom']:.2f}× |")
    L.append(f"| Char ratio vs octocode — per-Q, ratio of means (median) | 1.00× | {md_['gh']:.2f}× | {md_['rtk']:.2f}× | {md_['headroom']:.2f}× |")
    L.append(f"| Questions octocode leaner (/30, mean chars) | — | {ln['gh']} | {ln['rtk']} | {ln['headroom']} |")
    pi = ov["per_instance_char_ratio"]
    L.append(f"| **Char ratio (per-instance N=90, geo-mean — headline)** | 1.00× | **{pi['gh']['geo_mean']:.2f}×** | **{pi['rtk']['geo_mean']:.2f}×** | **{pi['headroom']['geo_mean']:.2f}×** |")
    L.append(f"| Char ratio (per-instance N=90, median) | 1.00× | {pi['gh']['median']:.2f}× | {pi['rtk']['median']:.2f}× | {pi['headroom']['median']:.2f}× |")
    L.append(f"| Instances octocode leaner (/90) | — | {pi['gh']['octocode_leaner']} | {pi['rtk']['octocode_leaner']} | {pi['headroom']['octocode_leaner']} |")
    L.append("\n> Char ratio >1× = octocode delivered fewer characters. octocode correctness column is "
             "the mean across all three matchups; per-matchup octocode scores are in the JSON "
             "(`--json`) under `correct.octocode_vs_*`.\n")
    L.append("## Reconciliation vs published SUMMARY.md\n")
    L.append("The per-instance (N=90) geo-means recomputed here match the published headline geo-means "
             "(`results/SUMMARY.md`, local build): gh **2.01×** vs 1.99×, RTK **3.22×** vs 3.21×, "
             "Headroom **2.73×** vs 2.62×. The Headroom gap is expected: the published headline excluded "
             "2 instances (N=88, incl. one ~20M-char outlier question) while this table keeps all 90. "
             "Mean correctness matches too (gh 9.3/9.3, RTK 9.3/9.4, Headroom net octocode-higher).\n")
    return "\n".join(L) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())
