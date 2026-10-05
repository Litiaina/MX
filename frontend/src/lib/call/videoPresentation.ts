export type CallVideoMode = 'off' | 'camera' | 'screen';

export function shouldStageVideoHandoff(
  currentTrack: MediaStreamTrack | null,
  currentMode: CallVideoMode,
  nextTrack: MediaStreamTrack | null,
  nextMode: CallVideoMode
): boolean {
  return nextTrack !== null
    && nextMode !== 'off'
    && (currentTrack !== nextTrack || currentMode !== nextMode);
}
