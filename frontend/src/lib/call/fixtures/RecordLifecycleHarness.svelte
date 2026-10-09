<script lang="ts">
  import { onMount } from 'svelte';
  import RecordsView from '../../components/RecordsView.svelte';
  import type { LiveMessage } from '../../live/client';
  let revision = $state(0);
  let message = $state<LiveMessage | null>(null);
  onMount(() => {
    (window as any).recordLifecycle = (next: LiveMessage) => { message = next; revision++; };
  });
</script>
<RecordsView session={{ uid: 'test', name: 'Test', email: 'test@example.invalid', access_level: 0, access_name: 'Administrator', totp_enabled: false, totp_enrollment_pending: false, recovery_codes_remaining: 0 }} accessLevel={0} {revision} liveMessage={message} />
