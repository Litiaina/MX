import { describe, expect, it } from 'vitest';
import { officeLoadingSteps, officeEngineProgress, loadingBytes, loadingPercent, openingDetail } from './loading';

describe('honest Office loading feedback', () => {
  it('keeps preparation, file transfer, resources, runtime and rendering separate', () => {
    expect(officeLoadingSteps.map(step => step.phase)).toEqual(['access', 'document', 'engine', 'runtime', 'opening']);
  });
  it('reads actual engine data-package progress', () => {
    expect(officeEngineProgress('Downloading data... (2048/4096)')).toMatchObject({ phase: 'engine', loaded: 2048, total: 4096 });
  });
  it('does not invent a percentage for unmeasured initialization', () => {
    expect(officeEngineProgress('Downloading data...')).toMatchObject({ phase: 'engine' });
    expect(officeEngineProgress('Running...')).toMatchObject({ phase: 'runtime' });
    expect(loadingPercent(undefined, undefined)).toBeNull();
    expect(officeEngineProgress('')).toBeNull();
    expect(officeEngineProgress('Some vendor diagnostic')).toBeNull();
  });
  it('rejects inconsistent or unsafe engine totals', () => {
    expect(officeEngineProgress('Downloading data... (8192/4096)')?.total).toBeNull();
    expect(officeEngineProgress('Downloading data... (4/0)')?.total).toBeNull();
    expect(officeEngineProgress('Downloading data... (4/9007199254740992)')?.total).toBeNull();
    expect(officeEngineProgress('Downloading data... (9007199254740992/9007199254740992)')?.loaded).toBeUndefined();
  });
  it('shows only a measured per-download percentage', () => {
    expect(loadingPercent(0, 100)).toBe(0);
    expect(loadingPercent(25, 100)).toBe(25);
    expect(loadingPercent(100, 100)).toBe(100);
    for (const [loaded, total] of [[4, null], [4, 0], [101, 100], [-1, 100], [Infinity, 100], [1, NaN]]) {
      expect(loadingPercent(loaded as number, total)).toBeNull();
    }
  });
  it('uses explicitly labeled binary byte units', () => {
    expect(loadingBytes(512)).toBe('512 B');
    expect(loadingBytes(1024)).toBe('1.0 KiB');
    expect(loadingBytes(1536 * 1024)).toBe('1.5 MiB');
    expect(loadingBytes(1024 ** 3)).toBe('1.0 GiB');
    expect(loadingBytes(Infinity)).toBe('0 B');
  });
  it('describes the actual document type instead of generic startup text', () => {
    expect(openingDetail('.xlsx')).toContain('worksheets, formulas');
    expect(openingDetail('.ods')).toContain('spreadsheet');
    expect(openingDetail('.pptx')).toContain('slides');
    expect(openingDetail('.docx')).toContain('pages');
  });
});
