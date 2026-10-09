<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import CollaborationCall from '../../components/CollaborationCall.svelte';
  import { MxLiveClient, type LiveMessage } from '../../live/client';
  import type { Session } from '../../api/types';

  let { session, channelUid, index }: { session: Session; channelUid: string; index: number } = $props();
  let events = $state<LiveMessage[]>([]);
  let connected = $state(false);
  let joined = $state(false);
  let generation = $state(0);
  let reason = $state('');
  let clock = 0;
  const connections: RTCPeerConnection[] = [];
  const NativeConnection = window.RTCPeerConnection;
  window.RTCPeerConnection = class extends NativeConnection {
    constructor(options?: RTCConfiguration) { super(options); connections.push(this); }
  };
  const context = new AudioContext();
  const destinations = [context.createMediaStreamDestination(), context.createMediaStreamDestination()];
  const tones = untrack(() => [300 + index * 70, 1500 + index * 80]);
  const oscillators = destinations.map((destination, source) => {
    const oscillator = context.createOscillator(); const gain = context.createGain();
    oscillator.frequency.value = tones[source]; gain.gain.value = .15;
    oscillator.connect(gain).connect(destination); oscillator.start(); return oscillator;
  });
  const canvases = [document.createElement('canvas'), document.createElement('canvas')];
  for (const canvas of canvases) { canvas.width = 640; canvas.height = 360; }
  let frame = 0;
  const draw = setInterval(() => {
    for (const [source, canvas] of canvases.entries()) {
      const painter = canvas.getContext('2d')!;
      painter.fillStyle = source ? '#006655' : '#441144'; painter.fillRect(0, 0, 640, 360);
      painter.fillStyle = 'white'; painter.font = '30px sans-serif'; painter.fillText(`${session.name}: ${++frame}`, 30, 80);
    }
  }, 80);
  const devices = navigator.mediaDevices;
  (window as any).testMediaDevices = devices;
  Object.defineProperty(devices, 'getUserMedia', { value: async (options: MediaStreamConstraints) => new MediaStream([
    ...(options.audio ? [destinations[0].stream.getAudioTracks()[0].clone()] : []),
    ...(options.video ? canvases[0].captureStream(12).getVideoTracks() : [])
  ]) });
  Object.defineProperty(devices, 'getDisplayMedia', { value: async () => new MediaStream([
    ...canvases[1].captureStream(12).getVideoTracks(), destinations[1].stream.getAudioTracks()[0].clone()
  ]) });
  Object.defineProperty(devices, 'enumerateDevices', { value: async () => [
    { deviceId: 'test-mic', groupId: 'test', kind: 'audioinput', label: 'Test microphone' },
    { deviceId: 'test-camera', groupId: 'test', kind: 'videoinput', label: 'Test camera' },
    { deviceId: '', groupId: 'test', kind: 'audiooutput', label: 'System default' }
  ] });

  onMount(() => {
    const live = new MxLiveClient((event) => events = [...events.slice(-999), event], (ready) => connected = ready);
    live.start();
    const timer = setInterval(() => clock++, 100);
    (window as any).testEight = {
      state: async () => {
        const active = connections.filter((pc) => pc.connectionState !== 'closed');
        const stats: Array<{ id: string; kind: string; bytes: number; frames: number }> = [];
        for (const [index, pc] of active.entries()) for (const report of (await pc.getStats()).values()) {
          if (report.type === 'inbound-rtp') stats.push({ id: `${index}:${report.id}`, kind: report.kind || report.mediaType, bytes: report.bytesReceived || 0, frames: report.framesDecoded || 0 });
        }
        return { joined, connected, clock, reason, stats, peers: active.map((pc) => ({ state: pc.connectionState, signaling: pc.signalingState, senders: pc.getSenders().map((sender) => sender.track?.kind || null), transceivers: pc.getTransceivers().map((transceiver) => ({ mid: transceiver.mid, direction: transceiver.currentDirection, kind: transceiver.receiver.track.kind, muted: transceiver.receiver.track.muted })) })), events: events.map((event) => event.type) };
      },
      reconnect: () => { live.stop(); live.start(); }
    };
    return () => { live.stop(); clearInterval(timer); clearInterval(draw); oscillators.forEach((oscillator) => oscillator.stop()); void context.close(); window.RTCPeerConnection = NativeConnection; };
  });
  async function join() { await context.resume(); generation++; reason = ''; joined = true; }
</script>

<button id="join" disabled={joined || !connected} onclick={() => void join()}>Join test call</button>
<p id="ended">{reason}</p>
{#if joined}
  {#key generation}
    <CollaborationCall {session} {channelUid} channelName="Eight-user validation" channelKind="group" initialMode="video" {connected} callEvents={events} onEnded={(why) => { joined = false; reason = why || 'left'; }} />
  {/key}
{/if}
