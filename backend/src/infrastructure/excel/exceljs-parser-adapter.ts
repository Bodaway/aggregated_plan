import ExcelJS from 'exceljs';
import type { ExcelParserAdapter } from '@application/excel-parser-adapter';
import type { RawExcelRow } from '@domain/excel-parsing-domain';

const DATA_START_ROW = 6;

const getCellText = (cell: ExcelJS.Cell): string | undefined => {
  const value = cell.value;
  if (value === null || value === undefined) return undefined;
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  if (value instanceof Date) {
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${value.getFullYear()}-${month}-${day}`;
  }
  if (typeof value === 'object' && 'richText' in value) {
    return value.richText.map((rt) => rt.text).join('');
  }
  return String(value);
};

export const createExcelJsParserAdapter = (): ExcelParserAdapter => ({
  parseWorkbook: async (buffer: Buffer): Promise<readonly RawExcelRow[]> => {
    const workbook = new ExcelJS.Workbook();
    await workbook.xlsx.load(buffer as unknown as ExcelJS.Buffer);

    const worksheet = workbook.worksheets[0];
    if (!worksheet) {
      return [];
    }

    const rows: RawExcelRow[] = [];
    worksheet.eachRow((row, rowNumber) => {
      if (rowNumber < DATA_START_ROW) return;

      const topic = getCellText(row.getCell(1));
      const task = getCellText(row.getCell(2));
      const startDate = getCellText(row.getCell(6));
      const endDate = getCellText(row.getCell(7));
      const comment = getCellText(row.getCell(8));

      if (topic || task) {
        rows.push({ topic, task, startDate, endDate, comment });
      }
    });

    return rows;
  },
});
