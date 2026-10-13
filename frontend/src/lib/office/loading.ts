export const officeLoadingSteps = [
  { phase: 'access', title: 'Prepare document' },
  { phase: 'document', title: 'Load document' },
  { phase: 'engine', title: 'Load editor resources' },
  { phase: 'runtime', title: 'Start editor on this device' },
  { phase: 'opening', title: 'Open and render document' }
] as const;
export type OfficeLoadingPhase = typeof officeLoadingSteps[number]['phase'];
export interface OfficeLoadingProgress {
  phase: OfficeLoadingPhase;
  detail: string;
  loaded?: number;
  total?: number | null;
}

/** Interpret only real Emscripten signals; never invent overall boot progress. */
export function officeEngineProgress(status: string): OfficeLoadingProgress | null {
  const download = /^Downloading data\.\.\.\s*\((\d+)\/(\d+)\)$/.exec(status);
  if (download) {
    const loaded = Number(download[1]), total = Number(download[2]);
    return { phase: 'engine', detail: 'Receiving bundled editor resources from MX.',
      loaded: Number.isSafeInteger(loaded) ? loaded : undefined,
      total: Number.isSafeInteger(total) && total > 0 && total >= loaded ? total : null };
  }
  if (status.startsWith('Downloading data')) return { phase: 'engine', detail: 'Loading bundled editor resources from MX or this browser’s cache.' };
  if (status === 'Running...') return { phase: 'runtime', detail: 'Initializing the editor, graphics and document workers on your device.' };
  return null;
}

export function loadingBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '0 B';
  if (value < 1024) return `${Math.round(value)} B`;
  const unit = Math.min(3, Math.floor(Math.log(value) / Math.log(1024)));
  return `${(value / 1024 ** unit).toFixed(1)} ${['B', 'KiB', 'MiB', 'GiB'][unit]}`;
}
export function loadingPercent(loaded?: number, total?: number | null): number | null {
  return typeof loaded === 'number' && Number.isFinite(loaded) && loaded >= 0 &&
    typeof total === 'number' && Number.isFinite(total) && total > 0 && loaded <= total
    ? Math.floor(loaded / total * 100) : null;
}
export function openingDetail(extension: string): string {
  if (/^\.(xlsx?|xlsm|ods)$/.test(extension)) return 'Reading worksheets, formulas and formatting, then preparing the spreadsheet view.';
  if (/^\.(pptx?|pptm|odp)$/.test(extension)) return 'Reading slides, images and formatting, then preparing the presentation view.';
  return 'Reading the document, laying out pages and formatting, then preparing the editing view.';
}
