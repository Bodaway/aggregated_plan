// Null outside a break, which is the ordinary case: the HUD shows its grid.
// `label` and `body` are the rule's own words, so rewording a rule changes the
// next overlay without the front end knowing anything about rules. The way out
// of a break is `END_BREAK_MUTATION`, in mutations/break-session.ts.
export const ACTIVE_BREAK_QUERY = `
  query ActiveBreak {
    activeBreak {
      eventId kind label body startedAt endsAt
    }
  }
`;
