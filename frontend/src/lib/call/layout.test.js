import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('../../styles/workspaces.css', import.meta.url), 'utf8');
const component = readFileSync(new URL('../components/CollaborationCall.svelte', import.meta.url), 'utf8');
const mediaTile = readFileSync(new URL('../components/CallMediaTile.svelte', import.meta.url), 'utf8');
const workspace = readFileSync(new URL('../components/Workspace.svelte', import.meta.url), 'utf8');
const workspaceApi = readFileSync(new URL('../api/workspace.ts', import.meta.url), 'utf8');

describe('call layout safeguards', () => {
  it('keeps conversation call status compact instead of assigning it the flexible content row', () => {
    expect(css).toMatch(/\.conversation-main\s*\{[^}]*display:\s*flex;[^}]*flex-direction:\s*column;/s);
    expect(css).toMatch(/\.conversation-main\s*>\s*\.message-list\s*\{[^}]*flex:\s*1\s+1\s+auto;/s);
    expect(css).toMatch(/\.conversation-call-status\s*\{[^}]*max-height:\s*4\.25rem;[^}]*flex:\s*0\s+0\s+auto;/s);
  });

  it('uses content-driven call panel sizing and exposes focused and full-screen media states', () => {
    expect(css).toMatch(/\.collaboration-call\s*\{[^}]*display:\s*flex;[^}]*flex-direction:\s*column;/s);
    expect(css).toContain('.call-media-grid.focused-view');
    expect(css).toContain('.call-media-tile:fullscreen');
    expect(css).toContain('.incoming-call-card');
  });

  it('keeps each media tile full-screen control synchronized with the browser', () => {
    expect(mediaTile).toContain("document.addEventListener('fullscreenchange', syncFullscreenState)");
    expect(mediaTile).toContain("document.addEventListener('fullscreenerror', syncFullscreenState)");
    expect(mediaTile).toContain('document.fullscreenElement === tile');
    expect(mediaTile).toContain('document.exitFullscreen?.()');
    expect(mediaTile).toContain('{#if hasVideo || fullscreen}');
    expect(mediaTile).toContain('{#if fullscreen && !hasVideo}');
    expect(mediaTile).toContain("fullscreen ? 'Exit full screen' : 'View full screen'");
    expect(mediaTile).toContain('{#if fullscreen}<Minimize2 size={14} />{:else}<Maximize2 size={14} />{/if}');
    expect(css).toMatch(/\.call-media-tile:fullscreen\s+\.call-tile-actions\s*\{[^}]*opacity:\s*1;/s);
  });

  it('keeps remote microphone playback independent from camera and screen rendering', () => {
    expect(mediaTile).toContain('<video bind:this={video} class="call-video-layer"');
    expect(mediaTile.match(/<video\b/g)).toHaveLength(1);
    expect(mediaTile).toContain('<audio bind:this={audio} autoplay');
    expect(mediaTile).toContain("currentStream?.addEventListener('addtrack', resumeAudio)");
    expect(css).toMatch(/\.call-media-tile\s*>\s*audio\s*\{[^}]*display:\s*none;/s);
  });

  it('uses one video and a still frame for source changes without opacity crossfading', () => {
    expect(mediaTile).toContain('new CallVideoPresentation(video, snapshot');
    expect(mediaTile).toContain('<canvas bind:this={snapshot}');
    expect(css).toMatch(/\.call-media-tile\s+\.call-video-layer\s*\{\s*opacity:\s*0;\s*\}/s);
    expect(css).toMatch(/\.call-media-tile\s+\.call-video-layer\.active\s*\{[^}]*opacity:\s*1;/s);
  });

  it('uses MX collaboration colors for native call device controls in either theme', () => {
    expect(css).toMatch(/\.call-device-settings\s*\{[^}]*background:\s*var\(--collab-bg\);[^}]*color:\s*var\(--collab-text\);/s);
    expect(css).toMatch(/\.call-device-settings option\s*\{[^}]*background-color:\s*var\(--collab-bg\);[^}]*color:\s*var\(--collab-text\);/s);
  });

  it('sends hangup with keepalive so closing or navigating cannot strand the room', () => {
    expect(workspaceApi).toMatch(/leaveCall[\s\S]*?method:\s*'DELETE',\s*keepalive:\s*true/s);
    expect(component).toContain("window.addEventListener('pagehide', leaveOnPageHide)");
  });

  it('keeps the minimized dock compact even after minimizing an expanded call', () => {
    expect(css).toMatch(/\.collaboration-call\.minimized\s*\{[^}]*inset:\s*auto[^;}]*;[^}]*height:\s*auto;[^}]*max-height:\s*7rem;/s);
    expect(css).toMatch(/\.call-minimized-summary\s*\{[^}]*height:\s*3\.5rem;[^}]*max-height:\s*3\.5rem;[^}]*flex:\s*0\s+0\s+3\.5rem;/s);
    expect(component).toMatch(/if\s*\(minimized\)\s*\{\s*expanded\s*=\s*false;/s);
    expect(component).toMatch(/if\s*\(expanded\)\s*minimized\s*=\s*false;/s);
  });

  it('lets users drag the call dock while keeping expanded mode anchored', () => {
    expect(component).toContain('function beginDockDrag(event: PointerEvent)');
    expect(component).toContain('function clampDockPosition()');
    expect(component).toContain("right:auto;bottom:auto");
    expect(css).toContain('.collaboration-call.dragging > header');
    expect(css).toContain('.collaboration-call.expanded > header');
  });

  it('presents and cleans up private and group incoming-call alerts separately', () => {
    expect(workspace).toContain("callAlertStrategy(call, callerName)");
    expect(workspace).toContain("strategy.kind === 'direct' ? 'direct-call' : 'group-call'");
    expect(workspace).toContain("stopIncomingCallAlert(call.channel_uid)");
    expect(workspace).toContain("privateCall ? 'Incoming private call' : 'Group call available'");
    expect(workspace).toContain("privateCall ? 'Decline' : 'Not now'");
    expect(css).toMatch(/\.incoming-call-card\s*>\s*footer\s*\{[^}]*grid-template-columns:\s*repeat\(3,/s);
  });

  it('closes the call UI before background server cleanup so immediate rejoin is available', () => {
    expect(component).toMatch(/if\s*\(notifyServer\)\s*void\s+leaveCall\(channelUid,\s*callSessionUid\)[\s\S]*?onEnded\(reason\);/s);
  });
});
