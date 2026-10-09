import { afterEach, describe, expect, it, vi } from 'vitest';
import { CallController, callMediaConstraints, callPermissionMessage, candidateMatchesRemoteDescription, isCallSignal, screenShareConstraints, type RemoteCallMedia } from './controller';

class FakeTrack {
  enabled = true;
  muted = false;
  stopped = false;
  onended: (() => void) | null = null;

  constructor(public readonly kind: 'audio' | 'video', public readonly id: string) {}
  get readyState() { return this.stopped ? 'ended' : 'live'; }
  stop() { this.stopped = true; }
}

class FakeStream {
  constructor(private tracks: FakeTrack[] = []) {}
  addTrack(track: FakeTrack) { if (!this.tracks.includes(track)) this.tracks.push(track); }
  removeTrack(track: FakeTrack) { this.tracks = this.tracks.filter((item) => item !== track); }
  getTracks() { return [...this.tracks]; }
  getAudioTracks() { return this.tracks.filter((track) => track.kind === 'audio'); }
  getVideoTracks() { return this.tracks.filter((track) => track.kind === 'video'); }
}

class FakeSender {
  constructor(public track: FakeTrack | null) {}
  async replaceTrack(track: FakeTrack | null) { this.track = track; }
}

class FakePeerConnection {
  static instances: FakePeerConnection[] = [];
  connectionState = 'new';
  signalingState = 'stable';
  localDescription: RTCSessionDescriptionInit | null = null;
  remoteDescription: RTCSessionDescriptionInit | null = null;
  closed = false;
  rejectCandidates = false;
  addedCandidates: RTCIceCandidateInit[] = [];
  onicecandidate: ((event: { candidate: { toJSON: () => RTCIceCandidateInit } | null }) => void) | null = null;
  ontrack: ((event: unknown) => void) | null = null;
  onconnectionstatechange: (() => void) | null = null;
  onnegotiationneeded: (() => void) | null = null;
  private senders: FakeSender[] = [];

