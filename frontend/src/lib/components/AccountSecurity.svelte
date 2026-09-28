<script lang="ts">
  import { onMount } from 'svelte';
  import Bell from '@lucide/svelte/icons/bell';
  import Play from '@lucide/svelte/icons/play';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import {
    beginTotpEnrollment,
    cancelTotpEnrollment,
    changePassword,
    confirmTotpEnrollment,
    disableTotp,
    regenerateRecoveryCodes,
    updateProfile
  } from '../api/account';
  import { secondFactor } from '../api/auth';
  import { clearAuthTokens } from '../api/client';
  import type { Session, TotpEnrollmentResponse } from '../api/types';
  import type { NotificationPreferences, NotificationSoundInfo, UserPreferences } from '../api/domain';
  import { deleteNotificationSound, loadNotificationSound, loadNotificationSoundInfo, loadPreferences, savePreferences, uploadNotificationSound } from '../api/workspace';
  import { decodeNotificationSound, playNotificationSound } from '../util/notificationAudio';

  let {
    session,
    onSessionChanged,
    onSessionEnded
  }: {
    session: Session;
    onSessionChanged: () => Promise<unknown>;
    onSessionEnded: (message: string) => void;
  } = $props();

  let busyAction = $state('');
  let notice = $state('');
  let error = $state('');

  let profileName = $state('');
  let profileEmail = $state('');
  let profilePassword = $state('');
  let profileFactor = $state('');

  let currentPassword = $state('');
  let newPassword = $state('');
  let confirmPassword = $state('');
  let passwordFactor = $state('');

  let enrollmentPassword = $state('');
  let enrollmentCode = $state('');
  let enrollment = $state<TotpEnrollmentResponse | null>(null);

  let securityPassword = $state('');
  let securityFactor = $state('');
  let recoveryCodes = $state<string[]>([]);
  let accountPreferences = $state<UserPreferences | null>(null);
  let notifications = $state<NotificationPreferences | null>(null);
  let notificationSound = $state<NotificationSoundInfo>({ exists: false });
  let selectedSound = $state<File | null>(null);
  let soundUploadProgress = $state(0);

  onMount(() => void loadNotificationSettings());

  $effect(() => {
    profileName = session.name;
    profileEmail = session.email;
  });

  function startAction(name: string) {
    busyAction = name;
    notice = '';
    error = '';
  }

  async function loadNotificationSettings() {
    try {
      const response = await loadPreferences();
      accountPreferences = response.preferences;
      notifications = { ...response.preferences.notifications };
      // Custom sound support is optional while an older MX process is being
      // restarted. It must never prevent Profile, Password, or 2FA from loading.
      try { notificationSound = await loadNotificationSoundInfo(); }
      catch { notificationSound = { exists: false }; }
    }
    catch (reason) { fail(reason); }
  }
  function setNotification(key: keyof NotificationPreferences, value: string) {
    if (!notifications) return;
    (notifications as unknown as Record<string, unknown>)[key] = value === '' ? null : value === 'true';
  }
  async function saveNotificationSettings(event: SubmitEvent) {
    event.preventDefault(); if (!accountPreferences || !notifications) return;
    startAction('notifications');
    try {
      if (notifications.browser_enabled && 'Notification' in window && Notification.permission === 'default') {
        const permission = await Notification.requestPermission();
        if (permission !== 'granted') notifications.browser_enabled = false;
      }
      const response = await savePreferences({ ...accountPreferences, notifications });
      accountPreferences = response.preferences; notifications = { ...response.preferences.notifications };
      notice = 'Notification preferences saved.';
    } catch (reason) { fail(reason); } finally { busyAction = ''; }
  }
  function chooseNotificationSound(event: Event) {
    selectedSound = (event.currentTarget as HTMLInputElement).files?.[0] || null;
    soundUploadProgress = 0;
  }
  async function uploadCustomSound() {
    if (!selectedSound || !accountPreferences || !notifications) return;
    startAction('notification-sound'); soundUploadProgress = 0;
    try {
      const decoded = await decodeNotificationSound(selectedSound);
      await playNotificationSound(decoded, notifications.sound_volume ?? 70);
      notificationSound = await uploadNotificationSound(selectedSound, (loaded, total) => soundUploadProgress = total ? Math.round(loaded / total * 100) : 0);
      notifications.sound_enabled = true; notifications.sound_source = 'custom';
      const response = await savePreferences({ ...accountPreferences, notifications });
      accountPreferences = response.preferences; notifications = { ...response.preferences.notifications };
      selectedSound = null; soundUploadProgress = 100; notice = 'Custom notification sound uploaded to N1 and enabled.';
    } catch (reason) { fail(reason); } finally { busyAction = ''; }
  }
  async function removeCustomSound() {
    if (!accountPreferences || !notifications || !notificationSound.exists || !window.confirm('Remove your custom notification sound and use the MX sound?')) return;
    startAction('notification-sound');
    try {
      await deleteNotificationSound(); notificationSound = { exists: false };
      notifications.sound_source = 'default';
      const response = await savePreferences({ ...accountPreferences, notifications });
      accountPreferences = response.preferences; notifications = { ...response.preferences.notifications };
      notice = 'Custom notification sound removed. MX will use its built-in sound.';
    } catch (reason) { fail(reason); } finally { busyAction = ''; }
  }
  async function testNotificationSound() {
    if (!notifications) return;
    startAction('test-notification-sound');
    try {
      let buffer: AudioBuffer | null = null;
      if (selectedSound) buffer = await decodeNotificationSound(selectedSound);
      else if (notifications.sound_source === 'custom' && notificationSound.exists) buffer = await decodeNotificationSound(await loadNotificationSound(notificationSound.updated_at));
      await playNotificationSound(buffer, notifications.sound_volume ?? 70);
      notice = 'Notification sound played.';
    } catch (reason) { fail(reason); } finally { busyAction = ''; }
  }
  function formatBytes(value = 0) { return value < 1024 ** 2 ? `${(value / 1024).toFixed(1)} KB` : `${(value / 1024 ** 2).toFixed(1)} MB`; }

  function fail(reason: unknown) {
    error = reason instanceof Error ? reason.message : 'The operation failed.';
  }

  async function saveProfile(event: SubmitEvent) {
    event.preventDefault();
    const name = profileName.trim() === session.name ? null : profileName.trim();
    const email = profileEmail.trim() === session.email ? null : profileEmail.trim();
    if (!name && !email) {
      notice = 'There are no profile changes to save.';
      return;
    }

    startAction('profile');
    try {
      const response = await updateProfile({
        currentPassword: profilePassword,
        factor: secondFactor(profileFactor),
        name,
        email
      });
      profilePassword = '';
      profileFactor = '';
      if (response.session_invalidated) {
        onSessionEnded('Your profile credentials changed. Sign in again.');
        return;
      }
      await onSessionChanged();
      notice = 'Profile updated.';
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function savePassword(event: SubmitEvent) {
    event.preventDefault();
    if (newPassword !== confirmPassword) {
      error = 'New passwords do not match.';
      return;
    }

    startAction('password');
    try {
      await changePassword({
        currentPassword,
        newPassword,
        factor: secondFactor(passwordFactor)
      });
      onSessionEnded('Password changed. All existing sessions were revoked.');
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function startEnrollment(event: SubmitEvent) {
    event.preventDefault();
    startAction('enroll');
    try {
      enrollment = await beginTotpEnrollment(enrollmentPassword);
      enrollmentCode = '';
      await onSessionChanged();
      notice = 'Scan the QR code, then confirm a current authenticator code.';
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function cancelEnrollment() {
    if (!enrollmentPassword) {
      error = 'Enter your current password to cancel the incomplete enrollment.';
      return;
    }
    startAction('cancel-enrollment');
    try {
      await cancelTotpEnrollment(enrollmentPassword);
      enrollment = null;
      enrollmentPassword = '';
      enrollmentCode = '';
      await onSessionChanged();
      notice = 'Incomplete authenticator enrollment cancelled.';
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function confirmEnrollment(event: SubmitEvent) {
    event.preventDefault();
    startAction('confirm');
    try {
      const response = await confirmTotpEnrollment(enrollmentPassword, enrollmentCode.trim());
      enrollment = null;
      enrollmentPassword = '';
      enrollmentCode = '';
      presentRecoveryCodes(response.recovery_codes);
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function removeTotp() {
    if (!window.confirm('Disable two-factor authentication and revoke every recovery code?')) return;
    startAction('disable');
    try {
      await disableTotp(securityPassword, secondFactor(securityFactor));
      onSessionEnded('Two-factor authentication was disabled. Sign in again.');
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  async function replaceRecoveryCodes() {
    if (!window.confirm('Replace every recovery code? Existing codes will stop working immediately.')) return;
    startAction('recovery');
    try {
      const response = await regenerateRecoveryCodes(
        securityPassword,
        secondFactor(securityFactor)
      );
      securityPassword = '';
      securityFactor = '';
      presentRecoveryCodes(response.recovery_codes);
    } catch (reason) {
      fail(reason);
    } finally {
      busyAction = '';
    }
  }

  function presentRecoveryCodes(codes: string[]) {
    clearAuthTokens();
    recoveryCodes = [...codes];
    notice = 'Save these recovery codes now. They will not be displayed again.';
  }

  async function copyRecoveryCodes() {
    try {
      await navigator.clipboard.writeText(recoveryCodes.join('\n'));
      notice = 'Recovery codes copied.';
    } catch {
      error = 'Clipboard access is unavailable. Download the codes instead.';
    }
  }

  function downloadRecoveryCodes() {
    const contents = `MX recovery codes\nGenerated: ${new Date().toISOString()}\n\n${recoveryCodes.join('\n')}\n`;
    const url = URL.createObjectURL(new Blob([contents], { type: 'text/plain;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'mx-recovery-codes.txt';
    link.click();
    URL.revokeObjectURL(url);
  }

  function finishRecoveryHandoff() {
    recoveryCodes = [];
    onSessionEnded('Security settings changed. Sign in again with your updated credentials.');
  }
</script>

<section class="account-page" aria-labelledby="account-title">
  <div class="page-heading">
    <div>
      <p class="eyebrow">Profile and security</p>
      <h1 id="account-title">My Account</h1>
      <p class="muted">Manage your identity, password, authenticator, and recovery codes.</p>
    </div>
    <div class:secure={session.totp_enabled} class="security-pill">
      {session.totp_enabled ? '2FA enabled' : '2FA not enabled'}
    </div>
  </div>

  {#if notice}
    <div class="notice success" role="status">{notice}</div>
  {/if}
  {#if error}
    <div class="notice error" role="alert">{error}</div>
  {/if}

  {#if recoveryCodes.length}
    <section class="panel recovery-panel">
      <p class="eyebrow">One-time display</p>
      <h2>Save your recovery codes</h2>
      <p>Each code works once. Store them somewhere separate from your authenticator.</p>
      <div class="recovery-grid">
        {#each recoveryCodes as code}
          <code>{code}</code>
        {/each}
      </div>
      <div class="button-row">
        <button class="button" type="button" onclick={copyRecoveryCodes}>Copy</button>
        <button class="button" type="button" onclick={downloadRecoveryCodes}>Download</button>
        <button class="button primary" type="button" onclick={finishRecoveryHandoff}>I saved them</button>
      </div>
    </section>
  {:else}
    <div class="account-grid">
      <section class="panel">
        <h2>Profile</h2>
        <p class="muted">Email changes revoke every current session.</p>
        <form class="form-stack" onsubmit={saveProfile}>
          <label>Name<input bind:value={profileName} autocomplete="name" required /></label>
          <label>Email<input bind:value={profileEmail} type="email" autocomplete="username" required /></label>
          <label>Current password<input bind:value={profilePassword} type="password" autocomplete="current-password" required /></label>
          <label>Authenticator or recovery code<input bind:value={profileFactor} autocomplete="one-time-code" placeholder="Required when 2FA is enabled" /></label>
          <button class="button primary" type="submit" disabled={busyAction !== ''}>Save profile</button>
        </form>
      </section>

      <section class="panel">
        <h2>Password</h2>
        <p class="muted">Use at least 12 characters. A change revokes every existing session.</p>
        <form class="form-stack" onsubmit={savePassword}>
          <label>Current password<input bind:value={currentPassword} type="password" autocomplete="current-password" required /></label>
          <label>New password<input bind:value={newPassword} type="password" minlength="12" autocomplete="new-password" required /></label>
          <label>Confirm new password<input bind:value={confirmPassword} type="password" minlength="12" autocomplete="new-password" required /></label>
          <label>Authenticator or recovery code<input bind:value={passwordFactor} autocomplete="one-time-code" placeholder="Required when 2FA is enabled" /></label>
          <button class="button primary" type="submit" disabled={busyAction !== ''}>Change password</button>
        </form>
      </section>

      <section class="panel wide">
        <div class="panel-heading">
          <div>
            <h2>Two-factor authentication</h2>
            <p class="muted">
              {#if session.totp_enabled}
                {session.recovery_codes_remaining} recovery code{session.recovery_codes_remaining === 1 ? '' : 's'} remaining.
              {:else if session.totp_enrollment_pending}
                An authenticator enrollment was started but has not been confirmed. Two-factor authentication is not active yet.
              {:else}
                Enrollment remains inactive until you confirm a valid code.
              {/if}
            </p>
          </div>
        </div>

        {#if session.totp_enabled}
          <div class="security-management">
            <label>Current password<input bind:value={securityPassword} type="password" autocomplete="current-password" required /></label>
            <label>Authenticator or recovery code<input bind:value={securityFactor} autocomplete="one-time-code" required /></label>
            <div class="button-row">
              <button class="button" type="button" onclick={replaceRecoveryCodes} disabled={busyAction !== ''}>Replace recovery codes</button>
              <button class="button danger" type="button" onclick={removeTotp} disabled={busyAction !== ''}>Disable 2FA</button>
            </div>
          </div>
        {:else if enrollment}
          <div class="enrollment-grid">
            <img src={enrollment.qr_code_data_url} alt="Authenticator enrollment QR code" />
            <div>
              <h3>Scan, then confirm</h3>
              <p class="muted">Enrollment expires in {Math.floor(enrollment.expires_in / 60)} minutes.</p>
              <code class="secret">{enrollment.secret}</code>
              <form class="form-stack compact" onsubmit={confirmEnrollment}>
                <label>Authenticator code<input bind:value={enrollmentCode} inputmode="numeric" pattern="[0-9]{6}" autocomplete="one-time-code" required /></label>
                <button class="button primary" type="submit" disabled={busyAction !== ''}>Confirm and enable</button>
              </form>
            </div>
          </div>
        {:else}
          <form class="form-stack compact" onsubmit={startEnrollment}>
            {#if session.totp_enrollment_pending}<div class="notice warning"><strong>Enrollment incomplete</strong><p>For security, the previous secret is not shown again. Enter your password to restart with a new QR code, or cancel the pending enrollment.</p></div>{/if}
            <label>Current password<input bind:value={enrollmentPassword} type="password" autocomplete="current-password" required /></label>
            <div class="button-row">
              {#if session.totp_enrollment_pending}<button class="button" type="button" onclick={cancelEnrollment} disabled={busyAction !== ''}>Cancel incomplete setup</button>{/if}
              <button class="button primary" type="submit" disabled={busyAction !== ''}>{session.totp_enrollment_pending ? 'Restart authenticator setup' : 'Set up authenticator'}</button>
            </div>
          </form>
        {/if}
      </section>

      {#if notifications}
        <section class="panel wide notification-settings">
          <div class="panel-heading"><div><h2>Notifications</h2><p class="muted">Receive immediate in-app alerts, sound, and optional operating-system notifications. Channel-level mute settings still take priority.</p></div><Bell size={22} aria-hidden="true" /></div>
          <form onsubmit={saveNotificationSettings}>
            <div class="notification-delivery-options">
              <label class="checkbox"><input type="checkbox" bind:checked={notifications.browser_enabled} /> Show operating-system notifications when MX is in the background</label>
              <label class="checkbox"><input type="checkbox" bind:checked={notifications.sound_enabled} /> Play a sound for immediate notifications</label>
            </div>

            <section class:disabled={!notifications.sound_enabled} class="notification-sound-card">
              <div class="notification-sound-heading"><span><Volume2 size={20} /></span><div><strong>Notification sound</strong><small>The built-in chime works offline. A custom sound is stored privately in N1, preloaded into MX's server cache, and decoded once by this browser.</small></div></div>
              <div class="notification-sound-controls">
                <label>Sound<select bind:value={notifications.sound_source} disabled={!notifications.sound_enabled}><option value="default">Built-in MX chime</option><option value="custom" disabled={!notificationSound.exists}>My custom sound{notificationSound.exists && notificationSound.file_name ? ` — ${notificationSound.file_name}` : ''}</option></select></label>
                <label>Volume: {notifications.sound_volume}%<input type="range" min="0" max="100" step="5" bind:value={notifications.sound_volume} disabled={!notifications.sound_enabled} /></label>
                <button class="button icon-label" type="button" onclick={testNotificationSound} disabled={!notifications.sound_enabled || busyAction !== ''}><Play size={15} />Test sound</button>
              </div>
              <div class="notification-sound-upload">
                <label class="sound-file-picker"><Upload size={17} /><span><strong>{selectedSound?.name || 'Upload a custom sound'}</strong><small>MP3, WAV, OGG, WebM audio, M4A, or AAC · maximum 5 MiB</small></span><input type="file" accept="audio/mpeg,audio/wav,audio/ogg,audio/webm,audio/mp4,audio/aac,.mp3,.wav,.ogg,.webm,.m4a,.aac" onchange={chooseNotificationSound} /></label>
                {#if selectedSound}<button class="button primary icon-label" type="button" onclick={uploadCustomSound} disabled={busyAction !== ''}><Upload size={15} />{busyAction === 'notification-sound' ? `Uploading ${soundUploadProgress}%` : 'Upload and use'}</button>{/if}
                {#if notificationSound.exists}<div class="stored-sound"><span><strong>{notificationSound.file_name}</strong><small>{notificationSound.mime_type} · {formatBytes(notificationSound.size)}</small></span><button class="button danger small icon-label" type="button" onclick={removeCustomSound} disabled={busyAction !== ''}><Trash2 size={14} />Remove</button></div>{/if}
              </div>
            </section>

            <div class="notification-preference-grid">
              <label>Direct messages<select value={String(notifications.messages ?? '')} onchange={(event) => setNotification('messages', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Mentions<select value={String(notifications.mentions ?? '')} onchange={(event) => setNotification('mentions', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Channel activity<select value={String(notifications.channel_activity ?? '')} onchange={(event) => setNotification('channel_activity', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Records created<select value={String(notifications.record_created ?? '')} onchange={(event) => setNotification('record_created', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Records updated<select value={String(notifications.record_updated ?? '')} onchange={(event) => setNotification('record_updated', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Record assignments<select value={String(notifications.record_assigned ?? '')} onchange={(event) => setNotification('record_assigned', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Attachments received<select value={String(notifications.attachment_received ?? '')} onchange={(event) => setNotification('attachment_received', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Workflow changes<select value={String(notifications.workflow_changes ?? '')} onchange={(event) => setNotification('workflow_changes', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
              <label>Task activity<select value={String(notifications.task_activity ?? '')} onchange={(event) => setNotification('task_activity', event.currentTarget.value)}><option value="">Use system default</option><option value="true">Notify</option><option value="false">Do not notify</option></select></label>
            </div>
            <div class="notification-schedule"><label>Delivery<select bind:value={notifications.digest}><option value={null}>Use system default</option><option value="immediate">Immediately</option><option value="daily">Daily digest</option><option value="weekly">Weekly digest</option><option value="off">Only show in notification center</option></select></label><label>Quiet hours start<input type="time" bind:value={notifications.quiet_hours_start} /></label><label>Quiet hours end<input type="time" bind:value={notifications.quiet_hours_end} /></label></div>
            <div class="button-row"><button class="button primary" disabled={busyAction !== ''}>Save notification preferences</button></div>
          </form>
        </section>
      {/if}
    </div>
  {/if}
</section>
