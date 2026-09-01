# Output
Load before the parent answers or decides the next spawn. Why: fan-out without a decision card hides solo-vs-spawn and false merges.

## Solo vs spawn card
```text
Decision: SOLO | BATCH | SPAWN | HANDOFF
Why: <speed | expertise | isolation | context>
Workers: <n> · Topology: <pattern>
Packets sealed?: yes/no · Ownership declared?: yes/no
Authority/budget bounded?: yes/no
Technique (optional): rubber-duck | interview | mimic-flow | verifier | red-team | blind-review | consensus | local-ollama
```

## Synthesis card
```text
Barrier: all needed workers idle/terminal
Conflicts: <list or none>
Claims re-checked in parent: <anchors>
Verdict: answer | replan | interview | duck | stop
Gaps: <…>
Next: <one action>
```

## Next-step gate
1. Answer user  
2. Rubber-duck the plan  
3. Interview / red-team / steelman claims  
4. Blind-review the artifact  
5. Mimic-flow respawn with playbook  
6. Consensus (N independent) or verifier-critic  
7. Local Ollama offload (`references/local-ollama.md`)  
8. Replan / stop leftovers  
9. Measure with `octocode-eval-benchmark`

Next: techniques → `references/techniques.md`; merge rules → `references/synthesize.md`.
