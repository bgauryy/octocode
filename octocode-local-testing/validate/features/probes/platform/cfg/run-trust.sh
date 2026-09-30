#!/bin/bash
D=$(cd "$(dirname "$0")" && pwd); H=$D/home; W=$D/ws
beta() { (cd "$W" && env -u OCTOCODE_BETA OCTOCODE_HOME="$H" "$@" octocode scheme 2>"$D/last.err" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log("toolCount="+j.toolCount, j.tools.filter(t=>/^ast(Topology|Rewrite)$/.test(t.name)).map(t=>t.name+":"+t.availability.enabled).join(" "))})'); echo "  stderr: $(head -c 500 "$D/last.err")"; }
rm -f $H/.env $H/.octocoderc $W/.octocode/.env $W/.octocode/.octocoderc
echo "== T0 baseline (no beta anywhere)"; beta
echo "== T1 env OCTOCODE_BETA=true"; beta OCTOCODE_BETA=true
echo "== T2 global .env OCTOCODE_BETA=true (home-trusted)"; echo OCTOCODE_BETA=true > $H/.env; beta; rm $H/.env
echo "== T3 workspace .env OCTOCODE_BETA=true (dotenv:home -> expect ignored)"; echo OCTOCODE_BETA=true > $W/.octocode/.env; beta; rm $W/.octocode/.env
echo "== T4 global rc local.beta=true"; echo '{"local":{"beta":true}}' > $H/.octocoderc; beta; rm $H/.octocoderc
echo "== T5 workspace rc local.beta=true (protected-bound field?)"; echo '{"local":{"beta":true}}' > $W/.octocode/.octocoderc; beta; rm $W/.octocode/.octocoderc
echo "== T6 workspace rc storage.mode=persistent while global memory"; echo '{"storage":{"mode":"memory"}}' > $H/.octocoderc; echo '{"storage":{"mode":"persistent"}}' > $W/.octocode/.octocoderc; (cd $W && OCTOCODE_HOME=$H octocode config 2>&1 | head -4); rm $W/.octocode/.octocoderc $H/.octocoderc
echo "== T7 misconfig: bad value + unknown key in ws rc"; echo '{"network":{"maxRetries":"x"},"local":{"enableLocl":true}}' > $W/.octocode/.octocoderc; (cd $W && OCTOCODE_HOME=$H octocode scheme >/dev/null; echo "exit $?"); (cd $W && OCTOCODE_HOME=$H octocode config --json 2>/dev/null | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log("diagnostics:",JSON.stringify(JSON.parse(s).diagnostics)))')
echo "== T8 misconfig: invalid JSON ws rc"; echo '{ broken' > $W/.octocode/.octocoderc; (cd $W && OCTOCODE_HOME=$H octocode scheme >/dev/null; echo "exit $?"); rm $W/.octocode/.octocoderc
echo "== T9 invalid env value REQUEST_TIMEOUT=abc in global .env"; echo REQUEST_TIMEOUT=abc > $H/.env; (cd $W && OCTOCODE_HOME=$H octocode scheme >/dev/null; echo "exit $?"); rm $H/.env
echo "== T10 protected key OCTOCODE_HOME/PATH in global .env"; printf 'PATH=/evil\nNODE_OPTIONS=--inspect\n' > $H/.env; (cd $W && OCTOCODE_HOME=$H octocode config --json 2>&1 | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log("skippedProtected:",JSON.stringify(j.skippedProtected),"diag:",JSON.stringify(j.diagnostics))})'); rm $H/.env
echo "== T11 GITHUB_API_URL in ws .env (dotenv:home)"; echo GITHUB_API_URL=https://evil.example/api > $W/.octocode/.env; (cd $W && OCTOCODE_HOME=$H octocode config --json 2>&1 | head -c 800); echo; rm $W/.octocode/.env
