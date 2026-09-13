"""Expanded immutable-reference localFetch parity and continuation sensor."""
import argparse, hashlib, json, os, subprocess, sys
from pathlib import Path

FIXED_TIME = 1600000000.443

def write(root, name, data):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    raw = data if isinstance(data, bytes) else data.encode()
    path.write_bytes(raw)
    os.utime(path, (FIXED_TIME, FIXED_TIME))
    return str(path)

def fixtures(root):
    root.mkdir(parents=True, exist_ok=True)
    unicode_text = "α😀one\r\nβ two\r\nneedle שלום\r\nlast\r\n"
    long_text = "λ" * 9000 + "\nlast\n"
    many = "".join(f"line-{i:04}\n" for i in range(250))
    match = "before\nneedle one\ngap\ngap\nneedle two\nafter\n"
    markdown = "# Root\n```\n## hidden\n```\nparagraph\n  ## Child ###\n"
    paths = {
        "unicode": write(root, "unicode.txt", unicode_text),
        "long": write(root, "long.txt", long_text),
        "many": write(root, "many.txt", many),
        "match": write(root, "match.txt", match),
        "markdown": write(root, "outline.mdx", markdown),
        "secret": write(root, "credential.txt", "token ghp_" + "a" * 36 + "\nvisible\n"),
        "malformed": write(root, "malformed.txt", b"prefix\nvalid\n" + b"\xff" + b"tail\n"),
        "binary": write(root, "binary.dat", b"text\x00binary"),
        "source_limit": write(root, "source-limit.txt", "s" * (101 * 1024)),
        "view_limit": write(root, "view-limit.txt", "v" * 60000),
        "security_limit": write(root, "security-limit.txt", ("safe-line-" + "z" * 90 + "\n") * 100001),
    }
    return paths, {"unicode": unicode_text, "long": long_text, "many": many, "match": match}

def environment(home, root):
    home.mkdir(parents=True, exist_ok=True)
    return {"PATH":"/usr/bin:/bin", "HOME":str(home), "OCTOCODE_HOME":str(home),
            "ENABLE_LOCAL":"true", "ENABLE_CLONE":"false", "NO_COLOR":"1",
            "ALLOWED_PATHS":str(root), "WORKSPACE_ROOT":str(root),
            "OCTOCODE_ENABLE_STATS":"false", "OCTOCODE_OUTPUT_FORMAT":"json"}

def invoke(argv, query, home, root):
    command = [*argv, "tools", "localFetch", "--queries", json.dumps(query), "--json", "--compact"]
    run = subprocess.run(command, cwd=root, env=environment(home, root), text=True,
                         capture_output=True, timeout=30)
    try: payload = json.loads(run.stdout)
    except json.JSONDecodeError: payload = None
    return {"exit":run.returncode, "payload":payload, "stdout":run.stdout, "stderr":run.stderr}

def continuation_chain(argv, query, home, root):
    pages, seen = [], set()
    while True:
        result = invoke(argv, query, home, root)
        pages.append(result)
        if result["payload"] is None or "results" not in result["payload"]: break
        data = result["payload"]["results"][0].get("data", {})
        nxt = data.get("next", {}).get("continue")
        if not nxt: break
        query = nxt["query"]
        key = json.dumps(query, sort_keys=True)
        if key in seen: raise RuntimeError("continuation cycle")
        seen.add(key)
        if len(pages) > 1000: raise RuntimeError("continuation did not terminate")
    return pages

def equivalent(left, right):
    if left["exit"] != right["exit"] or left["stderr"] != right["stderr"]: return False
    if left["payload"] is None or right["payload"] is None:
        return left["stdout"] == right["stdout"]
    return left["payload"] == right["payload"]

def intentional_decision(ident, reference, candidate):
    """Typed migration decisions where the RFC intentionally fixes legacy behavior."""
    if len(reference) != 1 or len(candidate) != 1: return None
    ref = reference[0].get("payload", {}).get("results", [{}])[0].get("data", {})
    cand = candidate[0].get("payload", {}).get("results", [{}])[0].get("data", {})
    if ident == "match-regex-lookbehind" and ref.get("errorCode") == "toolExecutionFailed" and cand.get("content") == "needle one\n":
        return {"kind":"intentional-native-change", "legacy":"ECMAScript lookbehind rejected by the Rust-regex implementation", "native":"lookbehind executed by the bounded isolated ECMAScript worker"}
    return None

def page_content(pages):
    return "".join(p["payload"]["results"][0]["data"].get("content", "") for p in pages)

