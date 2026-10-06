import type { CallIceServer, CallMode, CallSignalKind } from '../api/domain';

export interface RemoteCallMedia {
  userUid: string;
  stream: MediaStream;
  connectionState: RTCPeerConnectionState;
}

export interface LocalCallMedia {
  stream: MediaStream;
  microphoneEnabled: boolean;
  cameraEnabled: boolean;
  screenSharing: boolean;
}

export interface IncomingCallSignal {
  senderUid: string;
  senderSessionUid: string;
  recipientSessionUid: string;
  kind: CallSignalKind;
  data: Record<string, unknown>;
}

export interface CallPeerIdentity {
  userUid: string;
  sessionUid: string;
}

type SendSignal = (recipientUid: string, recipientSessionUid: string, kind: CallSignalKind, data: object) => Promise<unknown>;

interface PeerState {
  sessionUid: string;
  connection: RTCPeerConnection;
  remoteStream: MediaStream;
  polite: boolean;
  makingOffer: boolean;
  ignoreOffer: boolean;
  settingRemoteAnswer: boolean;
  negotiationAllowed: boolean;
  negotiationPending: boolean;
  pendingCandidates: RTCIceCandidateInit[];
  videoSender: RTCRtpSender | null;
  screenAudioSender: RTCRtpSender | null;
}

export function callMediaConstraints(mode: CallMode): MediaStreamConstraints {
  return {
    audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
    video: mode === 'video'
      ? { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 24, max: 30 }, facingMode: 'user' }
      : false
  };
}

export function screenShareConstraints(): DisplayMediaStreamOptions & { selfBrowserSurface: 'exclude'; preferCurrentTab: false } {
  return {
    video: { frameRate: { ideal: 15, max: 30 } },
    audio: true,
    selfBrowserSurface: 'exclude',
    preferCurrentTab: false
  };
}

export function callPermissionMessage(reason: unknown): string {
  const name = reason instanceof DOMException ? reason.name : '';
  if (name === 'NotAllowedError' || name === 'PermissionDeniedError') return 'Camera or microphone permission was denied. Allow access in the browser site settings and try again.';
  if (name === 'NotFoundError' || name === 'DevicesNotFoundError') return 'No usable camera or microphone was found on this device.';
  if (name === 'NotReadableError' || name === 'TrackStartError') return 'The camera or microphone is already in use or unavailable to the browser.';
  if (name === 'OverconstrainedError') return 'The selected camera cannot provide a compatible video format.';
  return reason instanceof Error ? reason.message : 'The browser could not start the call media devices.';
}

export function isCallSignal(value: unknown): value is IncomingCallSignal {
  if (!value || typeof value !== 'object') return false;
  const signal = value as Partial<IncomingCallSignal>;
  return typeof signal.senderUid === 'string'
    && typeof signal.senderSessionUid === 'string'
    && typeof signal.recipientSessionUid === 'string'
    && ['offer', 'answer', 'ice'].includes(String(signal.kind))
    && !!signal.data
    && typeof signal.data === 'object';
}

export function candidateMatchesRemoteDescription(
  candidate: RTCIceCandidateInit,
  description: RTCSessionDescription | RTCSessionDescriptionInit | null
): boolean {
  const usernameFragment = candidate.usernameFragment?.trim();
  if (!usernameFragment || !description?.sdp) return true;
  return description.sdp
    .split(/\r?\n/)
    .some((line) => line.trim() === `a=ice-ufrag:${usernameFragment}`);
}

export class CallController {
  #localSource = new MediaStream();
  #cameraTrack: MediaStreamTrack | null = null;
  #screenTrack: MediaStreamTrack | null = null;
  #screenAudioTrack: MediaStreamTrack | null = null;
  #microphoneTrack: MediaStreamTrack | null = null;
  #preferredMicrophoneDeviceId = '';
  #preferredCameraDeviceId = '';
  #peers = new Map<string, PeerState>();
  #closed = false;

  constructor(
    private readonly selfUid: string,
    private readonly selfSessionUid: string,
    private readonly iceServers: CallIceServer[],
    private readonly sendSignal: SendSignal,
    private readonly onRemoteChanged: (peers: RemoteCallMedia[]) => void,
    private readonly onLocalChanged: (media: LocalCallMedia) => void,
    private readonly onError: (message: string) => void,
    private readonly onScreenEnded: () => void
  ) {}

