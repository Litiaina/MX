<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import AudioLines from '@lucide/svelte/icons/audio-lines';
  import Camera from '@lucide/svelte/icons/camera';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import HeadphoneOff from '@lucide/svelte/icons/headphone-off';
  import Headphones from '@lucide/svelte/icons/headphones';
  import Mic from '@lucide/svelte/icons/mic';
  import MicOff from '@lucide/svelte/icons/mic-off';
  import Minimize2 from '@lucide/svelte/icons/minimize-2';
  import MonitorUp from '@lucide/svelte/icons/monitor-up';
  import PhoneOff from '@lucide/svelte/icons/phone-off';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Video from '@lucide/svelte/icons/video';
  import VideoOff from '@lucide/svelte/icons/video-off';
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import type { CallMode, CallParticipant, CallState } from '../api/domain';
  import type { Session } from '../api/types';
  import { getCallState, joinCall, leaveCall, sendCallSignal, updateCallParticipant } from '../api/workspace';
  import { CallController, callPermissionMessage, type IncomingCallSignal, type LocalCallMedia, type RemoteCallMedia } from '../call/controller';
  import { monitorAudioLevel } from '../call/audioLevel';
  import { callEventKey, callLeaveMatches, callPeerIdentities, callSessionWasReplaced, createCallSessionUid, historicalCallEventKeys, reconcileCallParticipants } from '../call/session';
  import type { LiveMessage } from '../live/client';
  import CallMediaTile from './CallMediaTile.svelte';

  let { session, channelUid, channelName, channelKind = '', initialMode, connected, callEvents = [], onEnded }: {
    session: Session;
    channelUid: string;
    channelName: string;
    channelKind?: string;
    initialMode: CallMode;
    connected: boolean;
    callEvents?: LiveMessage[];
    onEnded: (reason?: 'replaced') => void;
  } = $props();

  let controller: CallController | null = null;
  let participants = $state<CallParticipant[]>([]);
  let remoteMedia = $state<RemoteCallMedia[]>([]);
  let localMedia = $state<LocalCallMedia | null>(null);
  let stage = $state<'starting' | 'active' | 'ending' | 'error'>('starting');
  let error = $state('');
  let minimized = $state(false);
  let expanded = $state(false);
  let maxParticipants = $state(12);
  let focusedUserUid = $state('');
  let deafened = $state(false);
  let restoreMicrophoneAfterDeafen = false;
  let settingsOpen = $state(false);
  let mediaDevices = $state<MediaDeviceInfo[]>([]);
  let microphoneDeviceId = $state('');
  let cameraDeviceId = $state('');
  let outputDeviceId = $state('');
  let callStartedAt = $state(Date.now());
  let clockNow = $state(Date.now());
  let localMicLevel = $state(0);
  let deviceRefreshBusy = $state(false);
  let deviceStatus = $state('');
  let speakerTesting = $state(false);
  let mediaTransitionBusy = $state(false);
  let settingsPreview = $state<HTMLVideoElement>();
  let callPanel = $state<HTMLElement>();
  let dockPosition = $state<{ x: number; y: number } | null>(null);
  let dragState = $state<{ pointerId: number; offsetX: number; offsetY: number } | null>(null);
  let heartbeatTimer: number | undefined;
  let clockTimer: number | undefined;
  let ended = false;
  let heartbeatInFlight = false;
  const callSessionUid = createCallSessionUid();
  // The Workspace retains a rolling event history. A remounted call must not
  // replay the previous session's joined/left events or SDP/ICE messages.
  const handledEvents = untrack(() => historicalCallEventKeys(callEvents));
  const localParticipant = $derived.by(() => {
    const stored = participants.find((item) => item.user_uid === session.uid);
    return {
      user_uid: session.uid,
      session_uid: callSessionUid,
      user_name: stored?.user_name || session.name,
      profile_photo_updated_at: stored?.profile_photo_updated_at ?? session.profile_photo_updated_at,
      audio_enabled: localMedia?.microphoneEnabled ?? stored?.audio_enabled ?? true,
      video_enabled: Boolean(localMedia?.cameraEnabled || localMedia?.screenSharing),
      screen_sharing: localMedia?.screenSharing ?? false,
      joined_at: stored?.joined_at || Date.now()
    } satisfies CallParticipant;
  });
  const remoteParticipants = $derived(participants.filter((item) => item.user_uid !== session.uid));
  const microphones = $derived(mediaDevices.filter((device) => device.kind === 'audioinput'));
  const cameras = $derived(mediaDevices.filter((device) => device.kind === 'videoinput'));
  const audioOutputs = $derived(mediaDevices.filter((device) => device.kind === 'audiooutput'));
  const callDuration = $derived(formatDuration(Math.max(0, clockNow - callStartedAt)));
  const dockStyle = $derived(!expanded && dockPosition ? `left:${dockPosition.x}px;top:${dockPosition.y}px;right:auto;bottom:auto` : '');

  $effect(() => {
    const stream = localMedia?.stream;
    if (!stream || !localMedia?.microphoneEnabled || deafened) {
      localMicLevel = 0;
      return;
    }
    return monitorAudioLevel(stream, (level) => localMicLevel = level);
  });

  $effect(() => {
    if (!settingsPreview) return;
    const previewTrack = localMedia?.screenSharing ? null : localMedia?.stream.getVideoTracks()[0] || null;
    const currentTrack = (settingsPreview.srcObject as MediaStream | null)?.getVideoTracks()[0] || null;
    if (currentTrack === previewTrack) {
      if (previewTrack && settingsPreview.paused) void settingsPreview.play().catch(() => undefined);
      return;
    }
    if (!previewTrack) settingsPreview.pause();
    settingsPreview.srcObject = previewTrack ? new MediaStream([previewTrack]) : null;
    settingsPreview.muted = true;
    if (previewTrack) void settingsPreview.play().catch(() => undefined);
  });

  onMount(() => {
    void begin();
    const leaveOnPageHide = () => { if (!ended) void leaveCall(channelUid, callSessionUid).catch(() => undefined); };
    window.addEventListener('pagehide', leaveOnPageHide);
    return () => { window.removeEventListener('pagehide', leaveOnPageHide); window.clearInterval(heartbeatTimer); window.clearInterval(clockTimer); controller?.close(); if (!ended) void leaveCall(channelUid, callSessionUid).catch(() => undefined); ended = true; };
  });

  $effect(() => {
    for (const event of callEvents) {
      const key = callEventKey(event);
      if (handledEvents.has(key)) continue;
      handledEvents.add(key);
      if (handledEvents.size > 1000) handledEvents.delete(handledEvents.values().next().value!);
      void handleLiveEvent(event);
    }
  });

  $effect(() => {
    const readyAfterConnection = connected && stage === 'active';
    if (!readyAfterConnection) return;
    untrack(() => {
      if (!controller) return;
      void heartbeat();
      controller.connectToExisting(callPeerIdentities(participants, session.uid));
    });
  });

  async function begin() {
    try {
      if (!window.isSecureContext && location.hostname !== 'localhost' && location.hostname !== '127.0.0.1') throw new Error('Voice and video calls require MX to be opened through HTTPS.');
      const available = await getCallState(channelUid);
      if (ended) return;
      controller = new CallController(
        session.uid,
        callSessionUid,
        available.ice_servers,
        (recipientUid, recipientSessionUid, kind, data) => sendCallSignal(channelUid, recipientUid, callSessionUid, recipientSessionUid, kind, data),
        (media) => remoteMedia = media,
        (media) => localMedia = media,
        (message) => error = message,
        () => void screenEnded()
      );
      await controller.start(initialMode);
      if (ended) { controller?.close(); return; }
      await refreshMediaDevices();
      if (ended) return;
      const state = await joinCall(channelUid, initialMode, callSessionUid);
      if (ended) { void leaveCall(channelUid, callSessionUid).catch(() => undefined); return; }
      applyState(state);
      controller.connectToExisting(callPeerIdentities(state.participants, session.uid));
      stage = 'active';
      heartbeatTimer = window.setInterval(() => void heartbeat(), Math.max(8, state.heartbeat_seconds || 15) * 1000);
      clockTimer = window.setInterval(() => clockNow = Date.now(), 1000);
    } catch (reason) {
      if (ended) return;
      controller?.close();
      controller = null;
      error = callPermissionMessage(reason);
      stage = 'error';
    }
  }

  function applyState(state: CallState) {
    if (ended) return;
    participants = reconcileCallParticipants(participants, state.participants);
    maxParticipants = state.max_participants || maxParticipants;
    if (state.started_at) callStartedAt = state.started_at;
    if (!focusedUserUid) focusedUserUid = participants.find((item) => item.screen_sharing && item.user_uid !== session.uid)?.user_uid || '';
    if (callSessionWasReplaced(participants, session.uid, callSessionUid)) { void finish(false, 'replaced'); return; }
    if (stage === 'active' && !participants.some((item) => item.user_uid === session.uid)) { void finish(false); return; }
    // A current state response can discover a viewer whose join event was
    // missed; establish its media connection immediately without re-offering
    // to already connected participants.
    controller?.connectToExisting(callPeerIdentities(participants, session.uid));
  }

  function mergeParticipant(participant: CallParticipant) {
    participants = reconcileCallParticipants(participants, [...participants.filter((item) => item.user_uid !== participant.user_uid), participant]
      .sort((left, right) => left.joined_at - right.joined_at || left.user_name.localeCompare(right.user_name)));
    if (participant.screen_sharing && participant.user_uid !== session.uid) focusedUserUid = participant.user_uid;
    else if (!participant.screen_sharing && focusedUserUid === participant.user_uid) focusedUserUid = '';
  }

  async function handleLiveEvent(event: LiveMessage) {
    if (ended) return;
    if (!event.type.startsWith('call.') || !event.payload || typeof event.payload !== 'object') return;
    const payload = event.payload as Record<string, unknown>;
    if (payload.channel_uid !== channelUid) return;
    if (event.type === 'call.ended') {
      try {
        const current = await getCallState(channelUid);
        const stillJoined = current.participants.some((participant) => participant.user_uid === session.uid && participant.session_uid === callSessionUid);
        if (!current.active || !stillJoined) await finish(false);
        else applyState(current);
      } catch { /* Heartbeat reconciliation will determine whether this session remains active. */ }
      return;
    }
    if (event.type === 'call.signal') {
      const signal: IncomingCallSignal = {
        senderUid: String(payload.sender_uid || event.actor_uid || ''),
        senderSessionUid: String(payload.sender_session_uid || ''),
        recipientSessionUid: String(payload.recipient_session_uid || ''),
        kind: String(payload.kind || '') as IncomingCallSignal['kind'],
        data: (payload.data && typeof payload.data === 'object' ? payload.data : {}) as Record<string, unknown>
      };
      await controller?.handleSignal(signal);
      return;
    }
    if (event.type === 'call.participant.joined' || event.type === 'call.participant.updated') {
      if (payload.participant && typeof payload.participant === 'object') {
        const participant = payload.participant as unknown as CallParticipant;
        if (participant.user_uid === session.uid && participant.session_uid !== callSessionUid) {
          await finish(false, 'replaced');
          return;
        }
        mergeParticipant(participant);
        if (event.type === 'call.participant.joined' && participant.user_uid !== session.uid) {
          controller?.connectToExisting([{ userUid: participant.user_uid, sessionUid: participant.session_uid }]);
        }
      }
      return;
    }
    if (event.type === 'call.participant.left') {
      const userUid = String(payload.user_uid || '');
      const eventSessionUid = String(payload.session_uid || '');
      const participant = participants.find((item) => item.user_uid === userUid);
      if (eventSessionUid && participant?.session_uid && eventSessionUid !== participant.session_uid) return;
      if (userUid === session.uid) {
        if (callLeaveMatches(callSessionUid, eventSessionUid)) await finish(false);
        return;
      }
      participants = participants.filter((item) => item.user_uid !== userUid);
      if (focusedUserUid === userUid) focusedUserUid = '';
      controller?.removePeer(userUid);
    }
  }

  async function heartbeat() {
    if (ended || heartbeatInFlight || stage !== 'active' || !localMedia) return;
    heartbeatInFlight = true;
    try {
      applyState(await updateCallParticipant(channelUid, callSessionUid, {
        audio_enabled: localMedia.microphoneEnabled,
        video_enabled: localMedia.cameraEnabled || localMedia.screenSharing,
        screen_sharing: localMedia.screenSharing
      }));
    } catch (reason) {
      if (ended) return;
      // Browsers can throttle timers in long-hidden tabs. Rejoin if the server
      // expired this participant instead of leaving a dead call dock behind.
      try {
        const current = await getCallState(channelUid);
        if (ended) return;
        if (callSessionWasReplaced(current.participants, session.uid, callSessionUid)) {
          await finish(false, 'replaced');
          return;
        }
        if (current.participants.some((participant) => participant.user_uid === session.uid && participant.session_uid === callSessionUid)) {
          applyState(current);
          controller?.connectToExisting(callPeerIdentities(current.participants, session.uid));
          return;
        }
        const rejoined = await joinCall(channelUid, localMedia.cameraEnabled || localMedia.screenSharing ? 'video' : 'voice', callSessionUid);
        if (ended) { void leaveCall(channelUid, callSessionUid).catch(() => undefined); return; }
        controller?.setMicrophoneEnabled(localMedia.microphoneEnabled);
        const state = await updateCallParticipant(channelUid, callSessionUid, {
          audio_enabled: localMedia.microphoneEnabled,
          video_enabled: localMedia.cameraEnabled || localMedia.screenSharing,
          screen_sharing: localMedia.screenSharing
        }).catch(() => rejoined);
        applyState(state);
        controller?.connectToExisting(callPeerIdentities(state.participants, session.uid));
      } catch {
        error = reason instanceof Error ? reason.message : 'The call connection could not be refreshed.';
      }
    } finally { heartbeatInFlight = false; }
  }

  async function toggleMicrophone() {
    if (!controller || !localMedia || deafened) return;
    const enabled = !localMedia.microphoneEnabled;
    controller.setMicrophoneEnabled(enabled);
    try { applyState(await updateCallParticipant(channelUid, callSessionUid, { audio_enabled: enabled })); }
    catch (reason) { error = reason instanceof Error ? reason.message : 'Microphone state could not be updated.'; }
  }

  async function toggleCamera() {
    if (!controller || !localMedia || mediaTransitionBusy) return;
    mediaTransitionBusy = true;
    try {
      await controller.setCameraEnabled(!localMedia.cameraEnabled);
      const state = controller.localState();
      applyState(await updateCallParticipant(channelUid, callSessionUid, { video_enabled: state.cameraEnabled || state.screenSharing }));
    } catch (reason) { error = callPermissionMessage(reason); }
    finally { mediaTransitionBusy = false; }
  }

  async function toggleScreenShare() {
    if (!controller || !localMedia || mediaTransitionBusy) return;
    mediaTransitionBusy = true;
    try {
      if (localMedia.screenSharing) await controller.stopScreenShare();
      else await controller.startScreenShare();
      const state = controller.localState();
      if (focusedUserUid === session.uid) focusedUserUid = '';
      applyState(await updateCallParticipant(channelUid, callSessionUid, { video_enabled: state.cameraEnabled || state.screenSharing, screen_sharing: state.screenSharing }));
    } catch (reason) { error = callPermissionMessage(reason); }
    finally { mediaTransitionBusy = false; }
  }

  async function screenEnded() {
    if (!controller || stage !== 'active') return;
    const state = controller.localState();
    if (focusedUserUid === session.uid) focusedUserUid = '';
    try { applyState(await updateCallParticipant(channelUid, callSessionUid, { video_enabled: state.cameraEnabled, screen_sharing: false })); }
    catch { /* The heartbeat will reconcile the state. */ }
  }

  async function finish(notifyServer = true, reason?: 'replaced') {
    if (ended) return;
    ended = true; stage = 'ending';
    window.clearInterval(heartbeatTimer);
    window.clearInterval(clockTimer);
    controller?.close(); controller = null;
    // Close the dock immediately. The session-scoped DELETE may finish after a
    // fast rejoin, but it cannot remove the newer browser session.
    if (notifyServer) void leaveCall(channelUid, callSessionUid).catch(() => undefined);
    onEnded(reason);
  }

  function peerFor(userUid: string) { return remoteMedia.find((item) => item.userUid === userUid) || null; }
  function formatDuration(milliseconds: number) {
    const seconds = Math.floor(milliseconds / 1000);
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const remainder = seconds % 60;
    return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}` : `${minutes}:${String(remainder).padStart(2, '0')}`;
  }
  async function refreshMediaDevices() {
    if (!navigator.mediaDevices?.enumerateDevices) return;
    deviceRefreshBusy = true;
    deviceStatus = '';
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const detectedMicrophones = devices.filter((device) => device.kind === 'audioinput');
      const detectedCameras = devices.filter((device) => device.kind === 'videoinput');
      const detectedOutputs = devices.filter((device) => device.kind === 'audiooutput');
      mediaDevices = devices;
      if (!detectedMicrophones.some((device) => device.deviceId === microphoneDeviceId)) microphoneDeviceId = detectedMicrophones[0]?.deviceId || '';
      if (!detectedCameras.some((device) => device.deviceId === cameraDeviceId)) cameraDeviceId = detectedCameras[0]?.deviceId || '';
      if (!detectedOutputs.some((device) => device.deviceId === outputDeviceId)) outputDeviceId = detectedOutputs[0]?.deviceId || '';
      deviceStatus = `${detectedMicrophones.length} microphone${detectedMicrophones.length === 1 ? '' : 's'} · ${detectedCameras.length} camera${detectedCameras.length === 1 ? '' : 's'}${detectedOutputs.length ? ` · ${detectedOutputs.length} output${detectedOutputs.length === 1 ? '' : 's'}` : ''}`;
    } catch { deviceStatus = 'MX could not refresh the browser device list.'; }
    finally { deviceRefreshBusy = false; }
  }
  async function changeMicrophone(deviceId: string) {
    microphoneDeviceId = deviceId;
    try { await controller?.selectMicrophoneDevice(deviceId); }
    catch (reason) { error = callPermissionMessage(reason); }
  }
  async function changeCamera(deviceId: string) {
    cameraDeviceId = deviceId;
    try { await controller?.selectCameraDevice(deviceId); }
    catch (reason) { error = callPermissionMessage(reason); }
  }
  async function testSpeaker() {
    if (speakerTesting) return;
    const AudioContextConstructor = window.AudioContext
      || (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextConstructor) { deviceStatus = 'This browser cannot run an output test.'; return; }
    speakerTesting = true;
    deviceStatus = 'Playing a short test tone…';
    const context = new AudioContextConstructor();
    const destination = context.createMediaStreamDestination();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    const audio = new Audio();
    try {
      oscillator.type = 'sine';
      oscillator.frequency.value = 523.25;
      gain.gain.setValueAtTime(0.0001, context.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.14, context.currentTime + 0.04);
      gain.gain.exponentialRampToValueAtTime(0.0001, context.currentTime + 0.38);
      oscillator.connect(gain).connect(destination);
      audio.srcObject = destination.stream;
      const output = audio as HTMLAudioElement & { setSinkId?: (deviceId: string) => Promise<void> };
      if (outputDeviceId && output.setSinkId) await output.setSinkId(outputDeviceId);
      await audio.play();
      oscillator.start();
      oscillator.stop(context.currentTime + 0.4);
      await new Promise((resolve) => window.setTimeout(resolve, 500));
      deviceStatus = 'Speaker test completed.';
    } catch (reason) {
      deviceStatus = reason instanceof Error ? `Speaker test failed: ${reason.message}` : 'Speaker test failed.';
    } finally {
      audio.pause();
      audio.srcObject = null;
      destination.stream.getTracks().forEach((track) => track.stop());
      void context.close().catch(() => undefined);
      speakerTesting = false;
    }
  }
  async function toggleDeafen() {
    if (!controller || !localMedia) return;
    const next = !deafened;
    if (next) {
      restoreMicrophoneAfterDeafen = localMedia.microphoneEnabled;
      deafened = true;
      if (restoreMicrophoneAfterDeafen) {
        controller.setMicrophoneEnabled(false);
        await updateCallParticipant(channelUid, callSessionUid, { audio_enabled: false }).then(applyState).catch(() => undefined);
      }
    } else {
      deafened = false;
      if (restoreMicrophoneAfterDeafen) {
        controller.setMicrophoneEnabled(true);
        await updateCallParticipant(channelUid, callSessionUid, { audio_enabled: true }).then(applyState).catch(() => undefined);
      }
      restoreMicrophoneAfterDeafen = false;
    }
  }
  function toggleExpanded() {
    expanded = !expanded;
    if (expanded) minimized = false;
  }
  async function toggleMinimized() {
    minimized = !minimized;
    if (minimized) {
      expanded = false;
      settingsOpen = false;
    }
    await tick();
    clampDockPosition();
  }
  function beginDockDrag(event: PointerEvent) {
    if (expanded || event.button !== 0 || (event.target as Element | null)?.closest('button')) return;
    const rect = callPanel?.getBoundingClientRect();
    if (!rect) return;
    dragState = { pointerId: event.pointerId, offsetX: event.clientX - rect.left, offsetY: event.clientY - rect.top };
    dockPosition = { x: rect.left, y: rect.top };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }
  function moveDock(event: PointerEvent) {
    if (!dragState || dragState.pointerId !== event.pointerId || !callPanel) return;
    const rect = callPanel.getBoundingClientRect();
    dockPosition = {
      x: Math.max(8, Math.min(event.clientX - dragState.offsetX, window.innerWidth - rect.width - 8)),
      y: Math.max(8, Math.min(event.clientY - dragState.offsetY, window.innerHeight - rect.height - 8))
    };
  }
  function endDockDrag(event: PointerEvent) {
    if (dragState?.pointerId !== event.pointerId) return;
    dragState = null;
    const target = event.currentTarget as HTMLElement;
    if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId);
  }
  function clampDockPosition() {
    if (!dockPosition || !callPanel || expanded) return;
    const rect = callPanel.getBoundingClientRect();
    dockPosition = {
      x: Math.max(8, Math.min(dockPosition.x, window.innerWidth - rect.width - 8)),
      y: Math.max(8, Math.min(dockPosition.y, window.innerHeight - rect.height - 8))
    };
  }
</script>

<svelte:window onresize={clampDockPosition} />
<section bind:this={callPanel} class:expanded class:minimized class:dragging={!!dragState} class="collaboration-call" style={dockStyle} aria-label={`Call in ${channelName}`}>
  <header role="toolbar" tabindex="-1" aria-label="Call window controls; drag this bar to move the call" onpointerdown={beginDockDrag} onpointermove={moveDock} onpointerup={endDockDrag} onpointercancel={endDockDrag} title={expanded ? undefined : 'Drag to move call'}>
    <div><i class:offline={!connected}></i><span><small>{connected ? `${channelKind === 'direct' ? 'Private call' : 'MX call'} · ${callDuration}` : 'Signaling reconnecting'}</small><strong>{channelName}</strong></span></div>
    <div>{#if !minimized}<button type="button" onclick={toggleExpanded} aria-label={expanded ? 'Restore call window' : 'Expand call window'} title={expanded ? 'Restore' : 'Expand'}>{#if expanded}<Minimize2 size={16} />{:else}<Maximize2 size={16} />{/if}</button>{/if}<button type="button" onclick={() => void toggleMinimized()} aria-label={minimized ? 'Show call' : 'Minimize call'} title={minimized ? 'Show call' : 'Minimize'}>{#if minimized}<ChevronUp size={16} />{:else}<ChevronDown size={16} />{/if}</button></div>
  </header>
  {#if !minimized}
    {#if error}<div class="call-error" role="alert"><span>{error}</span><button type="button" onclick={() => error = ''} aria-label="Dismiss call error">×</button></div>{/if}
    {#if settingsOpen}<section class="call-device-settings" aria-label="Call device settings">
      <header><span><strong>Voice and video settings</strong><small>Check your devices before they matter.</small></span><button type="button" onclick={() => settingsOpen = false} aria-label="Close device settings">×</button></header>
      <div class="call-device-preview" class:inactive={localMedia?.screenSharing || !localMedia?.cameraEnabled}>
        <video bind:this={settingsPreview} autoplay playsinline muted aria-label="Outgoing video preview"></video>
        {#if localMedia?.screenSharing}<span><MonitorUp size={20} /><strong>Screen sharing is active</strong><small>Self-preview is hidden to prevent screen feedback.</small></span>{:else if !localMedia?.cameraEnabled}<span><Camera size={20} /><strong>Camera is off</strong><small>Turn it on to check framing.</small></span>{/if}
        <em>Outgoing video</em>
      </div>
      <div class="call-device-section">
        <label><span><Mic size={14} /> Microphone</span><select value={microphoneDeviceId} onchange={(event) => void changeMicrophone(event.currentTarget.value)}>{#if !microphones.length}<option value="">Browser default</option>{/if}{#each microphones as device, index}<option value={device.deviceId}>{device.label || `Microphone ${index + 1}`}</option>{/each}</select></label>
        <div class="call-mic-check" aria-label={`Microphone level ${Math.round(localMicLevel * 100)} percent`}><span><AudioLines size={14} /> Mic level</span><div>{#each Array(12) as _, index}<i class:active={index < Math.ceil(localMicLevel * 12)}></i>{/each}</div><small>{!localMedia?.microphoneEnabled || deafened ? 'Muted' : localMicLevel > .04 ? 'Input detected' : 'Listening…'}</small></div>
      </div>
      <div class="call-device-section">
        <label><span><Camera size={14} /> Camera</span><select value={cameraDeviceId} onchange={(event) => void changeCamera(event.currentTarget.value)}>{#if !cameras.length}<option value="">No camera detected</option>{/if}{#each cameras as device, index}<option value={device.deviceId}>{device.label || `Camera ${index + 1}`}</option>{/each}</select></label>
      </div>
      <div class="call-device-section">
        <label><span><Volume2 size={14} /> Speaker</span><select bind:value={outputDeviceId}>{#if !audioOutputs.length}<option value="">System default</option>{/if}{#each audioOutputs as device, index}<option value={device.deviceId}>{device.label || `Speaker ${index + 1}`}</option>{/each}</select></label>
        <button class="button small call-speaker-test" type="button" onclick={() => void testSpeaker()} disabled={speakerTesting}><Volume2 size={14} /> {speakerTesting ? 'Testing…' : 'Test speaker'}</button>
      </div>
      <footer><small aria-live="polite">{deviceStatus || 'Device names and output routing are provided by your browser.'}</small><button class="button small" type="button" onclick={() => void refreshMediaDevices()} disabled={deviceRefreshBusy}><RefreshCw size={14} class={deviceRefreshBusy ? 'spinning' : ''} /> {deviceRefreshBusy ? 'Refreshing…' : 'Refresh devices'}</button></footer>
    </section>{/if}
    {#if stage === 'starting'}<div class="call-starting"><span class="loader"></span><strong>Starting secure media…</strong><small>Waiting for browser media permission.</small></div>
    {:else if stage === 'error'}<div class="call-starting error-state"><strong>Could not start the call</strong><small>{error}</small><button class="button" type="button" onclick={() => finish(false)}>Close</button></div>
    {:else}
      <div class:focused-view={!!focusedUserUid} class="call-media-grid" data-participants={participants.length}>
        <CallMediaTile participant={localParticipant} stream={localMedia?.stream || null} local focused={focusedUserUid === session.uid} onToggleFocus={() => focusedUserUid = focusedUserUid === session.uid ? '' : session.uid} connectionState="connected" />
        {#each remoteParticipants as participant (participant.user_uid)}{@const peer = peerFor(participant.user_uid)}<CallMediaTile {participant} stream={peer?.stream || null} {deafened} {outputDeviceId} focused={focusedUserUid === participant.user_uid} onToggleFocus={() => focusedUserUid = focusedUserUid === participant.user_uid ? '' : participant.user_uid} connectionState={peer?.connectionState || 'connecting'} />{/each}
      </div>
      <footer class="call-controls">
        <span>{participants.length} connected · {callDuration} · room limit {maxParticipants}</span>
        <div class="call-control-mic-meter" title={`Your microphone level: ${Math.round(localMicLevel * 100)}%`} aria-label={`Your microphone level ${Math.round(localMicLevel * 100)} percent`}>{#each Array(5) as _, index}<i class:active={localMedia?.microphoneEnabled && !deafened && index < Math.ceil(localMicLevel * 5)}></i>{/each}</div>
        <div>
          <button class:off={!localMedia?.microphoneEnabled} disabled={deafened} type="button" onclick={toggleMicrophone} aria-label={localMedia?.microphoneEnabled ? 'Mute microphone' : 'Unmute microphone'} title={deafened ? 'Undeafen first' : localMedia?.microphoneEnabled ? 'Mute' : 'Unmute'}>{#if localMedia?.microphoneEnabled}<Mic size={17} />{:else}<MicOff size={17} />{/if}</button>
          <button class:off={deafened} type="button" onclick={() => void toggleDeafen()} aria-label={deafened ? 'Undeafen' : 'Deafen'} title={deafened ? 'Undeafen' : 'Deafen'}>{#if deafened}<HeadphoneOff size={17} />{:else}<Headphones size={17} />{/if}</button>
          <button class:off={!localMedia?.cameraEnabled} disabled={mediaTransitionBusy} type="button" onclick={toggleCamera} aria-label={localMedia?.cameraEnabled ? 'Turn camera off' : 'Turn camera on'} title={localMedia?.cameraEnabled ? 'Camera off' : 'Camera on'}>{#if localMedia?.cameraEnabled}<Video size={17} />{:else}<VideoOff size={17} />{/if}</button>
          <button class:active={localMedia?.screenSharing} disabled={mediaTransitionBusy} type="button" onclick={toggleScreenShare} aria-label={localMedia?.screenSharing ? 'Stop screen sharing' : 'Share screen'} title={localMedia?.screenSharing ? 'Stop sharing' : 'Share screen'}><MonitorUp size={17} /></button>
          <button class:active={settingsOpen} type="button" onclick={() => settingsOpen = !settingsOpen} aria-label="Voice and video settings" title="Voice and video settings"><Settings2 size={17} /></button>
          <button class="hangup" type="button" onclick={() => finish(true)} aria-label="Leave call" title="Leave call"><PhoneOff size={18} /></button>
        </div>
      </footer>
    {/if}
  {:else}<button class="call-minimized-summary" type="button" onclick={() => minimized = false}><span><strong>{participants.length || 1}</strong><small>in call</small></span><em>{localMedia?.microphoneEnabled ? 'Mic on' : 'Muted'}</em></button>{/if}
</section>
