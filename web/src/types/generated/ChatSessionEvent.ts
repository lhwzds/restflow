export type ChatSessionEvent =
  | { type: 'Created'; session_id: string }
  | { type: 'Updated'; session_id: string }
  | { type: 'MessageAdded'; session_id: string; source: string }
  | { type: 'Deleted'; session_id: string }
