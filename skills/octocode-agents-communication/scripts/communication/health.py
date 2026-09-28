"""Bounded delivery diagnostics; observations never replay or acknowledge mail."""
from .database import query, read_transaction


class HealthMixin:
    def health(self, input):
        from .store import now
        from .dispatch import delivery_owner_live
        at = now()
        threshold, limit = int(input.get('staleAfterMs', 300000)), int(input.get('limit', 25))
        orphaned = at - min(threshold, 30000)
        self.db.create_function('delivery_owner_live', 1, lambda session: delivery_owner_live(self.database, session))
        pending = """WITH pending AS (
SELECT d.message,d.recipient,s.name,s.vendor,m.wake,m.expiresAt,
coalesce(x.state,'queued') AS state,x.attemptedAt,x.submittedAt,
CASE WHEN x.state='uncertain' THEN 'uncertain'
WHEN m.wake='action' AND m.expiresAt<=?2 THEN 'expiredAction'
WHEN m.wake='action' AND s.expiresAt<=?2 THEN 'offlineRecipient'
WHEN x.state='staged' AND (x.attemptedAt<=?3 OR (x.attemptedAt<={orphaned} AND NOT delivery_owner_live(x.recipient))) THEN 'stalledOffer'
WHEN x.state='submitted' AND m.wake='action' AND x.submittedAt<=?3 THEN 'overdueHandling'
ELSE NULL END AS issue FROM sessions s JOIN deliveries d ON d.recipient=s.id AND d.acknowledgedAt IS NULL
JOIN messages m ON m.id=d.message LEFT JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE s.workspace=?1) """.format(orphaned=orphaned)
        params = [self.workspace, at, at-threshold]
        with read_transaction(self.db):
            counts = query(self.db, pending + """SELECT count(*) AS unacknowledged,
coalesce(sum(issue IS NOT NULL),0) AS attention,
coalesce(sum(state IN ('queued','ready') AND wake='action' AND expiresAt>?2),0) AS queuedAction,
coalesce(sum(state IN ('queued','ready') AND wake='passive' AND expiresAt>?2),0) AS waitingPassive,
coalesce(sum(state='staged'),0) AS staged, coalesce(sum(state='submitted'),0) AS submittedUnacknowledged,
coalesce(sum(state='uncertain'),0) AS uncertain, coalesce(sum(expiresAt<=?2),0) AS expiredUnacknowledged FROM pending""", params)[0]
            cursor = input.get('after', {})
            issues = query(self.db, pending + 'SELECT message,recipient,name,vendor,state,issue,attemptedAt,submittedAt FROM pending WHERE issue IS NOT NULL AND (message>?4 OR (message=?4 AND recipient>?5)) ORDER BY message,recipient LIMIT ?6', params + [int(cursor.get('message', 0)), cursor.get('recipient', ''), limit+1])
            more, issues = len(issues) > limit, issues[:limit]
        instructions = {
            'uncertain': 'Inspect the recipient and dispatch token before any explicit retry; context may have arrived.',
            'expiredAction': 'Inspect whether work is still needed; acknowledge a deliberate no-action decision or send a new request with a new key.',
            'offlineRecipient': 'Check the recipient and its delivery owner; resume deliberately. Do not revive its expired leases.',
            'stalledOffer': 'Inspect the in-flight owner and recipient before recovery; elapsed time is not permission to replay.',
            'overdueHandling': "Check the recipient's progress. Submission is not handling; do not automatically complete or resend."}
        status = 'attention' if counts['attention'] else 'pending' if any(counts[k] for k in ('queuedAction', 'staged', 'submittedUnacknowledged')) else 'clear'
        result = dict(status=status, workspace=self.workspace, observedAt=at, staleAfterMs=threshold, counts=counts, issues=issues, actions={r['issue']: instructions[r['issue']] for r in issues})
        if more:
            result['next'] = {'command': 'health', 'input': dict(staleAfterMs=threshold, limit=limit, after={k: issues[-1][k] for k in ('message', 'recipient')})}
        return result
