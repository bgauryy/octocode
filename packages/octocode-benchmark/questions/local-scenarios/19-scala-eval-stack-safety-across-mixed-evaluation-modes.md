# Eval stack safety across mixed evaluation modes

## Goal

Investigate the following scenario: A deep flatMap chain is stack-safe, but a nearby recursive formulation overflows or repeats work.

## Questions

- Why is one deeply recursive Cats Eval computation stack-safe while a similar one overflows?
- How do evaluation modes and flatMap affect stack use and repeated work?
- When does memoization change observable behavior?