  constructor() { FakePeerConnection.instances.push(this); }
  addTrack(track: FakeTrack) { const sender = new FakeSender(track); this.senders.push(sender); return sender; }
  removeTrack(sender: FakeSender) { this.senders = this.senders.filter((item) => item !== sender); }
  getSenders() { return [...this.senders]; }
  async createOffer() { return { type: 'offer' as const, sdp: 'fresh-offer' }; }
  async createAnswer() { return { type: 'answer' as const, sdp: 'fresh-answer' }; }
  async setLocalDescription(description?: RTCSessionDescriptionInit) {
    const resolved = description || (this.signalingState === 'have-remote-offer'
      ? { type: 'answer' as const, sdp: 'fresh-answer' }
      : { type: 'offer' as const, sdp: 'fresh-offer' });
    this.localDescription = { ...resolved, toJSON: () => ({ ...resolved }) } as RTCSessionDescriptionInit;
    this.signalingState = resolved.type === 'offer' ? 'have-local-offer' : 'stable';
  }
  async setRemoteDescription(description: RTCSessionDescriptionInit) { this.remoteDescription = description; this.signalingState = description.type === 'offer' ? 'have-remote-offer' : 'stable'; }
  async addIceCandidate(candidate: RTCIceCandidateInit) {
    if (this.rejectCandidates) throw new DOMException('Error processing ICE candidate', 'OperationError');
    this.addedCandidates.push(candidate);
  }
  restartIce() {}
  close() { this.closed = true; this.connectionState = 'closed'; }
}

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('loss-tolerant call negotiation', () => {
  it('replaces a stalled pair after ICE recovery fails, preserving capture and rejecting obsolete transport messages', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: async () => new FakeStream([new FakeTrack('audio', 'mic'), new FakeTrack('video', 'camera')]) } });
    FakePeerConnection.instances = [];
    const send = vi.fn(async (_uid: string, _session: string, _kind: string, _data: object) => undefined);
    const controller = new CallController('a', 'a-session', [], send, () => undefined, () => undefined, () => undefined, () => undefined);
    const signal = (kind: 'answer' | 'ice', data: Record<string, unknown>) => controller.handleSignal({ senderUid: 'z', senderSessionUid: 'z-session', recipientSessionUid: 'a-session', kind, data });
    try {
      await controller.start('video');
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
      await vi.advanceTimersByTimeAsync(1);
      await signal('answer', { type: 'answer', sdp: 'first answer', transport_revision: 1 });
      const first = FakePeerConnection.instances[0]; first.connectionState = 'connected';
      const remote = new FakeTrack('audio', 'remote'); remote.muted = true; first.ontrack?.({ track: remote });
      await vi.advanceTimersByTimeAsync(16000);
      const restartOffer = send.mock.calls.filter((call) => call[2] === 'offer').at(-1)![3] as Record<string, unknown>;
      await signal('answer', { type: 'answer', sdp: 'restart answer', transport_revision: 1, negotiation_uid: restartOffer.negotiation_uid });
      await vi.advanceTimersByTimeAsync(15000);
      expect(FakePeerConnection.instances).toHaveLength(2);
      const second = FakePeerConnection.instances[1];
      expect(first.closed).toBe(true); expect(second.closed).toBe(false);
      expect(second.getSenders().map((sender) => sender.track?.id)).toEqual(['mic', 'camera']);
      expect(controller.localState().stream.getTracks().every((track) => track.readyState === 'live')).toBe(true);
      const offer = send.mock.calls.filter((call) => call[2] === 'offer').at(-1)![3] as Record<string, unknown>;
      expect(offer.transport_revision).toBe(2);
      await signal('answer', { type: 'answer', sdp: 'old answer', transport_revision: 1 });
      await signal('ice', { candidate: 'old candidate', transport_revision: 1 });
      expect(second.remoteDescription).toBeNull(); expect(second.addedCandidates).toHaveLength(0);
      await signal('answer', { type: 'answer', sdp: 'fresh answer', transport_revision: 2, negotiation_uid: offer.negotiation_uid });
      await signal('ice', { candidate: 'fresh candidate', transport_revision: 2 });
      expect(second.remoteDescription?.sdp).toBe('fresh answer'); expect(second.addedCandidates).toHaveLength(1);
    } finally { controller.close(); }
  });

  it('follows a newer transport offer but never resets a pair on an early ICE or delayed old offer', async () => {
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const send = vi.fn(async () => undefined);
    const controller = new CallController('z', 'z-session', [], send, () => undefined, () => undefined, () => undefined, () => undefined);
    const signal = (kind: 'offer' | 'ice', revision: number) => controller.handleSignal({ senderUid: 'a', senderSessionUid: 'a-session', recipientSessionUid: 'z-session', kind, data: { type: kind, sdp: `offer-${revision}`, candidate: 'candidate', transport_revision: revision } });
    try {
      controller.connectToExisting([{ userUid: 'a', sessionUid: 'a-session' }]);
      await signal('ice', 2); expect(FakePeerConnection.instances).toHaveLength(1);
      await signal('offer', 2); expect(FakePeerConnection.instances).toHaveLength(2);
      expect(FakePeerConnection.instances[0].closed).toBe(true);
      expect(send).toHaveBeenCalledWith('a', 'a-session', 'answer', expect.objectContaining({ transport_revision: 2 }));
      await signal('offer', 1); await signal('offer', -1); await signal('offer', NaN);
      expect(FakePeerConnection.instances).toHaveLength(2);
      expect(FakePeerConnection.instances[1].remoteDescription?.sdp).toBe('offer-2');
    } finally { controller.close(); }
  });

  it('restarts a connected transport with no incoming media, but leaves flowing or ended tracks alone', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const controller = new CallController('a', 'a-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    try {
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
      await vi.advanceTimersByTimeAsync(1);
      await controller.handleSignal({ senderUid: 'z', senderSessionUid: 'z-session', recipientSessionUid: 'a-session', kind: 'answer', data: { type: 'answer', sdp: 'answer' } });
      const peer = FakePeerConnection.instances[0]; const restart = vi.spyOn(peer, 'restartIce');
      peer.connectionState = 'connected';
      const track = new FakeTrack('audio', 'remote'); track.muted = true;
      peer.ontrack?.({ track });
      await vi.advanceTimersByTimeAsync(16000);
      expect(restart).toHaveBeenCalledTimes(1);
      track.muted = false;
      await vi.advanceTimersByTimeAsync(60000);
      expect(restart).toHaveBeenCalledTimes(1);
      track.muted = true; track.stop();
      await vi.advanceTimersByTimeAsync(60000);
      expect(restart).toHaveBeenCalledTimes(1);
    } finally { controller.close(); }
  });
  it('attaches media to peers discovered while the capture permission request is pending', async () => {
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    let permit!: (stream: FakeStream) => void;
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: () => new Promise<FakeStream>((resolve) => permit = resolve) } });
    const controller = new CallController('z', 'z-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    try {
      const starting = controller.start('video');
      controller.connectToExisting([{ userUid: 'a', sessionUid: 'a-session' }]);
      permit(new FakeStream([new FakeTrack('audio', 'mic'), new FakeTrack('video', 'camera')]));
      await starting;
      expect(FakePeerConnection.instances[0].getSenders().map((sender) => sender.track?.id).sort()).toEqual(['camera', 'mic']);
    } finally { controller.close(); }
  });
  it('replays current ICE after SDP is acknowledged, stops when connected and clears timers on close', async () => {
    vi.useFakeTimers();
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const send = vi.fn(async () => undefined);
    const controller = new CallController('a', 'a-session', [], send, () => undefined, () => undefined, () => undefined, () => undefined);
    try {
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
      await vi.advanceTimersByTimeAsync(1);
      await controller.handleSignal({ senderUid: 'z', senderSessionUid: 'z-session', recipientSessionUid: 'a-session', kind: 'answer', data: { type: 'answer', sdp: 'answer' } });
      const peer = FakePeerConnection.instances[0];
      const restart = vi.spyOn(peer, 'restartIce');
      const candidate = { candidate: 'candidate:current' };
      peer.onicecandidate?.({ candidate: { toJSON: () => candidate } });
      send.mockClear();
      await vi.advanceTimersByTimeAsync(4000);
      expect(send).toHaveBeenCalledWith('z', 'z-session', 'ice', { ...candidate, transport_revision: 1 });
      expect(restart).not.toHaveBeenCalled();
      peer.connectionState = 'connected'; send.mockClear();
      await vi.advanceTimersByTimeAsync(10000);
      expect(send).not.toHaveBeenCalled();
    } finally { controller.close(); }
    expect(vi.getTimerCount()).toBe(0);
  });
  for (const fault of ['offer-http-failure', 'lost-offer', 'lost-answer'] as const) {
    it(`recovers ${fault} without rebuilding the peer or losing local media`, async () => {
      vi.useFakeTimers();
      FakePeerConnection.instances = [];
      vi.stubGlobal('MediaStream', FakeStream);
      vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
      vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: async () => new FakeStream([new FakeTrack('audio', 'mic')]) } });
      let dropped = false;
      const errors: string[] = [];
      const signals: Array<{ kind: string; data: object }> = [];
      let a!: CallController; let z!: CallController;
      const relay = (sender: string, recipient: () => CallController) => async (_uid: string, sessionUid: string, kind: string, data: object) => {
        signals.push({ kind, data });
        if (!dropped && ((kind === 'offer' && fault !== 'lost-answer') || (kind === 'answer' && fault === 'lost-answer'))) {
          dropped = true;
          if (fault === 'offer-http-failure') throw new TypeError('connection reset');
          return;
        }
        await recipient().handleSignal({ senderUid: sender, senderSessionUid: `${sender}-session`, recipientSessionUid: sessionUid, kind: kind as 'offer' | 'answer' | 'ice', data: data as Record<string, unknown> });
      };
      a = new CallController('a', 'a-session', [], relay('a', () => z), () => undefined, () => undefined, (error) => errors.push(error), () => undefined);
      z = new CallController('z', 'z-session', [], relay('z', () => a), () => undefined, () => undefined, (error) => errors.push(error), () => undefined);
      try {
        await a.start('voice'); await z.start('voice');
        a.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
        z.connectToExisting([{ userUid: 'a', sessionUid: 'a-session' }]);
        await vi.advanceTimersByTimeAsync(5000);
        expect(dropped).toBe(true);
        expect(FakePeerConnection.instances).toHaveLength(2);
        expect(FakePeerConnection.instances.every((pc) => pc.signalingState === 'stable')).toBe(true);
        const offers = signals.filter((signal) => signal.kind === 'offer');
        expect(offers).toHaveLength(2);
        expect(offers[0].data).toEqual(offers[1].data);
        expect(errors).toEqual([]);
        expect(a.localState().microphoneEnabled).toBe(true);
      } finally { a.close(); z.close(); }
      await vi.advanceTimersByTimeAsync(60000);
      expect(FakePeerConnection.instances).toHaveLength(2);
    });
  }

  it('ignores delayed signals from a retired sender session after rejoin', async () => {
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const controller = new CallController('a', 'a-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    try {
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'old' }]);
      await Promise.resolve();
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'new' }]);
      await controller.handleSignal({ senderUid: 'z', senderSessionUid: 'old', recipientSessionUid: 'a-session', kind: 'offer', data: { type: 'offer', sdp: 'obsolete' } });
      expect(FakePeerConnection.instances).toHaveLength(2);
      expect(FakePeerConnection.instances[1].closed).toBe(false);
    } finally { controller.close(); }
  });
  it('retires an explicitly departed peer before rejoin so delayed SDP and discovery cannot replace it', async () => {
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const errors: string[] = [];
    const controller = new CallController('a', 'a-session', [], async () => undefined, () => undefined, () => undefined, (message) => errors.push(message), () => undefined);
    try {
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'old' }]);
      await Promise.resolve();
      controller.removePeer('z');
      await controller.handleSignal({ senderUid: 'z', senderSessionUid: 'old', recipientSessionUid: 'a-session', kind: 'offer', data: { type: 'offer', sdp: 'obsolete' } });
      expect(FakePeerConnection.instances).toHaveLength(1);
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'new' }]);
      controller.connectToExisting([{ userUid: 'z', sessionUid: 'old' }]);
      await controller.handleSignal({ senderUid: 'z', senderSessionUid: 'old', recipientSessionUid: 'a-session', kind: 'offer', data: { type: 'offer', sdp: 'obsolete' } });
      expect(FakePeerConnection.instances).toHaveLength(2);
      expect(FakePeerConnection.instances[1].closed).toBe(false);
      expect(errors).toEqual([]);
    } finally { controller.close(); }
  });
  it('allows server-confirmed discovery to revive an expired lease with the same session', async () => {
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    FakePeerConnection.instances = [];
    const controller = new CallController('z', 'z-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    try {
      const participant = { userUid: 'a', sessionUid: 'lease-session' };
      controller.connectToExisting([participant]);
      controller.removePeer('a');
      await controller.handleSignal({ senderUid: 'a', senderSessionUid: participant.sessionUid, recipientSessionUid: 'z-session', kind: 'ice', data: { candidate: 'late' } });
      expect(FakePeerConnection.instances).toHaveLength(1);
      controller.connectToExisting([participant]);
      await controller.handleSignal({ senderUid: 'a', senderSessionUid: participant.sessionUid, recipientSessionUid: 'z-session', kind: 'offer', data: { type: 'offer', sdp: 'fresh-after-lease' } });
      expect(FakePeerConnection.instances).toHaveLength(2);
      expect(FakePeerConnection.instances[1].remoteDescription?.sdp).toBe('fresh-after-lease');
    } finally { controller.close(); }
  });
});

