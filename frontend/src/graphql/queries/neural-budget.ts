// The rolling window is five hours because that is the window the subscription
// itself is measured over; ten days of sparkline is what fits the panel's width.
// Both are server defaults too — passed explicitly so the panel's shape is decided
// in one place, here, rather than split between the client and the resolver.
export const NEURAL_BUDGET_QUERY = `
  query NeuralBudget($windowHours: Int!, $sparklineDays: Int!) {
    neuralBudget(windowHours: $windowHours, sparklineDays: $sparklineDays) {
      windowHours
      consumedTokens
      cacheReadTokens
      declaredCeiling
      consumedRatio
      perDay
      perModel { model tokens }
      topProject { name tokens ratio }
    }
  }
`;
