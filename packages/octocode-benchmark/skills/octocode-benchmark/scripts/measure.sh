#!/usr/bin/env bash
# Generic char-measurement wrapper — the FALLBACK for an arm with no dedicated
# bin/ wrapper (canonical arms use bin/octoc, bin/rtkm, bin/ghc). It prints the
# real output unchanged and appends one JSONL row counting Unicode code points
# both directions: the command string (model_out) and the returned output
# (model_in). Never trust a self-reported count.
#
# Usage:  measure.sh <ARM> <QID> <LABEL> -- <command...>
#   ARM   arm id (octocode | rtk | headroom | gh)
#   QID   question id, e.g. Q1
#   LABEL short step label
# Requires $CURRENT_RUN (a file holding the run dir) beside this script, or set
# RUNDIR in the environment.
set -uo pipefail
ARM="$1"; QID="$2"; LABEL="$3"; shift 3; [ "${1:-}" = "--" ] && shift
RUNDIR="${RUNDIR:-$(cat "$(dirname "$0")/CURRENT_RUN")}"; OUT="$RUNDIR/$ARM"; mkdir -p "$OUT"
CMD="$*"; T0=$(perl -MTime::HiRes=time -e 'printf "%.0f",time()*1000')
O="$("$@" 2>&1)"; RC=$?; T1=$(perl -MTime::HiRes=time -e 'printf "%.0f",time()*1000')
IN=$(printf '%s' "$O"  | perl -CS -ne '$c+=length; END{print $c}')
OUTC=$(printf '%s' "$CMD" | perl -CS -ne '$c+=length; END{print $c}')
{ echo "### [$QID/$LABEL] rc=$RC in=$IN out=$OUTC total=$((IN+OUTC)) ms=$((T1-T0))"; echo "\$ $CMD"; echo "$O"; } >> "$OUT/$QID.out.txt"
python3 -c 'import json,sys;open(sys.argv[1],"a").write(json.dumps({"arm":sys.argv[2],"qid":sys.argv[3],"label":sys.argv[4],"model_in_chars":int(sys.argv[5]),"model_out_chars":int(sys.argv[6]),"total_chars":int(sys.argv[5])+int(sys.argv[6]),"chars":int(sys.argv[5]),"ms":int(sys.argv[7]),"rc":int(sys.argv[8]),"cmd":sys.argv[9]})+"\n")' "$OUT/calls.jsonl" "$ARM" "$QID" "$LABEL" "$IN" "$OUTC" "$((T1-T0))" "$RC" "$CMD"
printf '%s\n' "$O"
