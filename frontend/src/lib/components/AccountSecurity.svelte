<script lang="ts">
  import {
    beginTotpEnrollment,
    changePassword,
    confirmTotpEnrollment,
    disableTotp,
    regenerateRecoveryCodes,
    updateProfile
  } from '../api/account';
  import { secondFactor } from '../api/auth';
  import { clearAuthTokens } from '../api/client';
  import type { Session, TotpEnrollmentResponse } from '../api/types';

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

  $effect(() => {
    profileName = session.name;
    profileEmail = session.email;
  });

  function startAction(name: string) {
    busyAction = name;
    notice = '';
    error = '';
  }

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
      notice = 'Scan the QR code, then confirm a current authenticator code.';
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
            <label>Current password<input bind:value={enrollmentPassword} type="password" autocomplete="current-password" required /></label>
            <button class="button primary" type="submit" disabled={busyAction !== ''}>Set up authenticator</button>
          </form>
        {/if}
      </section>
    </div>
  {/if}
</section>
