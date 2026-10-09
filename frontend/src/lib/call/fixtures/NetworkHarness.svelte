<script lang="ts">
  import { onMount } from 'svelte';
  import { CallController, type LocalCallMedia, type RemoteCallMedia, type IncomingCallSignal } from '../controller';
  import CallMediaTile from '../../components/CallMediaTile.svelte';
  import '../../../styles/tokens.css';
  import '../../../styles/workspaces.css';

  const params = new URL(location.href).searchParams;
  const self = params.get('user') || 'a';
  const users = (params.get('users') || 'a,z').split(',');
  let media = $state<RemoteCallMedia[]>([]);
  let local = $state<LocalCallMedia | null>(null);
  let sharing = $state<Record<string, boolean>>({});
  let clock = $state(0);
  let controller: CallController;
  let generation = 1;
  const diagnostics: string[] = [];
  const connections: RTCPeerConnection[] = [];
  const NativeConnection = window.RTCPeerConnection;
  window.RTCPeerConnection = class extends NativeConnection {
    constructor(options?: RTCConfiguration) { super(options); connections.push(this); }
  };
  const context = new AudioContext();
  const destination = context.createMediaStreamDestination();
  const oscillator = context.createOscillator();
  oscillator.connect(destination); oscillator.start();
  const canvases = [document.createElement('canvas'), document.createElement('canvas')];
  for (const canvas of canvases) { canvas.width = 640; canvas.height = 360; }
  let frame = 0;
  const draw = window.setInterval(() => {
    for (const [index, canvas] of canvases.entries()) {
      const painter = canvas.getContext('2d')!;
      painter.fillStyle = index ? '#006655' : '#441144'; painter.fillRect(0, 0, 640, 360);
      painter.fillStyle = 'white'; painter.font = '30px sans-serif'; painter.fillText(`${self}: ${++frame}`, 30, 80);
    }
  }, 80);
  // Keep this MediaDevices wrapper alive: WebKit can otherwise collect the
  // wrapper (and its own-property capture stubs) between user gestures.
  const devices = navigator.mediaDevices;
  (window as any).testMediaDevices = devices;
  Object.defineProperty(devices, 'getUserMedia', { value: async (options: MediaStreamConstraints) => new MediaStream([
    ...(options.audio ? [destination.stream.getAudioTracks()[0].clone()] : []),
    ...(options.video ? canvases[0].captureStream(12).getVideoTracks() : [])
  ]) });
  Object.defineProperty(devices, 'getDisplayMedia', { value: async () => new MediaStream([
    ...canvases[1].captureStream(12).getVideoTracks(), destination.stream.getAudioTracks()[0].clone()
  ]) });
  function create() {
    controller = new CallController(self, `${self}-${generation}`, [],
      (uid, sessionUid, kind, data) => (window as any).relay(uid, { senderUid: self, senderSessionUid: `${self}-${generation}`, recipientSessionUid: sessionUid, kind, data }),
      (peers) => media = peers, (state) => local = state, (error) => diagnostics.push(error), () => undefined);
  }
  create();
  const localParticipant = $derived({ user_uid: self, session_uid: `${self}-${generation}`, user_name: self, audio_enabled: local?.microphoneEnabled ?? false,
    video_enabled: !!local?.cameraEnabled || !!local?.screenSharing, screen_sharing: !!local?.screenSharing, joined_at: 1 });
  async function start() { await context.resume(); await controller.start('video'); (window as any).started = true; }
  onMount(() => {
    const timer = setInterval(() => clock += 1, 100);
    (window as any).testCall = {
      signal: (signal: IncomingCallSignal) => controller.handleSignal(signal),
      connect: (generations: Record<string, number> = {}) => controller.connectToExisting(users.filter((uid) => uid !== self).map((userUid) => ({ userUid, sessionUid: `${userUid}-${generations[userUid] || 1}` }))),
      source: (uid: string, active: boolean) => sharing = { ...sharing, [uid]: active },
      stop: () => controller.stopScreenShare(),
      camera: (enabled: boolean) => controller.setCameraEnabled(enabled),
      rejoin: async () => { controller.close(); generation++; create(); await start(); },
      remove: (uid: string) => controller.removePeer(uid),
      state: async () => {
        const stats: Array<{ kind: string; bytes: number; frames: number }> = [];
        const transportStats: unknown[] = [];
        const active = connections.filter((pc) => pc.connectionState !== 'closed');
        for (const pc of active) for (const report of (await pc.getStats()).values()) {
          if (report.type === 'inbound-rtp') stats.push({ kind: report.kind || report.mediaType, bytes: report.bytesReceived || 0, frames: report.framesDecoded || 0 });
          if (['outbound-rtp', 'transport', 'candidate-pair'].includes(report.type)) transportStats.push(report);
        }
        return { errors: diagnostics, sharing: local?.screenSharing, peers: active.map((pc) => ({ state: pc.connectionState, signaling: pc.signalingState, senders: pc.getSenders().map((sender) => ({ kind: sender.track?.kind, enabled: sender.track?.enabled, state: sender.track?.readyState, transport: sender.transport?.state })), transceivers: pc.getTransceivers().map((transceiver) => ({ direction: transceiver.currentDirection, kind: transceiver.receiver.track.kind, muted: transceiver.receiver.track.muted })), local: pc.localDescription?.sdp, remote: pc.remoteDescription?.sdp })), stats, transportStats, clock };
      },
      close: () => controller.close()
    };
    return () => { clearInterval(timer); clearInterval(draw); controller.close(); window.RTCPeerConnection = NativeConnection; void context.close(); };
  });
</script>
<button id="start" onclick={() => void start()}>Start media</button>
<button id="share" onclick={() => void controller.startScreenShare()}>Share screen</button>
<strong id="clock">Live UI {clock}</strong>
<div class="call-media-grid" style="height:400px;grid-template-columns:repeat(3,1fr)">
  <div data-peer={self} style="position:relative;height:320px;display:grid"><CallMediaTile participant={localParticipant} stream={local?.stream || null} local connectionState="connected" /></div>
  {#each users.filter((uid) => uid !== self) as uid (uid)}
    {@const peer = media.find((item) => item.userUid === uid)}
    <div data-peer={uid} style="position:relative;height:320px;display:grid">
      <CallMediaTile participant={{ user_uid: uid, session_uid: `${uid}-1`, user_name: uid, audio_enabled: true, video_enabled: true, screen_sharing: !!sharing[uid], joined_at: 1 }} stream={peer?.stream || null} connectionState={peer?.connectionState || 'new'} />
    </div>
  {/each}
</div>
