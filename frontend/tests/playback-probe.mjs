// Measure the actual rendered PCM from the verified null output, not merely
// received RTP tracks. MediaElementAudioSource on srcObject is silent in some
// engines, so it is not a valid playback oracle for these tests.
import { spawn } from 'node:child_process';
import { isolatedBrowserAudio } from './silent-audio.mjs';

export async function playbackTones(frequencies, durationMs = 1200) {
  const env = isolatedBrowserAudio();
  const recorder = spawn('parec', ['--device', env.PULSE_SOURCE, '--format=float32le', '--rate=48000', '--channels=1', '--latency-msec=50'], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const chunks = []; let error = '';
  recorder.stdout.on('data', (chunk) => chunks.push(chunk));
  recorder.stderr.on('data', (chunk) => error += chunk.toString());
  const exited = new Promise((resolve, reject) => { recorder.once('error', reject); recorder.once('exit', resolve); });
  const timer = setTimeout(() => recorder.kill('SIGTERM'), durationMs);
  try { await exited; } finally { clearTimeout(timer); }
  const bytes = Buffer.concat(chunks);
  if (bytes.length < 48000 * 4 / 4) throw new Error(`Silent-output capture failed: ${error || 'insufficient PCM samples'}`);
  // Ignore initial monitor latency; use a fixed-size Hann window to suppress
  // spectral leakage from the other participants' deliberately distinct tones.
  const size = Math.min(32768, Math.floor(bytes.length / 4));
  const start = Math.floor(bytes.length / 4) - size;
  const samples = Array.from({ length: size }, (_, i) => bytes.readFloatLE((start + i) * 4) * (.5 - .5 * Math.cos(2 * Math.PI * i / (size - 1))));
  return frequencies.map((frequency) => {
    let real = 0; let imaginary = 0;
    for (let i = 0; i < size; i++) { const phase = 2 * Math.PI * frequency * i / 48000; real += samples[i] * Math.cos(phase); imaginary -= samples[i] * Math.sin(phase); }
    const amplitude = 4 * Math.hypot(real, imaginary) / size;
    return 20 * Math.log10(Math.max(1e-12, amplitude));
  });
}
