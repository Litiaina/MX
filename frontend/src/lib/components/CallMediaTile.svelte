<script lang="ts">
  import { onMount } from 'svelte';
  import Focus from '@lucide/svelte/icons/focus';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import Minimize2 from '@lucide/svelte/icons/minimize-2';
  import MicOff from '@lucide/svelte/icons/mic-off';
  import MonitorUp from '@lucide/svelte/icons/monitor-up';
  import AudioLines from '@lucide/svelte/icons/audio-lines';
  import type { CallParticipant } from '../api/domain';
  import { monitorAudioLevel, speakingFromLevel } from '../call/audioLevel';
  import { CallVideoPresentation, type CallVideoMode } from '../call/videoPresentation';
  import CallAudioTrack from './CallAudioTrack.svelte';
  import ProfileAvatar from './ProfileAvatar.svelte';

  let { participant, stream = null, local = false, deafened = false, focused = false, outputDeviceId = '', connectionState = 'new', onToggleFocus = () => undefined }: {
    participant: CallParticipant;
    stream?: MediaStream | null;
    local?: boolean;
    deafened?: boolean;
    focused?: boolean;
    outputDeviceId?: string;
    connectionState?: RTCPeerConnectionState;
    onToggleFocus?: () => void;
  } = $props();
  let video = $state<HTMLVideoElement>();
  let snapshot = $state<HTMLCanvasElement>();
  let presentation = $state<CallVideoPresentation | null>(null);
  let tile = $state<HTMLElement>();
  let fullscreen = $state(false);
  let fullscreenPending = $state(false);
  let audioLevel = $state(0);
  let speaking = $state(false);
  let videoReady = $state(false);
  let snapshotVisible = $state(false);
  let selfPreviewEnabled = $state(false);
  const hasVideo = $derived(participant.video_enabled && !!stream?.getVideoTracks().length);
  const selfScreenShare = $derived(local && participant.screen_sharing);
  const previewHidden = $derived(selfScreenShare && (!selfPreviewEnabled || fullscreen));
  const audioTracks = $derived(local ? [] : (stream?.getAudioTracks() || []));

  $effect(() => {
    if (!selfScreenShare) selfPreviewEnabled = false;
  });

  $effect(() => {
    if (!video || !snapshot) return;
    const renderer = new CallVideoPresentation(video, snapshot, (state) => {
      videoReady = state.ready;
      snapshotVisible = state.snapshotVisible;
    });
    presentation = renderer;
    return () => renderer.dispose();
  });

  $effect(() => {
    const currentStream = stream;
    if (!currentStream || !participant.audio_enabled) {
      audioLevel = 0;
      speaking = false;
      return;
    }
    return monitorAudioLevel(currentStream, (level) => {
      audioLevel = level;
      speaking = speakingFromLevel(level, speaking);
    });
  });

  function syncFullscreenState() {
    fullscreen = document.fullscreenElement === tile;
    fullscreenPending = false;
  }

  onMount(() => {
    document.addEventListener('fullscreenchange', syncFullscreenState);
    document.addEventListener('fullscreenerror', syncFullscreenState);
    syncFullscreenState();
    return () => {
      document.removeEventListener('fullscreenchange', syncFullscreenState);
      document.removeEventListener('fullscreenerror', syncFullscreenState);
    };
  });

  $effect(() => {
    if (!presentation) return;
    const mediaMode: CallVideoMode = participant.screen_sharing ? 'screen' : participant.video_enabled ? 'camera' : 'off';
    const videoTrack = stream?.getVideoTracks()[0] || null;
    presentation.setSource(previewHidden ? null : videoTrack, mediaMode);
  });

  async function toggleFullscreen() {
    if (fullscreenPending) return;
    fullscreenPending = true;
    try {
      if (document.fullscreenElement === tile) {
        await document.exitFullscreen?.();
        return;
      }
      if (!tile?.isConnected || !tile.requestFullscreen) return;
      await tile.requestFullscreen();
    } catch {
      // A browser can reject fullscreen while its window is losing focus or
      // changing display mode. Re-read native state so the control never gets
      // stuck showing the wrong action.
    } finally {
      syncFullscreenState();
    }
  }
</script>

<article bind:this={tile} class:has-video={hasVideo && !previewHidden && (videoReady || snapshotVisible)} class:screen-share={participant.screen_sharing} class:focused class:speaking class:media-interrupted={fullscreen && !hasVideo} class="call-media-tile" data-speaking={speaking} ondblclick={() => (hasVideo || fullscreen) && void toggleFullscreen()}>
  <video bind:this={video} class="call-video-layer" class:active={videoReady} autoplay playsinline muted aria-label={`${participant.user_name} call video`}></video>
  <canvas bind:this={snapshot} class="call-video-snapshot" class:active={snapshotVisible} aria-hidden="true"></canvas>
  {#each audioTracks as track, index (track.id)}
    <CallAudioTrack {track} {deafened} {outputDeviceId} label={`${participant.user_name} call audio${index ? ` ${index + 1}` : ''}`} />
  {/each}
  {#if previewHidden}
    <div class="call-self-share-status"><MonitorUp size={28} /><strong>You’re sharing your screen</strong><small>{fullscreen ? 'Your preview is hidden in fullscreen to prevent screen feedback.' : 'Your preview is hidden. Others still receive your screen.'}</small>{#if !fullscreen}<button type="button" onclick={() => selfPreviewEnabled = true}>Show my preview</button>{/if}</div>
  {:else if !hasVideo || (!videoReady && !snapshotVisible)}
    <div class="call-media-avatar"><ProfileAvatar userUid={participant.user_uid} name={participant.user_name} updatedAt={participant.profile_photo_updated_at} online /></div>
  {/if}
  {#if selfScreenShare && !previewHidden}<button type="button" class="call-hide-self-preview" onclick={() => selfPreviewEnabled = false}>Hide my preview</button>{/if}
  {#if fullscreen && !hasVideo}<div class="call-media-interrupted"><strong>Media reconnecting…</strong><small>Fullscreen will stay open while MX restores the stream.</small></div>{/if}
  {#if hasVideo || fullscreen}<div class="call-tile-actions"><button type="button" onclick={onToggleFocus} title={focused ? 'Return to grid' : 'Focus this video'} aria-label={focused ? `Return ${participant.user_name} to the grid` : `Focus ${participant.user_name}`}><Focus size={14} /></button><button type="button" class:active={fullscreen} disabled={fullscreenPending} aria-pressed={fullscreen} onclick={() => void toggleFullscreen()} title={fullscreen ? 'Exit full screen' : 'View full screen'} aria-label={fullscreen ? `Exit ${participant.user_name} full screen` : `View ${participant.user_name} full screen`}>{#if fullscreen}<Minimize2 size={14} />{:else}<Maximize2 size={14} />{/if}</button></div>{/if}
  <footer>
    <span><strong>{participant.user_name}{local ? ' (you)' : ''}</strong>{#if speaking}<small class="call-speaking-label"><AudioLines size={11} /> Speaking</small>{:else if participant.screen_sharing}<small><MonitorUp size={11} /> Sharing screen</small>{:else if !local && !['connected', 'new'].includes(connectionState)}<small>{connectionState === 'connecting' ? 'Connecting…' : 'Reconnecting…'}</small>{/if}</span>
    {#if !participant.audio_enabled}<i title="Microphone muted"><MicOff size={14} /></i>{/if}
  </footer>
</article>