def cases(paths):
    return [
      ("line-chain", {"path":paths["many"],"chunkType":"lines","limit":17}, True),
      ("byte-chain-unicode", {"path":paths["unicode"],"chunkType":"bytes","limit":5}, True),
      ("long-line-chain", {"path":paths["long"],"chunkType":"lines","limit":1}, True),
      ("match-chain", {"path":paths["match"],"matchString":"needle","contextLines":0,"chunkType":"lines","limit":1}, True),
      ("context-bytes", {"path":paths["unicode"],"matchString":"שלום","contextBytes":4,"chunkType":"bytes","limit":7}, True),
      ("unicode-offset", {"path":paths["unicode"],"chunkType":"bytes","offset":2,"limit":7}, False),
      ("crlf-range", {"path":paths["unicode"],"startLine":2,"endLine":4,"chunkType":"lines","limit":2}, True),
      ("match-ignore-case", {"path":paths["match"],"matchString":"NEEDLE","matchStringCaseSensitive":False,"contextLines":1}, False),
      ("match-regex", {"path":paths["match"],"matchString":"needle (one|two)","matchStringIsRegex":True,"contextLines":0}, False),
      ("match-regex-lookbehind", {"path":paths["match"],"matchString":"(?<=needle )one","matchStringIsRegex":True,"contextLines":0}, False),
      ("match-minify-fallback", {"path":paths["match"],"matchString":"needle","contextLines":0,"minify":"standard"}, False),
      ("markdown-symbols", {"path":paths["markdown"],"fullContent":True,"minify":"symbols"}, False),
      ("secret-redaction", {"path":paths["secret"],"fullContent":True}, False),
      ("malformed-utf8", {"path":paths["malformed"],"fullContent":True}, False),
      ("binary", {"path":paths["binary"],"fullContent":True}, False),
      ("source-limit", {"path":paths["source_limit"],"fullContent":True}, False),
      ("view-limit", {"path":paths["view_limit"],"fullContent":True,"minify":"standard"}, False),
      ("security-limit", {"path":paths["security_limit"]}, False),
      ("all-optionals", {"path":paths["match"],"matchString":"needle","matchStringIsRegex":False,"matchStringCaseSensitive":True,"contextLines":0,"chunkType":"bytes","offset":0,"limit":8,"minify":"none"}, True),
    ]

def main():
    ap=argparse.ArgumentParser(); ap.add_argument("--node",required=True); ap.add_argument("--reference",required=True); ap.add_argument("--candidate",required=True); ap.add_argument("--state",type=Path,required=True); args=ap.parse_args()
    root=args.state.resolve(); paths,texts=fixtures(root/"expanded-fixtures")
    ref_argv=[args.node,args.reference]; cand_argv=[args.candidate]
    rows=[]
    for ident,query,chain in cases(paths):
        ref = continuation_chain(ref_argv,query,root/"expanded-reference-home",root) if chain else [invoke(ref_argv,query,root/"expanded-reference-home",root)]
        cand = continuation_chain(cand_argv,query,root/"expanded-candidate-home",root) if chain else [invoke(cand_argv,query,root/"expanded-candidate-home",root)]
        exact=len(ref)==len(cand) and all(equivalent(a,b) for a,b in zip(ref,cand))
        decision = intentional_decision(ident, ref, cand)
        equal=decision is not None or exact
        union_ok=True
        if chain and ident in {"line-chain","byte-chain-unicode","long-line-chain"}:
            expected=texts[{"line-chain":"many","byte-chain-unicode":"unicode","long-line-chain":"long"}[ident]]
            union_ok=page_content(ref)==expected and page_content(cand)==expected
        if chain:
            union_ok = union_ok and all(p["payload"] is not None for p in ref+cand)
        bounded_ref=[]; bounded_cand=[]
        for left,right in zip(ref,cand):
            left_next=left.get("payload",{}).get("results",[{}])[0].get("data",{}).get("next",{}).get("readBoundedLines")
            right_next=right.get("payload",{}).get("results",[{}])[0].get("data",{}).get("next",{}).get("readBoundedLines")
            if bool(left_next) != bool(right_next): equal=False
            if left_next and right_next:
                bounded_ref.append(invoke(ref_argv,left_next["query"],root/"reference-home",root))
                bounded_cand.append(invoke(cand_argv,right_next["query"],root/"candidate-home",root))
        bounded_equal=len(bounded_ref)==len(bounded_cand) and all(equivalent(a,b) and a["exit"]==0 and b["exit"]==0 for a,b in zip(bounded_ref,bounded_cand))
        if bounded_ref and not all(x.get("payload",{}).get("results",[{}])[0].get("data",{}).get("content") for x in bounded_ref+bounded_cand): bounded_equal=False
        rows.append({"id":ident,"query":query,"reference":ref,"candidate":cand,"exact":exact,"equal":equal,"unionCoverage":union_ok,"boundedReference":bounded_ref,"boundedCandidate":bounded_cand,"boundedEqual":bounded_equal, **({"decision":decision} if decision else {})})
    # Mixed bulk validates independent row success/error shaping and order.
    mixed=[{"path":paths["unicode"],"startLine":1,"endLine":1},{"path":paths["binary"],"fullContent":True},{"path":str(root/"expanded-fixtures/missing.txt"),"fullContent":True}]
    rr=invoke(ref_argv,mixed,root/"expanded-reference-home",root); cc=invoke(cand_argv,mixed,root/"expanded-candidate-home",root)
    mixed_exact=equivalent(rr,cc)
    rows.append({"id":"mixed-batch","query":mixed,"reference":[rr],"candidate":[cc],"exact":mixed_exact,"equal":mixed_exact,"unionCoverage":True,"boundedReference":[],"boundedCandidate":[],"boundedEqual":True})
    report={"scope":"S7 expanded localFetch differential and continuation coverage","fixtureVersion":1,"cases":rows,"exactPassed":sum(r["exact"] and r["unionCoverage"] and r["boundedEqual"] for r in rows),"acceptedBehaviorDecisions":sum(bool(r.get("decision")) for r in rows),"passed":sum(r["equal"] and r["unionCoverage"] and r["boundedEqual"] for r in rows),"total":len(rows),"suiteSha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    (root/"expanded-report.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps({k:v for k,v in report.items() if k!="cases"}))
    return 0 if report["passed"]==report["total"] else 1

if __name__=="__main__": sys.exit(main())