  async start(mode: CallMode): Promise<void> {
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('This browser does not support secure camera and microphone access. Open MX over HTTPS in a current browser.');
    const stream = await navigator.mediaDevices.getUserMedia(callMediaConstraints(mode));
    if (this.#closed) { stream.getTracks().forEach((track) => track.stop()); return; }
    this.#microphoneTrack = stream.getAudioTracks()[0] || null;
    this.#cameraTrack = stream.getVideoTracks()[0] || null;
    for (const track of stream.getTracks()) this.#localSource.addTrack(track);
    this.#emitLocal();
  }

  connectToExisting(peers: CallPeerIdentity[]): void {
    for (const peer of peers) {
      if (peer.userUid === this.selfUid) continue;
      // Both browsers learn about the same participant pair. Only one side
      // should make the initial offer; otherwise rejoin reliably creates glare.
      const shouldInitiate = this.selfUid.localeCompare(peer.userUid) < 0;
      this.#ensurePeer(peer.userUid, peer.sessionUid, shouldInitiate);
    }
  }

  async handleSignal(signal: IncomingCallSignal): Promise<void> {
    if (this.#closed || signal.senderUid === this.selfUid || signal.recipientSessionUid !== this.selfSessionUid) return;
    const state = this.#ensurePeer(signal.senderUid, signal.senderSessionUid, false);
    const connection = state.connection;
    try {
      if (signal.kind === 'ice') {
        const candidate = signal.data as RTCIceCandidateInit;
        if (!connection.remoteDescription) state.pendingCandidates.push(candidate);
        else await this.#addRemoteCandidate(signal.senderUid, state, candidate);
        return;
      }

      const description = signal.data as unknown as RTCSessionDescriptionInit;
      if (!['offer', 'answer'].includes(String(description.type))) return;
      const readyForOffer = !state.makingOffer && (connection.signalingState === 'stable' || state.settingRemoteAnswer);
      const offerCollision = description.type === 'offer' && !readyForOffer;
      state.ignoreOffer = !state.polite && offerCollision;
      if (state.ignoreOffer) return;

      state.settingRemoteAnswer = description.type === 'answer';
      await connection.setRemoteDescription(description);
      state.settingRemoteAnswer = false;
      state.negotiationAllowed = true;
      for (const candidate of state.pendingCandidates.splice(0)) {
        await this.#addRemoteCandidate(signal.senderUid, state, candidate);
      }
      if (description.type === 'offer') {
        // With a remote offer installed, the no-argument form atomically
        // creates and applies the correct answer for the current state.
        // The answer includes local tracks already queued before it is built.
        state.negotiationPending = false;
        await connection.setLocalDescription();
        if (connection.localDescription) await this.#signalDescription(signal.senderUid, connection.localDescription);
      }
      // Track additions requested during an outstanding offer must resume as
      // soon as its answer arrives, rather than waiting for participant refresh.
      void this.#negotiate(signal.senderUid, state);
    } catch (reason) {
      state.settingRemoteAnswer = false;
      if (!state.ignoreOffer) this.onError(reason instanceof Error ? `Call negotiation failed: ${reason.message}` : 'Call negotiation failed.');
    }
  }

  removePeer(userUid: string): void {
    const peer = this.#peers.get(userUid);
    if (!peer) return;
    peer.connection.ontrack = null;
    peer.connection.onicecandidate = null;
    peer.connection.onnegotiationneeded = null;
    peer.connection.onconnectionstatechange = null;
    peer.connection.close();
    peer.remoteStream.getTracks().forEach((track) => track.stop());
    this.#peers.delete(userUid);
    this.#emitRemote();
  }

  setMicrophoneEnabled(enabled: boolean): void {
    if (!this.#microphoneTrack) return;
    this.#microphoneTrack.enabled = enabled;
    this.#emitLocal();
  }

  async setCameraEnabled(enabled: boolean): Promise<void> {
    if (enabled && !this.#cameraTrack) {
      if (!navigator.mediaDevices?.getUserMedia) throw new Error('Camera access is unavailable in this browser.');
      const video = callMediaConstraints('video').video as MediaTrackConstraints;
      if (this.#preferredCameraDeviceId) video.deviceId = { exact: this.#preferredCameraDeviceId };
      const stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
      const track = stream.getVideoTracks()[0];
      if (!track) throw new Error('The browser did not provide a camera track.');
      this.#cameraTrack = track;
      this.#localSource.addTrack(track);
      if (!this.#screenTrack) await this.#replaceOutgoingVideo(track, this.#localSource);
    } else if (!enabled && this.#cameraTrack) {
      const track = this.#cameraTrack;
      this.#cameraTrack = null;
      this.#localSource.removeTrack(track);
      track.stop();
      if (!this.#screenTrack) await this.#replaceOutgoingVideo(null, this.#localSource);
    }
    this.#emitLocal();
  }

  async selectMicrophoneDevice(deviceId: string): Promise<void> {
    this.#preferredMicrophoneDeviceId = deviceId;
    if (!deviceId || !navigator.mediaDevices?.getUserMedia) return;
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { deviceId: { exact: deviceId }, echoCancellation: true, noiseSuppression: true, autoGainControl: true }, video: false });
    const track = stream.getAudioTracks()[0];
    if (!track) throw new Error('The selected microphone did not provide an audio track.');
    const previous = this.#microphoneTrack;
    track.enabled = previous?.enabled ?? true;
    if (previous) this.#localSource.removeTrack(previous);
    this.#microphoneTrack = track;
    this.#localSource.addTrack(track);
    for (const peer of this.#peers.values()) {
      const sender = peer.connection.getSenders().find((item) => item.track?.kind === 'audio');
      if (sender) await sender.replaceTrack(track);
      else peer.connection.addTrack(track, this.#localSource);
    }
    previous?.stop();
    this.#emitLocal();
  }

  async selectCameraDevice(deviceId: string): Promise<void> {
    this.#preferredCameraDeviceId = deviceId;
    if (!deviceId || !this.#cameraTrack || !navigator.mediaDevices?.getUserMedia) return;
    const stream = await navigator.mediaDevices.getUserMedia({ video: { deviceId: { exact: deviceId }, width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 24, max: 30 } }, audio: false });
    const track = stream.getVideoTracks()[0];
    if (!track) throw new Error('The selected camera did not provide a video track.');
    const previous = this.#cameraTrack;
    this.#localSource.removeTrack(previous);
    this.#cameraTrack = track;
    this.#localSource.addTrack(track);
    if (!this.#screenTrack) {
      await this.#replaceOutgoingVideo(track, this.#localSource);
    }
    previous.stop();
    this.#emitLocal();
  }

  async startScreenShare(): Promise<void> {
    if (!navigator.mediaDevices?.getDisplayMedia) throw new Error('Screen sharing is not supported by this browser.');
    if (this.#screenTrack) return;
    const stream = await navigator.mediaDevices.getDisplayMedia(screenShareConstraints());
    const track = stream.getVideoTracks()[0];
    if (!track) throw new Error('The browser did not provide a screen-sharing track.');
    this.#screenTrack = track;
    this.#screenAudioTrack = stream.getAudioTracks()[0] || null;
    track.onended = () => {
      void this.stopScreenShare()
        .catch((reason) => this.onError(callPermissionMessage(reason)))
        .finally(this.onScreenEnded);
    };
    if (this.#screenAudioTrack) {
      const screenAudioTrack = this.#screenAudioTrack;
      screenAudioTrack.onended = () => {
        if (this.#screenAudioTrack !== screenAudioTrack) return;
        this.#screenAudioTrack = null;
        void this.#replaceOutgoingScreenAudio(null, stream);
      };
    }
    await this.#replaceOutgoingVideo(track, stream);
    await this.#replaceOutgoingScreenAudio(this.#screenAudioTrack, stream);
    this.#emitLocal();
  }

  async stopScreenShare(): Promise<void> {
    const track = this.#screenTrack;
    if (!track) return;
    const screenAudioTrack = this.#screenAudioTrack;
    this.#screenTrack = null;
    this.#screenAudioTrack = null;
    track.onended = null;
    if (screenAudioTrack) screenAudioTrack.onended = null;
    track.stop();
    screenAudioTrack?.stop();
    if (this.#cameraTrack?.readyState === 'ended') {
      this.#localSource.removeTrack(this.#cameraTrack);
      this.#cameraTrack = null;
      const video = callMediaConstraints('video').video as MediaTrackConstraints;
      if (this.#preferredCameraDeviceId) video.deviceId = { exact: this.#preferredCameraDeviceId };
      const cameraStream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
      this.#cameraTrack = cameraStream.getVideoTracks()[0] || null;
      if (this.#cameraTrack) this.#localSource.addTrack(this.#cameraTrack);
    }
    await this.#replaceOutgoingVideo(this.#cameraTrack, this.#localSource);
    await this.#replaceOutgoingScreenAudio(null, this.#localSource);
    this.#emitLocal();
  }

  localState(): LocalCallMedia {
    const stream = new MediaStream();
    if (this.#microphoneTrack) stream.addTrack(this.#microphoneTrack);
    const video = this.#screenTrack || this.#cameraTrack;
    if (video) stream.addTrack(video);
    return {
      stream,
      microphoneEnabled: this.#microphoneTrack?.enabled === true,
      cameraEnabled: this.#cameraTrack !== null,
      screenSharing: this.#screenTrack !== null
    };
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const userUid of [...this.#peers.keys()]) this.removePeer(userUid);
    for (const track of this.#localSource.getTracks()) track.stop();
    this.#screenTrack?.stop();
    this.#screenAudioTrack?.stop();
    this.#cameraTrack = null;
    this.#screenTrack = null;
    this.#screenAudioTrack = null;
    this.#microphoneTrack = null;
    this.#localSource = new MediaStream();
    this.#emitLocal();
  }

  #ensurePeer(userUid: string, sessionUid: string, initiate: boolean): PeerState {
    const existing = this.#peers.get(userUid);
    if (existing && existing.sessionUid !== sessionUid) this.removePeer(userUid);
    const current = this.#peers.get(userUid);
    if (current) {
      if (initiate && !current.makingOffer && !current.connection.localDescription && !current.connection.remoteDescription) {
        current.negotiationAllowed = true;
        current.negotiationPending = true;
        void this.#negotiate(userUid, current);
      }
      return current;
    }
    const connection = new RTCPeerConnection({ iceServers: this.iceServers });
    const remoteStream = new MediaStream();
    const state: PeerState = {
      sessionUid,
      connection,
      remoteStream,
      polite: this.selfUid.localeCompare(userUid) > 0,
      makingOffer: false,
      ignoreOffer: false,
      settingRemoteAnswer: false,
      negotiationAllowed: initiate,
      negotiationPending: initiate,
      pendingCandidates: [],
      videoSender: null,
      screenAudioSender: null
    };
    this.#peers.set(userUid, state);
    if (this.#microphoneTrack) connection.addTrack(this.#microphoneTrack, this.#localSource);
    if (this.#screenAudioTrack) state.screenAudioSender = connection.addTrack(this.#screenAudioTrack, new MediaStream([this.#screenAudioTrack]));
    const outgoingVideo = this.#screenTrack || this.#cameraTrack;
    if (outgoingVideo) state.videoSender = connection.addTrack(outgoingVideo, new MediaStream([outgoingVideo]));
    connection.onicecandidate = (event) => {
      if (event.candidate) void this.sendSignal(userUid, state.sessionUid, 'ice', event.candidate.toJSON()).catch((reason) => this.onError(reason instanceof Error ? reason.message : 'Could not relay a network candidate.'));
    };
    connection.ontrack = (event) => {
      if (this.#closed || this.#peers.get(userUid) !== state) return;
      if (state.remoteStream.getTracks().includes(event.track)) return;
      // Track events may arrive after the participant's screen-sharing state.
      // Publish a new wrapper only when the track set changes so Svelte notices
      // the media arrival, while ordinary heartbeats keep playback untouched.
      const tracks = state.remoteStream.getTracks().filter((track) =>
        track.id !== event.track.id && (event.track.kind !== 'video' || track.kind !== 'video'));
      state.remoteStream = new MediaStream([...tracks, event.track]);
      event.track.onended = () => {
        if (this.#closed || this.#peers.get(userUid) !== state) return;
        const remaining = state.remoteStream.getTracks().filter((track) => track.id !== event.track.id);
        if (remaining.length === state.remoteStream.getTracks().length) return;
        state.remoteStream = new MediaStream(remaining);
        this.#emitRemote();
      };
      this.#emitRemote();
    };
    connection.onconnectionstatechange = () => {
      if (connection.connectionState === 'failed') {
        connection.restartIce();
        state.negotiationPending = true;
        void this.#negotiate(userUid, state);
      }
      this.#emitRemote();
    };
    connection.onnegotiationneeded = () => {
      state.negotiationPending = true;
      void this.#negotiate(userUid, state);
    };
    this.#emitRemote();
    if (initiate && connection.signalingState === 'stable') void this.#negotiate(userUid, state);
    return state;
  }

  async #negotiate(userUid: string, state: PeerState): Promise<void> {
    if (this.#closed || this.#peers.get(userUid) !== state || !state.negotiationAllowed || !state.negotiationPending
      || state.makingOffer || state.connection.signalingState !== 'stable') return;
    let failed = false;
    try {
      state.makingOffer = true;
      state.negotiationPending = false;
      // The no-argument form lets the browser choose offer/answer atomically
      // inside its signaling operation queue, avoiding a createOffer → remote
      // offer → setLocalDescription race.
      await state.connection.setLocalDescription();
      if (state.connection.localDescription) await this.#signalDescription(userUid, state.connection.localDescription);
    } catch (reason) {
      failed = true;
      state.negotiationPending = true;
      if (String(state.connection.signalingState) !== 'have-remote-offer') {
        this.onError(reason instanceof Error ? `Could not negotiate call media: ${reason.message}` : 'Could not negotiate call media.');
      }
    } finally {
      state.makingOffer = false;
      // An answer can finish while the HTTP send of our offer is still pending.
      // Resume a queued track change after both operations have completed.
      if (!failed && state.negotiationPending && state.connection.signalingState === 'stable') {
        queueMicrotask(() => { void this.#negotiate(userUid, state); });
      }
    }
  }

  async #replaceOutgoingVideo(track: MediaStreamTrack | null, stream: MediaStream): Promise<void> {
    await Promise.all([...this.#peers].map(async ([userUid, peer]) => {
      if (peer.connection.connectionState === 'closed') return;
      try {
        if (peer.videoSender) await peer.videoSender.replaceTrack(track);
        else if (track) {
          peer.videoSender = peer.connection.addTrack(track, stream);
          peer.negotiationPending = true;
          void this.#negotiate(userUid, peer);
        }
      } catch {
        // A sender can become unusable after an ICE restart or a suspended
        // display capture. Recreate only that sender and renegotiate instead
        // of leaving every participant on a frozen last frame.
        if (peer.videoSender) {
          try { peer.connection.removeTrack(peer.videoSender); } catch { /* Already detached. */ }
          peer.videoSender = null;
        }
        if (track) peer.videoSender = peer.connection.addTrack(track, stream);
        peer.negotiationPending = true;
        void this.#negotiate(userUid, peer);
      }
    }));
  }

  async #replaceOutgoingScreenAudio(track: MediaStreamTrack | null, stream: MediaStream): Promise<void> {
    await Promise.all([...this.#peers].map(async ([userUid, peer]) => {
      if (peer.connection.connectionState === 'closed') return;
      try {
        if (peer.screenAudioSender) await peer.screenAudioSender.replaceTrack(track);
        else if (track) {
          peer.screenAudioSender = peer.connection.addTrack(track, stream);
          peer.negotiationPending = true;
          void this.#negotiate(userUid, peer);
        }
      } catch {
        if (peer.screenAudioSender) {
          try { peer.connection.removeTrack(peer.screenAudioSender); } catch { /* Already detached. */ }
          peer.screenAudioSender = null;
        }
        if (track) peer.screenAudioSender = peer.connection.addTrack(track, stream);
        peer.negotiationPending = true;
        void this.#negotiate(userUid, peer);
      }
    }));
  }

  async #addRemoteCandidate(userUid: string, state: PeerState, candidate: RTCIceCandidateInit): Promise<void> {
    if (this.#closed || state.ignoreOffer || !candidateMatchesRemoteDescription(candidate, state.connection.remoteDescription)) return;
    try {
      await state.connection.addIceCandidate(candidate);
    } catch {
      // ICE candidates can arrive after a glare rollback or ICE restart. They
      // are advisory; one obsolete candidate must not fail an otherwise valid
      // call. A genuinely failed connection is recovered by the existing
      // connection-state handler and a fresh negotiation.
      if (state.connection.connectionState === 'failed') {
        state.connection.restartIce();
        state.negotiationPending = true;
        void this.#negotiate(userUid, state);
      }
    }
  }

  #signalDescription(userUid: string, description: RTCSessionDescription): Promise<unknown> {
    const peer = this.#peers.get(userUid);
    if (!peer) return Promise.resolve();
    return this.sendSignal(userUid, peer.sessionUid, description.type as CallSignalKind, description.toJSON());
  }

  #emitRemote(): void {
    this.onRemoteChanged([...this.#peers.entries()].map(([userUid, peer]) => ({
      userUid,
      stream: peer.remoteStream,
      connectionState: peer.connection.connectionState
    })));
  }

  #emitLocal(): void {
    this.onLocalChanged(this.localState());
  }
}
