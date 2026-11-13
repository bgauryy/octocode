# JSON depth and allocation amplification

## Goal

Investigate the following scenario: An untrusted JSON payload has modest bytes but extreme nesting or many tiny values.

## Questions

- How do JSON nesting depth, object width, and duplicate keys affect parsing and destruction costs?
- Which resource limits exist, and which inputs can amplify memory or stack use?
- What engineering trade-off would best address the dominant risk?
