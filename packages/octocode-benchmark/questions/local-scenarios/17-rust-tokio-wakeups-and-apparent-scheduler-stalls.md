# Tokio wakeups and apparent scheduler stalls

## Goal

Investigate the following scenario: A task is woken repeatedly while another task runs a long CPU loop.

## Questions

- Why can a Tokio task appear stalled despite repeated wakeups?
- How do scheduling queues, stealing, parking, and cooperative execution affect progress?
- What distinguishes a lost wakeup from a future that fails to yield?
