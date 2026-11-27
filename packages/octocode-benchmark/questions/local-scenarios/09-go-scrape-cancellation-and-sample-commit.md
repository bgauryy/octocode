# Scrape cancellation and sample commit

## Goal

Investigate the following scenario: A Prometheus scrape times out after some samples have been read.

## Questions

- What happens to samples read before a Prometheus scrape times out?
- When are samples committed, rolled back, or replaced by failure and staleness information?
- How do timeout, invalid exposition, and sample-limit failures differ?
