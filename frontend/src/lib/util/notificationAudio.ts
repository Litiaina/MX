let context: AudioContext | null = null;
export type NotificationSoundKind = 'notification' | 'direct-call' | 'group-call';

function audioContext(): AudioContext {
  context ||= new AudioContext();
  return context;
}

export async function unlockNotificationAudio(): Promise<void> {
  const current = audioContext();
  if (current.state === 'suspended') await current.resume();
}

export async function decodeNotificationSound(blob: Blob): Promise<AudioBuffer> {
  const current = audioContext();
  const bytes = await blob.arrayBuffer();
  return current.decodeAudioData(bytes.slice(0));
}

export async function playNotificationSound(buffer: AudioBuffer | null, volumePercent: number, kind: NotificationSoundKind = 'notification'): Promise<() => void> {
  const current = audioContext();
  if (current.state === 'suspended') await current.resume();
  const volume = Math.min(1, Math.max(0, volumePercent / 100));
  const gain = current.createGain();
  gain.gain.setValueAtTime(volume, current.currentTime);
  gain.connect(current.destination);

  if (buffer) {
    const source = current.createBufferSource();
    let stopped = false;
    const stop = () => {
      if (stopped) return;
      stopped = true;
      try { source.stop(); } catch { /* The source already ended. */ }
      source.disconnect();
      gain.disconnect();
    };
    source.buffer = buffer;
    source.connect(gain);
    source.start();
    source.addEventListener('ended', stop, { once: true });
    return stop;
  }

  if (kind === 'direct-call') {
    const notes = [
      { frequency: 523, start: 0, duration: .18 },
      { frequency: 659, start: .22, duration: .18 },
      { frequency: 523, start: .56, duration: .18 },
      { frequency: 659, start: .78, duration: .22 }
    ];
    const nodes: Array<{ oscillator: OscillatorNode; envelope: GainNode }> = [];
    let stopped = false;
    const stop = () => {
      if (stopped) return;
      stopped = true;
      for (const node of nodes) {
        try { node.oscillator.stop(); } catch { /* The note already ended. */ }
        node.oscillator.disconnect();
        node.envelope.disconnect();
      }
      gain.disconnect();
    };
    for (const note of notes) {
      const oscillator = current.createOscillator();
      const envelope = current.createGain();
      const startsAt = current.currentTime + note.start;
      oscillator.type = 'sine';
      oscillator.frequency.setValueAtTime(note.frequency, startsAt);
      envelope.gain.setValueAtTime(.0001, startsAt);
      envelope.gain.exponentialRampToValueAtTime(.26, startsAt + .018);
      envelope.gain.exponentialRampToValueAtTime(.0001, startsAt + note.duration);
      oscillator.connect(envelope); envelope.connect(gain);
      oscillator.start(startsAt); oscillator.stop(startsAt + note.duration);
      nodes.push({ oscillator, envelope });
    }
    nodes.at(-1)?.oscillator.addEventListener('ended', stop, { once: true });
    return stop;
  }

  const first = current.createOscillator();
  const second = current.createOscillator();
  const envelope = current.createGain();
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    try { first.stop(); } catch { /* The note already ended. */ }
    try { second.stop(); } catch { /* The note already ended. */ }
    first.disconnect(); second.disconnect(); envelope.disconnect(); gain.disconnect();
  };
  first.type = 'sine'; second.type = 'sine';
  first.frequency.setValueAtTime(kind === 'group-call' ? 560 : 660, current.currentTime);
  second.frequency.setValueAtTime(kind === 'group-call' ? 720 : 880, current.currentTime + .11);
  envelope.gain.setValueAtTime(0.0001, current.currentTime);
  envelope.gain.exponentialRampToValueAtTime(.32, current.currentTime + .018);
  envelope.gain.exponentialRampToValueAtTime(0.0001, current.currentTime + .34);
  first.connect(envelope); second.connect(envelope); envelope.connect(gain);
  first.start(current.currentTime); first.stop(current.currentTime + .2);
  second.start(current.currentTime + .1); second.stop(current.currentTime + .34);
  second.addEventListener('ended', stop, { once: true });
  return stop;
}
