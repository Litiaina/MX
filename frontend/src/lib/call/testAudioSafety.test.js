import { describe, expect, it } from 'vitest';
import { requireNullSink } from '../../../tests/silent-audio.mjs';

describe('browser test audio isolation', () => {
  const modules = [{ index: 7, name: 'module-null-sink' }];
  const sink = { name: 'mx_test_silent', owner_module: 7, flags: [], monitor_source: 'mx_test_silent.monitor' };
  it('accepts only an explicit test sink owned by the null-output module', () => {
    expect(requireNullSink([sink], modules, sink.name)).toBe(sink);
  });
  it('recognizes the PipeWire null factory despite its generic HARDWARE adapter flag', () => {
    const pipewireSink = { ...sink, flags: ['HARDWARE'], properties: { 'factory.name': 'support.null-audio-sink' } };
    expect(requireNullSink([pipewireSink], modules, sink.name)).toBe(pipewireSink);
    expect(() => requireNullSink([pipewireSink], [], sink.name)).toThrow();
  });
  it('rejects the default, unknown, physical, and deceptively named outputs', () => {
    expect(() => requireNullSink([sink], modules)).toThrow();
    expect(() => requireNullSink([sink], modules, 'default')).toThrow();
    expect(() => requireNullSink([], modules, sink.name)).toThrow();
    expect(() => requireNullSink([{ ...sink, flags: ['HARDWARE'] }], modules, sink.name)).toThrow();
    expect(() => requireNullSink([sink], [{ index: 7, name: 'module-alsa-sink' }], sink.name)).toThrow();
    expect(() => requireNullSink([{ ...sink, owner_module: 9 }], modules, sink.name)).toThrow();
  });
});
