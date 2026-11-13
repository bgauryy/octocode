# Target reload and duplicate scrape loops

## Goal

Investigate the following scenario: A configuration reload changes labels and removes a target while its scrape is in flight.

## Questions

- What happens when a reload removes or changes a target during an active scrape?
- When are scrape loops reused, replaced, or cancelled?
- Can old and new loops produce overlapping results, and what prevents inconsistent storage state?
