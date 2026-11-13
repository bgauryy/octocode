# SIMD dispatch and unsupported instructions

## Goal

Investigate the following scenario: A JPEG conversion works on one x86 machine but faults on another.

## Questions

- How is the color-conversion implementation selected for a CPU?
- What could cause an unsupported instruction to execute despite feature detection?
- How would an instruction-selection failure differ from an ABI or buffer failure?
