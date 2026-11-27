# Reference preservation and object identity

## Goal

Investigate the following scenario: A serialized object graph contains repeated references and a cycle.

## Questions

- How does Newtonsoft.Json preserve repeated references and cycles?
- How does object construction affect when identity becomes available?
- What happens with forward references, unresolved references, and constructor-created objects?
