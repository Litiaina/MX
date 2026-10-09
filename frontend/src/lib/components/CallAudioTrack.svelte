<script lang="ts">
  import { onMount } from 'svelte';

  let { track, label, deafened = false, outputDeviceId = '' }: {
    track: MediaStreamTrack;
    label: string;
    deafened?: boolean;
    outputDeviceId?: string;
  } = $props();
  let audio = $state<HTMLAudioElement>();

  $effect(() => {
    if (!audio) return;
    const element = audio;
    const currentTrack = track;
    // One audio track per element: browsers do not consistently mix multiple
    // MediaStream audio tracks. Keep microphone and shared audio independent.
    if ((element.srcObject as MediaStream | null)?.getAudioTracks()[0] !== currentTrack) {
      element.srcObject = new MediaStream([currentTrack]);
    }
    element.muted = deafened;
    const output = element as HTMLAudioElement & { setSinkId?: (deviceId: string) => Promise<void> };
    if (output.setSinkId && output.sinkId !== outputDeviceId) void output.setSinkId(outputDeviceId).catch(() => undefined);
    const resume = () => { if (!deafened && currentTrack.readyState !== 'ended') void element.play().catch(() => undefined); };
    currentTrack.addEventListener('unmute', resume);
    element.addEventListener('canplay', resume);
    resume();
    return () => { currentTrack.removeEventListener('unmute', resume); element.removeEventListener('canplay', resume); };
  });

  onMount(() => () => { if (audio) { audio.pause(); audio.srcObject = null; } });
</script>

<audio bind:this={audio} autoplay aria-label={label}></audio>
