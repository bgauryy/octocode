Load when writing the control loop of an agent.

# Control loop

## Reference control loop

A framework-independent sketch of how the parts fit together, not an SDK:

```python
def run_agent(request, services, limits):
    task = define_task(request, success_criteria=True)
    state = load_checkpoint(task.id)
    budget = Budget(limits)
    while budget.can_continue():
        # Build only the context the next decision needs.
        context = select_context(task=task, state=state,
                                 token_budget=budget.context_allowance())
        model = route_model(task, context)          # by complexity and risk
        decision = model.decide(task=task, context=context,
                                available_tools=discover_relevant_tools(task))
        if decision.kind == "finish":
            verdict = verify_completion(task, state, decision)
            if verdict.passed:
                return build_result(verdict=verdict, state=state)
            state = record_failed_verification(state, verdict)  # evidence, not completion
            continue
        result = execute(decision, services)        # validated, authorized, idempotent
        state = update_state(state, decision, result)
        save_checkpoint(task.id, state)
        if not making_progress(state):
            return build_partial_result(state, reason="no progress")
    return build_partial_result(state, reason="budget exhausted")
```

A production version also needs durable action IDs, protection against repeating side effects after a crash, input and output validation, cancellation, tracing and approval policies.
