import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Fail closed: browser test tones must never use the workstation's speakers.
// This helper only inspects an explicitly supplied null sink; it does not
// create devices, alter defaults/volume, or restart the user's audio services.
export function requireNullSink(sinks, modules, name) {
  if (!/^mx_test_[a-z0-9_]+$/.test(name || '')) throw new Error('Browser tests require MX_TEST_AUDIO_SINK pointing to an isolated mx_test_* null sink. Physical audio output is forbidden.');
  const sink = sinks.find((item) => item.name === name);
  const owner = modules.find((item) => Number(item.index) === Number(sink?.owner_module));
  // PipeWire marks even its null adapter HARDWARE. Accept that flag only
  // when both the owning Pulse module and SPA factory prove it is a null sink.
  const pipewireNull = sink?.properties?.['factory.name'] === 'support.null-audio-sink';
  if (!sink || (sink.flags?.includes('HARDWARE') && !pipewireNull) || owner?.name !== 'module-null-sink') {
    throw new Error(`Refusing browser audio: ${name} is not a verified module-null-sink output.`);
  }
  return sink;
}

export function isolatedBrowserAudio() {
  const name = process.env.MX_TEST_AUDIO_SINK;
  // Check the opt-in before invoking even read-only desktop audio commands.
  if (!name) requireNullSink([], [], name);
  const list = (kind) => JSON.parse(execFileSync('pactl', ['-f', 'json', 'list', kind], { encoding: 'utf8', timeout: 5000 }));
  // Some pactl/PipeWire combinations omit module indexes from JSON output.
  // The tabular format retains the IDs needed to verify sink ownership.
  const modules = execFileSync('pactl', ['list', 'short', 'modules'], { encoding: 'utf8', timeout: 5000 })
    .split('\n').flatMap((line) => { const match = line.match(/^(\d+)\t([^\t]+)/); return match ? [{ index: Number(match[1]), name: match[2] }] : []; });
  const sink = requireNullSink(list('sinks'), modules, name);
  return { ...process.env, PULSE_SINK: name, PULSE_SOURCE: sink.monitor_source,
    ALSA_CONFIG_PATH: fileURLToPath(new URL('./alsa-null.conf', import.meta.url)) };
}
