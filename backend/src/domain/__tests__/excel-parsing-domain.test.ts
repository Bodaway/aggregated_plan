import {
  isPhaseHeader,
  extractPhaseInfo,
  parseFrenchDate,
  parseExcelRow,
  groupRowsByProject,
  extractProjectDates,
} from '@domain/excel-parsing-domain';

describe('excel-parsing-domain', () => {
  describe('isPhaseHeader', () => {
    it('detects a numbered phase header', () => {
      expect(isPhaseHeader('1. Conception, Epic : SCB-267')).toBe(true);
    });

    it('detects phase 2 header', () => {
      expect(isPhaseHeader('2. Développement, Epic : SCB-300')).toBe(true);
    });

    it('detects phase with only number and dot', () => {
      expect(isPhaseHeader('3. Recette')).toBe(true);
    });

    it('rejects regular task name', () => {
      expect(isPhaseHeader('Dev Interface UI')).toBe(false);
    });

    it('rejects empty string', () => {
      expect(isPhaseHeader('')).toBe(false);
    });

    it('rejects string starting with number but no dot-space', () => {
      expect(isPhaseHeader('10 tasks remaining')).toBe(false);
    });
  });

  describe('extractPhaseInfo', () => {
    it('extracts name and epic ref from phase header', () => {
      const info = extractPhaseInfo('1. Conception, Epic : SCB-267');
      expect(info.name).toBe('Conception');
      expect(info.epicRef).toBe('SCB-267');
    });

    it('extracts name without epic ref', () => {
      const info = extractPhaseInfo('3. Recette');
      expect(info.name).toBe('Recette');
      expect(info.epicRef).toBeUndefined();
    });

    it('handles phase with extra whitespace', () => {
      const info = extractPhaseInfo('2.  Développement , Epic : SCB-300 ');
      expect(info.name).toBe('Développement');
      expect(info.epicRef).toBe('SCB-300');
    });
  });

  describe('parseFrenchDate', () => {
    it('parses "16-janv" correctly', () => {
      const result = parseFrenchDate('16-janv', 2026);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2026-01-16');
      }
    });

    it('parses "20-févr" correctly', () => {
      const result = parseFrenchDate('20-févr', 2026);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2026-02-20');
      }
    });

    it('parses "03-déc" correctly', () => {
      const result = parseFrenchDate('03-déc', 2025);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2025-12-03');
      }
    });

    it('parses "1-mars" with single digit day', () => {
      const result = parseFrenchDate('1-mars', 2026);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2026-03-01');
      }
    });

    it('parses "15-août" correctly', () => {
      const result = parseFrenchDate('15-août', 2026);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2026-08-15');
      }
    });

    it('parses all French month abbreviations', () => {
      const months: readonly [string, string][] = [
        ['01-janv', '2026-01-01'],
        ['01-févr', '2026-02-01'],
        ['01-mars', '2026-03-01'],
        ['01-avr', '2026-04-01'],
        ['01-mai', '2026-05-01'],
        ['01-juin', '2026-06-01'],
        ['01-juil', '2026-07-01'],
        ['01-août', '2026-08-01'],
        ['01-sept', '2026-09-01'],
        ['01-oct', '2026-10-01'],
        ['01-nov', '2026-11-01'],
        ['01-déc', '2026-12-01'],
      ];

      months.forEach(([input, expected]) => {
        const result = parseFrenchDate(input, 2026);
        expect(result.ok).toBe(true);
        if (result.ok) {
          expect(result.value).toBe(expected);
        }
      });
    });

    it('returns error for invalid month', () => {
      const result = parseFrenchDate('16-xyz', 2026);
      expect(result.ok).toBe(false);
    });

    it('returns error for invalid format', () => {
      const result = parseFrenchDate('not-a-date', 2026);
      expect(result.ok).toBe(false);
    });

    it('parses ISO date format passthrough', () => {
      const result = parseFrenchDate('2026-01-16', 2026);
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value).toBe('2026-01-16');
      }
    });
  });

  describe('parseExcelRow', () => {
    it('parses a valid row', () => {
      const result = parseExcelRow(
        {
          topic: 'eActions - non conformités',
          task: 'Dev Interface UI',
          startDate: '16-janv',
          endDate: '20-févr',
          comment: 'Some comment',
        },
        2026,
      );

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.topic).toBe('eActions - non conformités');
        expect(result.value.task).toBe('Dev Interface UI');
        expect(result.value.startDate).toBe('2026-01-16');
        expect(result.value.endDate).toBe('2026-02-20');
        expect(result.value.comment).toBe('Some comment');
        expect(result.value.isPhaseHeader).toBe(false);
      }
    });

    it('detects phase header row', () => {
      const result = parseExcelRow(
        {
          topic: 'eActions - non conformités',
          task: '1. Conception, Epic : SCB-267',
          startDate: '16-janv',
          endDate: '20-janv',
          comment: undefined,
        },
        2026,
      );

      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.isPhaseHeader).toBe(true);
      }
    });

    it('returns error for missing topic', () => {
      const result = parseExcelRow(
        {
          topic: undefined,
          task: 'Some task',
          startDate: '16-janv',
          endDate: '20-janv',
          comment: undefined,
        },
        2026,
      );

      expect(result.ok).toBe(false);
    });

    it('returns error for missing task', () => {
      const result = parseExcelRow(
        {
          topic: 'Project',
          task: undefined,
          startDate: '16-janv',
          endDate: '20-janv',
          comment: undefined,
        },
        2026,
      );

      expect(result.ok).toBe(false);
    });

    it('returns error for missing dates', () => {
      const result = parseExcelRow(
        {
          topic: 'Project',
          task: 'Task',
          startDate: undefined,
          endDate: undefined,
          comment: undefined,
        },
        2026,
      );

      expect(result.ok).toBe(false);
    });
  });

  describe('groupRowsByProject', () => {
    it('groups rows by topic name', () => {
      const rows = [
        {
          topic: 'Project A',
          task: 'Task 1',
          startDate: '2026-01-01' as const,
          endDate: '2026-01-10' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
        {
          topic: 'Project B',
          task: 'Task 2',
          startDate: '2026-02-01' as const,
          endDate: '2026-02-10' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
        {
          topic: 'Project A',
          task: 'Task 3',
          startDate: '2026-01-15' as const,
          endDate: '2026-01-20' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
      ];

      const grouped = groupRowsByProject(rows);
      expect(grouped.size).toBe(2);
      expect(grouped.get('Project A')?.length).toBe(2);
      expect(grouped.get('Project B')?.length).toBe(1);
    });

    it('returns empty map for empty input', () => {
      const grouped = groupRowsByProject([]);
      expect(grouped.size).toBe(0);
    });
  });

  describe('extractProjectDates', () => {
    it('extracts min start and max end', () => {
      const rows = [
        {
          topic: 'Project A',
          task: 'Task 1',
          startDate: '2026-01-15' as const,
          endDate: '2026-02-10' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
        {
          topic: 'Project A',
          task: 'Task 2',
          startDate: '2026-01-01' as const,
          endDate: '2026-03-20' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
        {
          topic: 'Project A',
          task: 'Task 3',
          startDate: '2026-02-01' as const,
          endDate: '2026-02-28' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
      ];

      const dates = extractProjectDates(rows);
      expect(dates.startDate).toBe('2026-01-01');
      expect(dates.endDate).toBe('2026-03-20');
    });

    it('handles single row', () => {
      const rows = [
        {
          topic: 'Project A',
          task: 'Task 1',
          startDate: '2026-05-01' as const,
          endDate: '2026-05-31' as const,
          comment: undefined,
          isPhaseHeader: false,
        },
      ];

      const dates = extractProjectDates(rows);
      expect(dates.startDate).toBe('2026-05-01');
      expect(dates.endDate).toBe('2026-05-31');
    });
  });
});
