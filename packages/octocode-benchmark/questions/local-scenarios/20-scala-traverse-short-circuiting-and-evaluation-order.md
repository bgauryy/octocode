# Traverse short-circuiting and evaluation order

## Goal

Investigate the following scenario: A collection traversal with an effect appears to evaluate elements after a failure.

## Questions

- When does Cats traversal stop evaluating after a short-circuiting effect fails?
- How do effect construction, effect execution, and collection order differ?
- What determines stack safety and whether later elements are evaluated?
