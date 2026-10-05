import { describe, expect, it } from 'vitest';
import { shouldStageVideoHandoff } from './videoPresentation';

describe('call video presentation', () => {
  const receiverTrack = { kind: 'video' } as MediaStreamTrack;
  const replacementTrack = { kind: 'video' } as MediaStreamTrack;

  it('keeps the decoder bound across heartbeats and microphone-only stream wrapper changes', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', receiverTrack, 'camera')).toBe(false);
    expect(shouldStageVideoHandoff(receiverTrack, 'screen', receiverTrack, 'screen')).toBe(false);
  });

  it('stages real track replacements and camera-to-screen semantic transitions', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', replacementTrack, 'camera')).toBe(true);
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', receiverTrack, 'screen')).toBe(true);
    expect(shouldStageVideoHandoff(receiverTrack, 'screen', receiverTrack, 'camera')).toBe(true);
  });

  it('does not stage an absent or disabled video source', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', null, 'off')).toBe(false);
  });
});