describe('call media policy', () => {
  it('requests the capture picker to exclude the call tab without disabling shared audio', () => {
    expect(screenShareConstraints()).toMatchObject({ selfBrowserSurface: 'exclude', preferCurrentTab: false, audio: true });
  });
  it('requests processed audio for every call and video only for video calls', () => {
    expect(callMediaConstraints('voice').audio).toMatchObject({ echoCancellation: true, noiseSuppression: true });
    expect(callMediaConstraints('voice').video).toBe(false);
    expect(callMediaConstraints('video').video).toMatchObject({ width: { ideal: 1280 }, frameRate: { max: 30 } });
  });

  it('returns actionable device permission errors', () => {
    expect(callPermissionMessage(new DOMException('blocked', 'NotAllowedError'))).toContain('permission was denied');
    expect(callPermissionMessage(new DOMException('missing', 'NotFoundError'))).toContain('No usable camera or microphone');
  });
});

describe('call signaling contract', () => {
  it('accepts only addressed WebRTC signal objects', () => {
    expect(isCallSignal({ senderUid: 'user-1', senderSessionUid: 'sender-1', recipientSessionUid: 'self-1', kind: 'offer', data: { type: 'offer', sdp: 'v=0' } })).toBe(true);
    expect(isCallSignal({ senderUid: 'user-1', senderSessionUid: 'sender-1', recipientSessionUid: 'self-1', kind: 'ice', data: { candidate: 'candidate' } })).toBe(true);
    expect(isCallSignal({ senderUid: '', kind: 'hangup', data: {} })).toBe(false);
    expect(isCallSignal(null)).toBe(false);
  });

  it('matches ICE candidates only to the remote description that created them', () => {
    const description = { type: 'offer' as const, sdp: 'v=0\r\na=ice-ufrag:fresh\r\n' };
    expect(candidateMatchesRemoteDescription({ candidate: 'candidate:fresh', usernameFragment: 'fresh' }, description)).toBe(true);
    expect(candidateMatchesRemoteDescription({ candidate: 'candidate:stale', usernameFragment: 'stale' }, description)).toBe(false);
    expect(candidateMatchesRemoteDescription({ candidate: 'candidate:legacy' }, description)).toBe(true);
  });

  it('drops a rejected late ICE candidate without failing the active call', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', { mediaDevices: { getUserMedia: vi.fn(async () => new FakeStream([microphone])) } });
    const errors: string[] = [];
    const controller = new CallController(
      'self', 'self-session', [], async () => undefined, () => undefined, () => undefined,
      (message) => errors.push(message), () => undefined
    );

    await controller.start('voice');
    await controller.handleSignal({
      senderUid: 'remote', senderSessionUid: 'remote-session', recipientSessionUid: 'self-session',
      kind: 'offer', data: { type: 'offer', sdp: 'v=0\r\na=ice-ufrag:fresh\r\n' }
    });
    const peer = FakePeerConnection.instances[0];
    peer.rejectCandidates = true;
    await controller.handleSignal({
      senderUid: 'remote', senderSessionUid: 'remote-session', recipientSessionUid: 'self-session',
      kind: 'ice', data: { candidate: 'candidate:fresh', usernameFragment: 'fresh' }
    });
    await controller.handleSignal({
      senderUid: 'remote', senderSessionUid: 'remote-session', recipientSessionUid: 'self-session',
      kind: 'ice', data: { candidate: 'candidate:stale', usernameFragment: 'stale' }
    });

    expect(errors).toEqual([]);
    expect(peer.addedCandidates).toEqual([]);
    controller.close();
  });

  it('assigns exactly one initial offerer so simultaneous peer discovery does not create glare', async () => {
    const microphoneLow = new FakeTrack('audio', 'microphone-low');
    const microphoneHigh = new FakeTrack('audio', 'microphone-high');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn()
          .mockResolvedValueOnce(new FakeStream([microphoneLow]))
          .mockResolvedValueOnce(new FakeStream([microphoneHigh]))
      }
    });
    const lowSignals: string[] = [];
    const highSignals: string[] = [];
    const low = new CallController('a-user', 'a-session', [], async (_uid, _session, kind) => { lowSignals.push(kind); }, () => undefined, () => undefined, () => undefined, () => undefined);
    const high = new CallController('z-user', 'z-session', [], async (_uid, _session, kind) => { highSignals.push(kind); }, () => undefined, () => undefined, () => undefined, () => undefined);
    await low.start('voice');
    await high.start('voice');

    low.connectToExisting([{ userUid: 'z-user', sessionUid: 'z-session' }]);
    high.connectToExisting([{ userUid: 'a-user', sessionUid: 'a-session' }]);
    await vi.waitFor(() => expect(lowSignals).toEqual(['offer']));
    expect(highSignals).toEqual([]);
    low.close();
    high.close();
  });
});

