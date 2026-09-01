// Answers false when the row is no longer the running one — the countdown can
// have finished in the second the button was pressed, and the tick's own write
// wins. A normal race, not an error to surface. The other half of the exchange
// is `ACTIVE_BREAK_QUERY`, in queries/break-session.ts.
export const END_BREAK_MUTATION = `
  mutation EndBreak($eventId: ID!) {
    endBreak(eventId: $eventId)
  }
`;
