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
