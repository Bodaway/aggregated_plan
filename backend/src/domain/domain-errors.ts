export type DomainErrorCode =
  | 'invalid-date-range'
  | 'invalid-name'
  | 'invalid-capacity'
  | 'invalid-allocation'
  | 'allocation-exceeds-week-capacity'
  | 'assignment-conflict'
  | 'availability-conflict'
  | 'capacity-exceeded'
  | 'duplicate-name'
  | 'not-found';

export type DomainError = {
  readonly code: DomainErrorCode;
  readonly message: string;
  readonly details?: Record<string, unknown>;
};

export const createDomainError = (
  code: DomainErrorCode,
  message: string,
  details?: Record<string, unknown>,
): DomainError => ({
  code,
  message,
  details,
});
