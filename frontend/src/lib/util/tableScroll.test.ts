import { describe, expect, it } from 'vitest';
import { clampTableScroll, tableScrollPosition } from './tableScroll';

describe('reachable record-table scrolling', () => {
  const viewport = { left: 0, right: 1200, top: 0, bottom: 800 };
  const tall = { left: 24, right: 1176, top: 200, bottom: 2200 };
  it('aligns a slim secondary scrollbar with a wide, tall table', () => {
    expect(tableScrollPosition(tall, viewport, 2600, 1150)).toEqual({ left: 25, top: 786, width: 1150, height: 14, contentWidth: 2600 });
  });
  it('does not add UI to fitting tables or duplicate a reachable native scrollbar', () => {
    expect(tableScrollPosition(tall, viewport, 1150, 1150)).toBeNull();
    expect(tableScrollPosition({ ...tall, bottom: 790 }, viewport, 2600, 1150)).toBeNull();
  });
  it('disappears before the table enters view and after it leaves view', () => {
    expect(tableScrollPosition({ ...tall, top: 810 }, viewport, 2600, 1150)).toBeNull();
    expect(tableScrollPosition({ ...tall, top: -2000, bottom: -100 }, viewport, 2600, 1150)).toBeNull();
  });
  it('respects nested clipping and shifted visual viewports without changing the scroll range', () => {
    expect(tableScrollPosition(tall, { left: 100, right: 900, top: 300, bottom: 600 }, 2600, 1150)).toEqual({ left: 100, top: 586, width: 800, height: 14, contentWidth: 2250 });
  });
  it('clamps scroll positions after columns are hidden or the viewport widens', () => {
    expect(clampTableScroll(4000, 2600, 1150)).toBe(1450);
    expect(clampTableScroll(-10, 2600, 1150)).toBe(0);
    expect(clampTableScroll(120, 900, 1000)).toBe(0);
  });
});
