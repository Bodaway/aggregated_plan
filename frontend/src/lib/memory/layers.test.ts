import { describe, it, expect } from 'vitest';
import {
  CAPTURE_CHIP_Z,
  MEMORY_BACKDROP_Z,
  MEMORY_SHEET_Z,
  TASK_SHEET_Z,
} from './layers';

describe('memory stacking layers', () => {
  it('puts the capture chip above the task sheets', () => {
    expect(CAPTURE_CHIP_Z).toBeGreaterThan(TASK_SHEET_Z);
  });

  it('puts the memory sheet above the task sheets', () => {
    // Equal values are NOT enough: the task edit sheet is rendered after the
    // page content in SearchProvider, so DOM order would hand it the win.
    expect(MEMORY_BACKDROP_Z).toBeGreaterThan(TASK_SHEET_Z);
    expect(MEMORY_SHEET_Z).toBeGreaterThan(MEMORY_BACKDROP_Z);
  });
});
