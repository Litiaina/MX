export interface ScrollBounds { left: number; right: number; top: number; bottom: number }

/** Only supplement a wide table whose own bottom scrollbar is out of reach. */
export function tableScrollPosition(table: ScrollBounds, visible: ScrollBounds, scrollWidth: number, clientWidth: number) {
  const height = 14;
  const left = Math.max(table.left + 1, visible.left);
  const right = Math.min(table.right - 1, visible.right);
  if (scrollWidth <= clientWidth + 1 || right - left < 40 || table.top >= visible.bottom - 40 || table.bottom <= visible.bottom + 1) return null;
  return { left, top: visible.bottom - height, width: right - left, height, contentWidth: scrollWidth - clientWidth + right - left };
}

export function clampTableScroll(value: number, scrollWidth: number, clientWidth: number) {
  return Math.max(0, Math.min(value, Math.max(0, scrollWidth - clientWidth)));
}
