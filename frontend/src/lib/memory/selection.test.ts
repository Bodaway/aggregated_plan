import { describe, it, expect } from 'vitest';
import { splitSelection, SELECTION_TITLE_MAX } from './selection';

describe('splitSelection', () => {
  it('uses a short selection as the title and leaves the body empty', () => {
    const split = splitSelection('Le compte Witivio est refusé par le ServiceNow de Pernod.');

    expect(split.title).toBe('Le compte Witivio est refusé par le ServiceNow de Pernod.');
    expect(split.body).toBeNull();
  });

  it('collapses newlines and repeated spaces into single spaces', () => {
    const split = splitSelection('  Le compte\n\nWitivio   est   refusé.  ');

    expect(split.title).toBe('Le compte Witivio est refusé.');
  });

  it('cuts a long selection at the last sentence end that fits the title', () => {
    const first = 'Le compte Witivio est refusé par le ServiceNow de Pernod Ricard.';
    const rest = 'Toute consultation doit donc partir du compte -ext, la politique vient de Pernod.';

    const split = splitSelection(`${first} ${rest}`);

    expect(split.title).toBe(first);
    expect(split.body).toBe(rest);
  });

  it('falls back to a word boundary with an ellipsis when no sentence ends in time', () => {
    const long =
      'un titre sans aucune ponctuation finale qui continue encore et encore bien au delà de la limite imposée au titre de la mémoire';

    const split = splitSelection(long);

    expect(split.title.length).toBeLessThanOrEqual(SELECTION_TITLE_MAX + 1);
    expect(split.title.endsWith('…')).toBe(true);
    expect(split.title).not.toContain('  ');
    expect(split.body).toBe(long);
  });

  it('reports a blank selection as empty rather than throwing', () => {
    const split = splitSelection('   \n  ');

    expect(split.title).toBe('');
    expect(split.body).toBeNull();
  });
});
