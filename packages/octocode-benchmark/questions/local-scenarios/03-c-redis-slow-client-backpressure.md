# Redis slow-client backpressure

## Goal

Investigate the following scenario: A client sends commands quickly but reads replies slowly.

## Questions

- How does Redis handle a client that sends commands faster than it reads replies?
- What bounds reply-buffer growth, and how do the rules differ for replicas?
- When does backpressure become a memory problem or event-loop fairness problem?