describe('call device lifecycle', () => {
  it('mutes locally, acquires camera on demand, and stops every device on close', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    const camera = new FakeTrack('video', 'camera');
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async (constraints: MediaStreamConstraints) => (
          constraints.audio ? new FakeStream([microphone]) : new FakeStream([camera])
        ))
      }
    });
    const localStates: Array<{ microphoneEnabled: boolean; cameraEnabled: boolean }> = [];
    const controller = new CallController('self', 'self-session', [], async () => undefined, () => undefined, (state) => {
      localStates.push({ microphoneEnabled: state.microphoneEnabled, cameraEnabled: state.cameraEnabled });
    }, () => undefined, () => undefined);

    await controller.start('voice');
    controller.setMicrophoneEnabled(false);
    await controller.setCameraEnabled(true);
    expect(controller.localState()).toMatchObject({ microphoneEnabled: false, cameraEnabled: true, screenSharing: false });
    await controller.setCameraEnabled(false);
    expect(camera.stopped).toBe(true);
    controller.close();
    expect(microphone.stopped).toBe(true);
    expect(localStates.some((state) => !state.microphoneEnabled && state.cameraEnabled)).toBe(true);
  });

  it('replaces a stale peer on rejoin and sends microphone, camera, and screen media to the fresh session', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    const camera = new FakeTrack('video', 'camera');
    const screen = new FakeTrack('video', 'screen');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async () => new FakeStream([microphone, camera])),
        getDisplayMedia: vi.fn(async () => new FakeStream([screen]))
      }
    });
    const signals: Array<{ recipientUid: string; recipientSessionUid: string; kind: string }> = [];
    const controller = new CallController(
      'a-self',
      'self-session-new',
      [],
      async (recipientUid, recipientSessionUid, kind) => { signals.push({ recipientUid, recipientSessionUid, kind }); },
      () => undefined,
      () => undefined,
      () => undefined,
      () => undefined
    );

    await controller.start('video');
    await controller.startScreenShare();
    controller.connectToExisting([{ userUid: 'z-remote', sessionUid: 'remote-session-old' }]);
    await vi.waitFor(() => expect(signals.at(-1)).toMatchObject({ recipientSessionUid: 'remote-session-old', kind: 'offer' }));
    const oldPeer = FakePeerConnection.instances[0];

    controller.connectToExisting([{ userUid: 'z-remote', sessionUid: 'remote-session-new' }]);
    await vi.waitFor(() => expect(signals.at(-1)).toMatchObject({ recipientSessionUid: 'remote-session-new', kind: 'offer' }));
    const freshPeer = FakePeerConnection.instances[1];
    expect(oldPeer.closed).toBe(true);
    expect(freshPeer.getSenders().map((sender) => sender.track?.id).sort()).toEqual(['microphone', 'screen']);

    await controller.handleSignal({
      senderUid: 'z-remote', senderSessionUid: 'remote-session-old', recipientSessionUid: 'self-session-old',
      kind: 'offer', data: { type: 'offer', sdp: 'stale-offer' }
    });
    expect(FakePeerConnection.instances).toHaveLength(2);

    await controller.stopScreenShare();
    expect(freshPeer.getSenders().map((sender) => sender.track?.id).sort()).toEqual(['camera', 'microphone']);
    controller.close();
  });

  it('acquires and releases a display track without pretending the camera is active', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    const screen = new FakeTrack('video', 'screen');
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async () => new FakeStream([microphone])),
        getDisplayMedia: vi.fn(async () => new FakeStream([screen]))
      }
    });
    const controller = new CallController('self', 'self-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);

    await controller.start('voice');
    await controller.startScreenShare();
    expect(controller.localState()).toMatchObject({ cameraEnabled: false, screenSharing: true });
    await controller.stopScreenShare();
    expect(screen.stopped).toBe(true);
    expect(controller.localState()).toMatchObject({ cameraEnabled: false, screenSharing: false });
    controller.close();
  });

  it('keeps microphone audio independent while starting and stopping shared-screen audio', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    const camera = new FakeTrack('video', 'camera');
    const screen = new FakeTrack('video', 'screen');
    const screenAudio = new FakeTrack('audio', 'screen-audio');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    const getDisplayMedia = vi.fn(async () => new FakeStream([screen, screenAudio]));
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async () => new FakeStream([microphone, camera])),
        getDisplayMedia
      }
    });
    const controller = new CallController('a-self', 'self-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    await controller.start('video');
    controller.connectToExisting([{ userUid: 'z-remote', sessionUid: 'remote-session' }]);
    const peer = FakePeerConnection.instances[0];
    const microphoneSender = peer.getSenders().find((sender) => sender.track === microphone);

    await controller.startScreenShare();
    expect(getDisplayMedia).toHaveBeenCalledWith(expect.objectContaining({ audio: true }));
    const screenAudioSender = peer.getSenders().find((sender) => sender.track === screenAudio);
    expect(microphoneSender?.track).toBe(microphone);
    expect(screenAudioSender?.track).toBe(screenAudio);
    controller.setMicrophoneEnabled(false);
    expect(microphoneSender?.track?.enabled).toBe(false);
    expect(screenAudioSender?.track?.enabled).toBe(true);
    controller.setMicrophoneEnabled(true);

    await controller.stopScreenShare();
    expect(screenAudio.stopped).toBe(true);
    expect(screenAudioSender?.track).toBeNull();
    expect(microphoneSender?.track).toBe(microphone);
    expect(microphoneSender?.track?.enabled).toBe(true);
    controller.close();
  });

  it('reliably restores live camera media after repeated screen-share switches', async () => {
    const microphone = new FakeTrack('audio', 'microphone');
    const camera = new FakeTrack('video', 'camera');
    const firstScreen = new FakeTrack('video', 'screen-1');
    const secondScreen = new FakeTrack('video', 'screen-2');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async () => new FakeStream([microphone, camera])),
        getDisplayMedia: vi.fn()
          .mockResolvedValueOnce(new FakeStream([firstScreen]))
          .mockResolvedValueOnce(new FakeStream([secondScreen]))
      }
    });
    const controller = new CallController('a-self', 'self-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);
    await controller.start('video');
    controller.connectToExisting([{ userUid: 'z-remote', sessionUid: 'remote-session' }]);
    const peer = FakePeerConnection.instances[0];
    const audioSender = peer.getSenders().find((sender) => sender.track?.kind === 'audio');
    expect(audioSender?.track).toBe(microphone);

    await controller.startScreenShare();
    expect(peer.getSenders().find((sender) => sender.track?.kind === 'video')?.track?.id).toBe('screen-1');
    controller.setMicrophoneEnabled(false);
    expect(audioSender?.track).toBe(microphone);
    expect(audioSender?.track?.enabled).toBe(false);
    controller.setMicrophoneEnabled(true);
    expect(audioSender?.track?.enabled).toBe(true);
    await controller.stopScreenShare();
    expect(peer.getSenders().find((sender) => sender.track?.kind === 'video')?.track?.id).toBe('camera');
    await controller.startScreenShare();
    expect(peer.getSenders().find((sender) => sender.track?.kind === 'video')?.track?.id).toBe('screen-2');
    await controller.stopScreenShare();
    expect(peer.getSenders().find((sender) => sender.track?.kind === 'video')?.track?.id).toBe('camera');
    expect(peer.getSenders().filter((sender) => sender.track?.kind === 'audio')).toEqual([audioSender]);
    expect(audioSender?.track).toBe(microphone);
    expect(audioSender?.track?.enabled).toBe(true);
    expect(controller.localState()).toMatchObject({ cameraEnabled: true, screenSharing: false });
    controller.close();
  });

  it('switches microphone and camera devices without leaving the old hardware active', async () => {
    const firstMicrophone = new FakeTrack('audio', 'microphone-1');
    const secondMicrophone = new FakeTrack('audio', 'microphone-2');
    const firstCamera = new FakeTrack('video', 'camera-1');
    const secondCamera = new FakeTrack('video', 'camera-2');
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: vi.fn(async (constraints: MediaStreamConstraints) => {
          if (constraints.audio && constraints.video) return new FakeStream([firstMicrophone, firstCamera]);
          if (constraints.audio) return new FakeStream([secondMicrophone]);
          return new FakeStream([secondCamera]);
        })
      }
    });
    const controller = new CallController('self', 'self-session', [], async () => undefined, () => undefined, () => undefined, () => undefined, () => undefined);

    await controller.start('video');
    await controller.selectMicrophoneDevice('microphone-2');
    await controller.selectCameraDevice('camera-2');
    expect(firstMicrophone.stopped).toBe(true);
    expect(firstCamera.stopped).toBe(true);
    expect(controller.localState()).toMatchObject({ microphoneEnabled: true, cameraEnabled: true });
    controller.close();
    expect(secondMicrophone.stopped).toBe(true);
    expect(secondCamera.stopped).toBe(true);
  });
});

