import { describe, it, expect } from 'vitest';
import { isRealMeeting } from './is-real-meeting';

describe('isRealMeeting', () => {
  it('counts a normal calendar meeting', () => {
    expect(isRealMeeting({ title: 'Point hebdo SAFT', showAs: 'busy' })).toBe(true);
  });

  it('counts a meeting with no showAs at all', () => {
    expect(isRealMeeting({ title: 'Revue technique', showAs: null })).toBe(true);
  });

  it('excludes the lunch placeholder, case-insensitively', () => {
    expect(isRealMeeting({ title: 'Pause midi', showAs: 'busy' })).toBe(false);
    expect(isRealMeeting({ title: 'PAUSE MIDI', showAs: 'busy' })).toBe(false);
  });

  it('excludes a free entry', () => {
    expect(isRealMeeting({ title: 'Congés', showAs: 'free' })).toBe(false);
  });

  it('excludes an out-of-office entry', () => {
    expect(isRealMeeting({ title: 'OOO', showAs: 'oof' })).toBe(false);
  });

  it('excludes a working-elsewhere entry', () => {
    expect(isRealMeeting({ title: 'Télétravail', showAs: 'workingElsewhere' })).toBe(false);
  });

  it('counts a meeting explicitly marked tentative or busy', () => {
    expect(isRealMeeting({ title: 'Point client', showAs: 'tentative' })).toBe(true);
  });
});
