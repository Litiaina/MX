export function savedToMx(revision: number, at: number, newerEdits: boolean): string {
  return `Saved to MX · revision ${revision} · ${new Date(at).toLocaleTimeString()}${newerEdits ? ' · newer edits still need saving' : ''}`;
}

export function offlinePreparationSummary(detail: string): string {
  if (/SSL|certificate|cert_/i.test(detail)) return 'Offline reopening is unavailable because the browser does not trust MX’s HTTPS certificate. Editing and saving can still work while MX is reachable.';
  return 'Offline reopening is not ready. Your open document is retained; this does not mean that saving to MX failed.';
}
