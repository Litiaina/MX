import { describe, expect, it } from 'vitest';
import { floatingPosition } from './floating';

describe('floating menu geometry', () => {
  const viewport = { left: 0, top: 0, width: 440, height: 300 };
  it('flips above a last-row action near the bottom edge', () => {
    expect(floatingPosition({ left: 380, right: 420, top: 260, bottom: 290 }, 180, 64, viewport)).toEqual({ left: 240, top: 192 });
  });
  it('opens below a first row and clamps horizontally', () => {
    expect(floatingPosition({ left: 10, right: 40, top: 10, bottom: 40 }, 180, 64, viewport)).toEqual({ left: 8, top: 44 });
  });
  it('respects the visible region when a mobile keyboard or zoom shifts it', () => {
    const result = floatingPosition({ left: 340, right: 370, top: 350, bottom: 380 }, 180, 64, { left: 100, top: 220, width: 280, height: 200 });
    expect(result).toEqual({ left: 190, top: 282 });
  });
});
