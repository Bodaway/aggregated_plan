import type { RawExcelRow } from '@domain/excel-parsing-domain';

export type ExcelParserAdapter = {
  readonly parseWorkbook: (buffer: Buffer) => Promise<readonly RawExcelRow[]>;
};
