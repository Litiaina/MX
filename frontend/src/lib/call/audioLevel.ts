export function audioLevelFromTimeDomain(samples: Uint8Array): number {
  if (!samples.length) return 0;
  let sum = 0;
  for (const sample of samples) {
    const normalized = (sample - 128) / 128;
    sum += normalized * normalized;
  }
  return Math.min(1, Math.sqrt(sum / samples.length) * 3.2);
}

export function speakingFromLevel(level: number, speaking: boolean): boolean {
  return speaking ? level >= 0.035 : level >= 0.075;
}

let sharedAudioContext: AudioContext | null = null;

function audioContext(): AudioContext | null {
  if (sharedAudioContext && sharedAudioContext.state !== 'closed') return sharedAudioContext;
  const AudioContextConstructor = window.AudioContext
    || (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  sharedAudioContext = AudioContextConstructor ? new AudioContextConstructor() : null;
  return sharedAudioContext;
}

export function monitorAudioLevel(
  stream: MediaStream,
  onLevel: (level: number) => void
): () => void {
  if (!stream.getAudioTracks().some((track) => track.readyState === 'live')) {
    onLevel(0);
    return () => undefined;
  }

  const context = audioContext();
  if (!context) {
    onLevel(0);
    return () => undefined;
  }
  const analyser = context.createAnalyser();
  analyser.fftSize = 512;
  analyser.smoothingTimeConstant = 0.72;
  const source = context.createMediaStreamSource(stream);
  source.connect(analyser);
  const samples = new Uint8Array(analyser.fftSize);
  let frame = 0;
  let stopped = false;

  const sample = () => {
    if (stopped) return;
    analyser.getByteTimeDomainData(samples);
    onLevel(audioLevelFromTimeDomain(samples));
    frame = requestAnimationFrame(sample);
  };
  void context.resume().catch(() => undefined);
  frame = requestAnimationFrame(sample);

  return () => {
    stopped = true;
    cancelAnimationFrame(frame);
    source.disconnect();
    analyser.disconnect();
    onLevel(0);
  };
}
