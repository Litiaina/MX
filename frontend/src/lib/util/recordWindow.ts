/** Small, bounded window helpers. Neither DOM virtualization nor caching may
 * discard editor state; callers keep edits separately, keyed by record UID. */
export type WindowPiece = { kind: 'row'; index: number; key: string } | { kind: 'gap'; height: number; key: string };
/** Locate a scroll target even when a fast scrollbar drag lands in a virtual
 * spacer and the destination rows have not reached the DOM yet. */
export function windowAnchor(uids: string[], heights: Record<string, number>, top: number, estimate: number) {
  if (!uids.length) return undefined;
  let offset = 0;
  for (let index = 0; index < uids.length; index++) {
    const height = heights[uids[index]] || estimate;
    if (offset + height > top || index === uids.length - 1) return {uid: uids[index], index, offset: offset - top};
    offset += height;
  }
}
export function windowPieces(uids: string[], heights: Record<string, number>, top: number, height: number, estimate: number, pinned: string[] = [], overscan = 8): WindowPiece[] {
  const offsets = [0];
  for (const uid of uids) offsets.push(offsets.at(-1)! + (heights[uid] || estimate));
  let first = 0, last = uids.length;
  while (first < uids.length && offsets[first + 1] < top) first++;
  last = first;
  while (last < uids.length && offsets[last] < top + height) last++;
  first = Math.max(0, first - overscan); last = Math.min(uids.length, last + overscan);
  const keep = new Set(Array.from({ length: last - first }, (_, index) => first + index));
  for (const uid of pinned) { const index = uids.indexOf(uid); if (index >= 0) keep.add(index); }
  const pieces: WindowPiece[] = []; let previous = 0;
  for (const index of [...keep].sort((a, b) => a - b)) {
    if (index > previous) pieces.push({ kind: 'gap', height: offsets[index] - offsets[previous], key: `gap:${uids[previous]}` });
    pieces.push({ kind: 'row', index, key: uids[index] }); previous = index + 1;
  }
  if (previous < uids.length) pieces.push({ kind: 'gap', height: offsets[uids.length] - offsets[previous], key: `gap:${uids[previous]}` });
  return pieces;
}

export function mergeWindow<T extends { uid: string }>(current: T[], incoming: T[], before: boolean, capacity: number, pinned: string[] = []) {
  const known = new Set(current.map(row => row.uid));
  const added = incoming.filter(row => !known.has(row.uid));
  let rows = before ? [...added, ...current] : [...current, ...added];
  // Do not evict a focused/edited record. Defer that edge request instead of
  // allowing either unbounded memory or silently destroying the editor.
  const drop = Math.max(0, rows.length - capacity);
  const evicted = before ? rows.slice(rows.length - drop) : rows.slice(0, drop);
  if (drop && evicted.some(row => pinned.includes(row.uid))) return null;
  if (drop) rows = before ? rows.slice(0, capacity) : rows.slice(drop);
  return { rows, added: added.length, removed: drop };
}

export interface RecordViewPosition {
  uid: string; cursor?: string; offset: number; horizontal: number; index: number;
  search: string; sortBy: string; sortDir: string; match: 'contains' | 'prefix' | 'exact';
  filters: Record<string, string>; attachments: 'with' | 'without' | ''; page: number;
}