describe('group screen sharing', () => {
  function setup(mode: 'voice' | 'video' = 'voice') {
    const microphone = new FakeTrack('audio', 'microphone');
    const camera = new FakeTrack('video', 'camera');
    const screen = new FakeTrack('video', 'screen');
    FakePeerConnection.instances = [];
    vi.stubGlobal('MediaStream', FakeStream);
    vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
    vi.stubGlobal('navigator', { mediaDevices: {
      getUserMedia: vi.fn(async () => new FakeStream(mode === 'video' ? [microphone, camera] : [microphone])),
      getDisplayMedia: vi.fn(async () => new FakeStream([screen]))
    } });
    const signals: Array<{ uid: string; kind: string }> = [];
    const remoteUpdates: RemoteCallMedia[][] = [];
    const errors: string[] = [];
    const controller = new CallController('a', 'a-session', [], async (uid, _session, kind) => { signals.push({ uid, kind }); },
      (peers) => remoteUpdates.push(peers), () => undefined, (error) => errors.push(error), () => undefined);
    const answer = (uid: string) => controller.handleSignal({ senderUid: uid, senderSessionUid: `${uid}-session`,
      recipientSessionUid: 'a-session', kind: 'answer', data: { type: 'answer', sdp: 'answer' } });
    return { controller, signals, remoteUpdates, errors, answer, screen };
  }

  it('negotiates the first screen video to both voice-only viewers without a heartbeat', async () => {
    const { controller, signals, answer, errors } = setup();
    await controller.start('voice');
    controller.connectToExisting([{ userUid: 'm', sessionUid: 'm-session' }, { userUid: 'z', sessionUid: 'z-session' }]);
    await vi.waitFor(() => expect(signals).toHaveLength(2));
    await answer('m');
    await answer('z');
    signals.length = 0;

    await controller.startScreenShare();
    await vi.waitFor(() => expect(signals.filter((signal) => signal.kind === 'offer').map((signal) => signal.uid).sort()).toEqual(['m', 'z']));
    expect(errors).toEqual([]);
    controller.close();
  });

  it('retains a screen negotiation requested while an earlier offer awaits its answer', async () => {
    const { controller, signals, answer, errors } = setup();
    await controller.start('voice');
    controller.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
    await vi.waitFor(() => expect(signals).toHaveLength(1));
    await controller.startScreenShare();
    expect(signals).toHaveLength(1);
    await answer('z');
    await vi.waitFor(() => expect(signals).toHaveLength(2));
    expect(errors).toEqual([]);
    controller.close();
  });

  it('does not renegotiate healthy connections on repeated participant refreshes', async () => {
    const { controller, signals, answer } = setup();
    await controller.start('voice');
    const peers = [{ userUid: 'm', sessionUid: 'm-session' }, { userUid: 'z', sessionUid: 'z-session' }];
    controller.connectToExisting(peers);
    await vi.waitFor(() => expect(signals).toHaveLength(2));
    await answer('m');
    await answer('z');
    signals.length = 0;
    for (let refresh = 0; refresh < 20; refresh++) controller.connectToExisting(peers);
    await Promise.resolve();
    expect(signals).toEqual([]);
    controller.close();
  });

  it('switches a healthy viewer while another viewer has a pending replaceTrack operation', async () => {
    const { controller, answer, screen } = setup('video');
    await controller.start('video');
    controller.connectToExisting([{ userUid: 'm', sessionUid: 'm-session' }, { userUid: 'z', sessionUid: 'z-session' }]);
    await vi.waitFor(() => expect(FakePeerConnection.instances[0].signalingState).toBe('have-local-offer'));
    await answer('m');
    await answer('z');
    const [slow, healthy] = FakePeerConnection.instances.map((peer) => peer.getSenders().find((sender) => sender.track?.kind === 'video')!);
    let release!: () => void;
    vi.spyOn(slow, 'replaceTrack').mockImplementation(async (track) => {
      await new Promise<void>((resolve) => release = resolve);
      slow.track = track;
    });
    const sharing = controller.startScreenShare();
    await vi.waitFor(() => expect(release).toBeTypeOf('function'));
    try { expect(healthy.track).toBe(screen); }
    finally { release(); await sharing; controller.close(); }
  });

  it('publishes a new stream when video arrives after screen state, and retires replaced video', async () => {
    const { controller, remoteUpdates } = setup();
    await controller.start('voice');
    controller.connectToExisting([{ userUid: 'z', sessionUid: 'z-session' }]);
    const peer = FakePeerConnection.instances[0];
    const voice = new FakeTrack('audio', 'remote-mic');
    peer.ontrack?.({ track: voice, streams: [new FakeStream([voice])] });
    const voiceStream = remoteUpdates.at(-1)![0].stream;
    const screen = new FakeTrack('video', 'remote-screen');
    peer.ontrack?.({ track: screen, streams: [new FakeStream([screen])] });
    const screenStream = remoteUpdates.at(-1)![0].stream;
    expect(screenStream).not.toBe(voiceStream);
    expect(screenStream.getTracks()).toEqual([voice, screen]);
    const replacement = new FakeTrack('video', 'replacement-screen');
    peer.ontrack?.({ track: replacement, streams: [new FakeStream([replacement])] });
    expect(remoteUpdates.at(-1)![0].stream.getVideoTracks()).toEqual([replacement]);
    screen.onended?.();
    expect(remoteUpdates.at(-1)![0].stream.getVideoTracks()).toEqual([replacement]);
    replacement.onended?.();
    expect(remoteUpdates.at(-1)![0].stream.getTracks()).toEqual([voice]);
    controller.close();
  });
});
