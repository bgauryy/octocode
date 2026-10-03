#!/bin/bash
# Config precedence probe: DISABLE_TOOLS / tools.disabled at 5 layers; observe via `octocode scheme` availability
D=$(cd "$(dirname "$0")" && pwd); H=$D/home; W=$D/ws
dis() { (cd "$W" && env -u DISABLE_TOOLS -u TOOLS_TO_RUN OCTOCODE_HOME="$H" "$@" octocode scheme 2>"$D/last.err" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.tools.filter(t=>!t.availability.enabled).map(t=>t.name+":"+JSON.stringify(t.availability)).join(" | "))})'); echo "  stderr: $(head -c 400 "$D/last.err")"; }
echo '{ "tools": { "disabled": ["ghSearchRepo"] } }' > $H/.octocoderc
echo '{ "tools": { "disabled": ["ghSearchCode"] } }' > $W/.octocode/.octocoderc
echo 'DISABLE_TOOLS=ghStructure' > $H/.env
echo 'DISABLE_TOOLS=ghGetFileContent' > $W/.octocode/.env
echo "== L1 all five layers (+ env ghSearchHistory)"; dis DISABLE_TOOLS=ghSearchHistory
echo "== L2 no env -> expect ws .env ghGetFileContent"; dis
rm $W/.octocode/.env; echo "== L3 no ws .env -> expect global .env ghStructure"; dis
rm $H/.env; echo "== L4 no global .env -> expect ws rc ghSearchCode"; dis
rm $W/.octocode/.octocoderc; echo "== L5 no ws rc -> expect global rc ghSearchRepo"; dis
echo "== L6 blank ws .env value falls back"; echo 'DISABLE_TOOLS=ghStructure' > $H/.env; echo 'DISABLE_TOOLS=' > $W/.octocode/.env; dis
rm -f $H/.env $W/.octocode/.env $H/.octocoderc
