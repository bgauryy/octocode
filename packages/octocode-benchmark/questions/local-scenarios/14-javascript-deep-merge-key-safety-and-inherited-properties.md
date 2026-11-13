# Deep merge key safety and inherited properties

## Goal

Investigate the following scenario: A deeply nested merge input contains constructor, prototype, or __proto__ keys.

## Questions

- How does Lodash merge handle constructor, prototype, and __proto__ keys?
- How do nested objects, arrays, and inherited properties change the behavior?
- Which input shapes are safely rejected or retained, and can any violate object-isolation expectations?
