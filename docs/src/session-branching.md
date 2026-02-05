# Session Branching

Session branching lets the agent fork a conversation at any point, creating an
independent copy that can diverge without affecting the original.

## Overview

The `branch_session` tool creates a new session by copying messages from the
current session up to a specified index. The new session gets its own key and
can evolve independently — useful for exploring alternative approaches or
running "what if" scenarios.

## Agent Tool

### Fork a session

```json
{
  "at_message": 5,
  "label": "explore-alternative"
}
```

- **`at_message`** — the message index to fork at (messages 0..N are copied).
  If omitted, all messages are copied.
- **`label`** — optional label for the new session.

The tool returns the new session key, which can be used to switch to the
branched session.

## Storage

Branch relationships are tracked in the `session_branches` table
(`crates/sessions/migrations/20260205130000_session_branches.sql`), recording
the parent session, child session, and the fork point.

```admonish info title="Note"
The branched session is fully independent after creation. Changes to the parent
do not propagate to the branch, and vice versa.
```
