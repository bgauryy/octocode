# Deployment rollout convergence under informer delay

## Goal

Investigate the following scenario: A Kubernetes Deployment changes desired replicas while pod or ReplicaSet events arrive late or duplicated.

## Questions

- How does a Kubernetes Deployment converge when informer events are delayed or duplicated during scaling?
- How do stale reads, retries, and conflicting updates affect rollout decisions?
- Which conditions guarantee eventual progress, and which can stall reconciliation?
