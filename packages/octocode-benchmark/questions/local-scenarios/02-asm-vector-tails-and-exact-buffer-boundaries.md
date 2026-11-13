# Vector tails and exact buffer boundaries

## Goal

Investigate the following scenario: An image width is not a multiple of the vector width.

## Questions

- How does SIMD color conversion handle widths smaller than or not divisible by its vector width?
- What padding and alignment guarantees do its memory accesses require?
- Which component owns those guarantees, and what happens if they are violated?
