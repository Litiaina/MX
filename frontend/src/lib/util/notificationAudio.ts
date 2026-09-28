let context: AudioContext | null = null;

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

export async function playNotificationSound(buffer: AudioBuffer | null, volumePercent: number): Promise<void> {
  const current = audioContext();
  if (current.state === 'suspended') await current.resume();
  const volume = Math.min(1, Math.max(0, volumePercent / 100));
  const gain = current.createGain();
  gain.gain.setValueAtTime(volume, current.currentTime);
  gain.connect(current.destination);

  if (buffer) {
    const source = current.createBufferSource();
    source.buffer = buffer;
    source.connect(gain);
    source.start();
    source.addEventListener('ended', () => gain.disconnect(), { once: true });
    return;
  }

  const first = current.createOscillator();
  const second = current.createOscillator();
  const envelope = current.createGain();
  first.type = 'sine'; second.type = 'sine';
  first.frequency.setValueAtTime(660, current.currentTime);
  second.frequency.setValueAtTime(880, current.currentTime + .11);
  envelope.gain.setValueAtTime(0.0001, current.currentTime);
  envelope.gain.exponentialRampToValueAtTime(.32, current.currentTime + .018);
  envelope.gain.exponentialRampToValueAtTime(0.0001, current.currentTime + .34);
  first.connect(envelope); second.connect(envelope); envelope.connect(gain);
  first.start(current.currentTime); first.stop(current.currentTime + .2);
  second.start(current.currentTime + .1); second.stop(current.currentTime + .34);
  second.addEventListener('ended', () => { envelope.disconnect(); gain.disconnect(); }, { once: true });
}
