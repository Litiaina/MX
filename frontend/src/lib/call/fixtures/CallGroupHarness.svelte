<script lang="ts">
  import { onMount } from 'svelte';
  import { CallController, type LocalCallMedia, type RemoteCallMedia, type IncomingCallSignal } from '../controller';
  import CallMediaTile from '../../components/CallMediaTile.svelte';
  import '../../../styles/workspaces.css';

  const params = new URL(location.href).searchParams;
  const self = params.get('user') || 'a';
  const users = ['a', 'm', 'z'];
  let media = $state<RemoteCallMedia[]>([]);
  let localMedia = $state<LocalCallMedia | null>(null);
  let modes = $state<Record<string, boolean>>({});
  let counter = $state(0);
  const diagnostics: string[] = [];
  const audioContext = new AudioContext();
  const destination = audioContext.createMediaStreamDestination();
  const oscillator = audioContext.createOscillator();
  oscillator.connect(destination);
  oscillator.start();
  const canvas = document.createElement('canvas');
  canvas.width = 640;
  canvas.height = 360;
  const context = canvas.getContext('2d')!;
  let frame = 0;
  function draw() {
    context.fillStyle = '#bb2277';
    context.fillRect(0, 0, 640, 360);
    context.fillStyle = '#ffffff';
    context.font = '32px sans-serif';
    context.fillText(`Shared screen ${self} ${++frame}`, 30, 80);
    requestAnimationFrame(draw);
  }
  draw();
  const nativeCapture = navigator.mediaDevices.getDisplayMedia.bind(navigator.mediaDevices);
  Object.defineProperty(navigator.mediaDevices, 'getUserMedia', { value: async () => destination.stream });
  // Force actual self-tab capture in the browser test, including browsers that
  // would ignore the application's exclusion hint. It must remain safe anyway.
  Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { value: async () => params.has('real')
    ? nativeCapture({ video: { frameRate: { ideal: 15, max: 30 } }, audio: false, preferCurrentTab: true, selfBrowserSurface: 'include' } as DisplayMediaStreamOptions)
    : canvas.captureStream(15) });
  const controller = new CallController(self, `${self}-session`, [],
    (uid, sessionUid, kind, data) => (window as any).relay(uid, {
      senderUid: self, senderSessionUid: `${self}-session`, recipientSessionUid: sessionUid, kind, data
    }), (next) => media = next, (next) => localMedia = next, (error) => diagnostics.push(error), () => undefined);
  const localParticipant = $derived({ user_uid: self, session_uid: `${self}-session`, user_name: self, audio_enabled: true,
    video_enabled: !!localMedia?.screenSharing, screen_sharing: !!localMedia?.screenSharing, joined_at: 1 });

  onMount(() => {
    const timer = setInterval(() => counter += 1, 100);
    (window as any).testCall = {
      start: () => controller.start('voice'),
      connect: () => controller.connectToExisting(users.filter((uid) => uid !== self).map((userUid) => ({ userUid, sessionUid: `${userUid}-session` }))),
      signal: (signal: IncomingCallSignal) => controller.handleSignal(signal),
      share: () => controller.startScreenShare(),
      stop: () => controller.stopScreenShare(),
      microphone: (enabled: boolean) => controller.setMicrophoneEnabled(enabled),
      source: (uid: string, active: boolean) => modes = { ...modes, [uid]: active },
      state: () => ({ errors: diagnostics, sharing: localMedia?.screenSharing, screenSettings: localMedia?.stream.getVideoTracks()[0]?.getSettings(),
        peers: media.map((peer) => ({ user: peer.userUid, state: peer.connectionState, tracks: peer.stream.getTracks().map((track) => ({ kind: track.kind, id: track.id, muted: track.muted })) })) }),
      close: () => controller.close()
    };
    return () => { clearInterval(timer); controller.close(); void audioContext.close(); };
  });
</script>
<button id="share" onclick={() => void controller.startScreenShare().catch((reason) => diagnostics.push(String(reason)))}>Share screen</button>
<button id="stop" onclick={() => void controller.stopScreenShare()}>Stop sharing</button>
<strong id="clock">Live UI {counter}</strong>
<div class="call-media-grid" style="height:700px;grid-template-columns:repeat(3,1fr)">
  <div data-peer={self} style="position:relative;height:320px;display:grid"><CallMediaTile participant={localParticipant} stream={localMedia?.stream || null} local connectionState="connected" /></div>
  {#each users.filter((uid) => uid !== self) as uid (uid)}
    {@const peer = media.find((item) => item.userUid === uid)}
    <div data-peer={uid} style="position:relative;height:320px;display:grid">
      <CallMediaTile participant={{ user_uid: uid, session_uid: `${uid}-session`, user_name: uid, audio_enabled: true, video_enabled: !!modes[uid], screen_sharing: !!modes[uid], joined_at: 1 }} stream={peer?.stream || null} connectionState={peer?.connectionState || 'new'} />
    </div>
  {/each}
</div>
