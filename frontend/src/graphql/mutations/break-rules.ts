export const CREATE_BREAK_RULE = `
  mutation CreateBreakRule($input: BreakRuleInput!) {
    createBreakRule(input: $input) { id }
  }
`;

export const UPDATE_BREAK_RULE = `
  mutation UpdateBreakRule($id: ID!, $input: BreakRuleInput!) {
    updateBreakRule(id: $id, input: $input) { id }
  }
`;

export const DELETE_BREAK_RULE = `
  mutation DeleteBreakRule($id: ID!) {
    deleteBreakRule(id: $id)
  }
`;
