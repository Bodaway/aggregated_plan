export const BREAK_RULES_QUERY = `
  query BreakRules {
    breakRules {
      id kind label body cadence intervalMinutes atTime
      durationSeconds priority enabled urgency
    }
  }
`;

export const BREAK_STATS_QUERY = `
  query BreakStats($from: String!, $to: String!) {
    breakStats(from: $from, to: $to) {
      perRule { ruleId label taken snoozed skipped ignored absorbed expired adherence }
    }
  }
`;

// Nullable: no enabled rule has an upcoming due instant today (an all-daily
// routine, or the working windows are exhausted). Null is a normal outcome,
// not an error — see useNextBreakDue().
export const NEXT_BREAK_DUE_QUERY = `
  query NextBreakDue {
    nextBreakDue
  }
`;
