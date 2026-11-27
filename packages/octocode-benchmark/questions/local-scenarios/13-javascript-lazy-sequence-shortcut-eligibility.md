# Lazy sequence shortcut eligibility

## Goal

Investigate the following scenario: Two equivalent-looking Lodash chains invoke a mapping callback different numbers of times.

## Questions

- Why can equivalent-looking Lodash chains call a mapping function different numbers of times?
- What determines whether lazy shortcut fusion applies?
- How do operation order, callback arity, and side effects affect correctness and work performed?
