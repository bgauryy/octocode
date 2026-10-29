#!/usr/bin/env python3
# Read Headroom preflight --warmup output from stdin; print OK if it passed cleanly.
# Tolerates non-JSON preamble (HF/model warnings print to stdout before the JSON).
# Passed = parseable JSON object, empty "failures", and no protected (compression-off) route.
import sys, json

raw = sys.stdin.read()
start = raw.find("{")
end = raw.rfind("}")
if start == -1 or end == -1 or end < start:
    print("no-json")
    sys.exit(0)
try:
    d = json.loads(raw[start:end + 1])
except Exception:
    print("parse-error")
    sys.exit(0)

fails = d.get("failures", ["<missing>"])
blob = json.dumps(d.get("warmup") or d.get("results") or d)
if fails == [] and "router:protected" not in blob:
    print("OK")
else:
    print("failures=" + json.dumps(fails) if fails else "protected-route")
