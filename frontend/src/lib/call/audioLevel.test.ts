import { describe, expect, it } from 'vitest';
import { audioLevelFromTimeDomain, speakingFromLevel } from './audioLevel';

describe('call audio levels', () => {
  it('normalizes silence and audible samples', () => {
    expect(audioLevelFromTimeDomain(new Uint8Array(512).fill(128))).toBe(0);
    expect(audioLevelFromTimeDomain(Uint8Array.from({ length: 512 }, (_, index) => index % 2 ? 154 : 102))).toBeGreaterThan(0.5);
  });

  it('uses hysteresis so speaking rings do not flicker at the threshold', () => {
    expect(speakingFromLevel(0.074, false)).toBe(false);
    expect(speakingFromLevel(0.08, false)).toBe(true);
    expect(speakingFromLevel(0.04, true)).toBe(true);
    expect(speakingFromLevel(0.03, true)).toBe(false);
  });
});
