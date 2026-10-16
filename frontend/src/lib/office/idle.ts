// Export occupies the native Office engine. Start only after actual interaction
// stops, and leave a longer breathing space after costly whole-document work.
export const OFFICE_IDLE_MS = 15_000;
export function officeIdleDelay(now: number, activityAt: number, preparedAt: number, exportMs: number, interacting = false): number {
  const cooldown = Math.max(30_000, Math.min(60_000, Math.max(0, exportMs) * 8));
  return Math.max(interacting ? 1000 : 0, OFFICE_IDLE_MS - (now - activityAt), cooldown - (now - preparedAt), 0);
}
