"""One bounded host Stop check, without auto-completion or delivery ownership."""
import json
from pathlib import Path
from .database import query, read_transaction


class CompletionMixin:
    def completion_check(self, session, input):
        from .store import now
        if input.get('hook_event_name') != 'Stop' or input.get('stop_hook_active') is True:
            return {}
        with read_transaction(self.db):
            identity = self.known(session)
            if identity.get('vendorSession') != input.get('session_id') or Path(input.get('cwd', '')).resolve(strict=True) != Path(self.workspace):
                raise ValueError('Completion check requires the bound native session and workspace')
            attachment = self.attachment(session)
            if attachment.get('transport') != 'claude' and not (identity['vendor'] == 'pi' and attachment.get('transport') == 'raw'):
                raise ValueError('Completion check requires a Claude native binding or Pi raw binding')
            pending = query(self.db, "SELECT DISTINCT m.id FROM deliveries d JOIN messages m ON m.id=d.message JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND x.state='submitted' ORDER BY m.id LIMIT 17", (session, now()))
        if not pending:
            return {}
        ids = [row['id'] for row in pending[:16]]
        reason = 'Peer work is still unacknowledged: IDs {}{}. Handle these IDs from existing context; only if a body is missing, recover it with inbox(message:ID). Finish requests with complete {{message:ID,reply:answer}}; handled FYIs/answers with complete {{messages:[IDs]}} without a reply. Leave unfinished work pending. If genuinely blocked, explain why and stop; this check will not block the recovery turn again.'.format(json.dumps(ids, separators=(',', ':')), ' (more pending; use inbox recovery as needed)' if len(pending)>16 else '')
        return dict(decision='block', pending=ids, reason=reason)
