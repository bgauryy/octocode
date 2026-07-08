Mode: Map

## TL;DR
Reflection/self-harness patterns for AI agents already have prior art (Reflexion, Self-Refine), but a compact receipt/eval loop is the narrower, less-crowded wedge. Research limits: none.

## Surface Plan
Local skipped: external prior-art map. Web/top resources active first. GitHub/packages/code active after resource leads.

## Top Resources First
Resource-first loop: started from top articles/resources and formal sources before code search.
- Official docs article: `strong` https://example.com/official-agent-docs
- Paper: `moderate` https://example.com/reflection-paper

## Worker Loop
Web Search Scout ran Serper/Tavily for breadth and returned formal source leads.
Source/Code Checker fetched those sources, expanded the source list, and checked GitHub/packages/code for implementation evidence.

## Repo/Code Follow-up
The resources named Reflexion and Self-Refine, so GitHub/packages follow-up checked repos/packages and exact code reads for implementation evidence.

## Loop Back
Looped back to resources to reconcile contradictions: one repo was stale, so the claim was marked weak instead of treated as proof.

## Landscape
Prior art exists, but the strongest wedge is a compact receipt/eval loop rather than a full agent framework.

Decision: Narrow

## Recommended Next Step
Write a focused protocol for resource-first research and delegated evidence receipts.
