import { describe, expect, it } from 'vitest';
import { recordColumnWidths, snapshotRecordColumns, validColumnSnapshot } from './recordColumns';
const fields = [{key: 'date', field_type: 'date'}, {key: 'number', field_type: 'integer'},
  {key: 'office', field_type: 'text'}, {key: 'subject', field_type: 'long_text'}];
describe('stable record column layout', () => {
  it('allocates spare space to full long text rather than widening dates and numbers', () => {
    const widths = recordColumnWidths(fields, 1500);
    expect(widths).toEqual([52, 128, 128, 240, 818, 134]);
  });
  it('keeps wide modules horizontally scrollable instead of shrinking their columns', () => {
    const widths = recordColumnWidths(fields, 400);
    expect(widths).toEqual([52, 128, 128, 240, 400, 134]);
  });
  it('preserves a measured paged layout exactly for the same fields and viewport', () => {
    const widths = [52, 120, 100, 300, 794, 134];
    expect(recordColumnWidths(fields, 1500, snapshotRecordColumns(fields, 1500, widths))).toEqual(widths);
  });
  it('rejects stale, malformed and incompatible browser layout state', () => {
    const snapshot = snapshotRecordColumns(fields, 1500, [52, 120, 100, 300, 794, 134]);
    for (const invalid of [null, {}, {...snapshot, widths: [NaN]}, {...snapshot, widths: [52, 120, 100, 300, -1, 134]},
      {...snapshot, viewport: 500}, {...snapshot, keys: ['other']}]) {
      expect(validColumnSnapshot(invalid, fields, 1500)).toBe(false);
    }
    expect(recordColumnWidths([...fields].reverse(), 1500, snapshot)).not.toEqual(snapshot.widths);
  });
});
