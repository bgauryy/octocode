# Atomic nesting and commit callbacks

## Goal

Investigate the following scenario: A nested atomic block fails, its exception is caught, and outer code continues querying.

## Questions

- What happens after a nested Django atomic block fails and its exception is caught?
- How do savepoints affect whether outer queries can continue?
- Which commit callbacks survive, and when can they run?
