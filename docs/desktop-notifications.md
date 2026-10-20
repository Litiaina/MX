# Desktop notifications

[Documentation index](README.md) · [Collaboration](collaboration.md)

RC.5 sends operating-system notifications only while MX is
unfocused, hidden or minimized. A focused MX window uses its in-app notification
toast and notification center instead. A focused MX tab also suppresses a second
desktop popup from another tab for the same account in that browser profile.

## Enable and test

1. Open MX using your organization's **trusted HTTPS** address. A certificate
   warning bypass may not be sufficient; use the organization's trusted CA.
2. Open **My account → Notifications** and enable desktop notifications.
3. Allow notifications in the browser's site-permission prompt, then save.
4. Check **Browser permission** and **Saved MX desktop alerts** separately:
   enabling an MX account preference does not grant permission to every browser
   or URL. Select **Enable permission** if needed. If settings are unsaved, use
   **Save notification settings** before running a test.
5. Select **Test when I switch away**, then minimize the browser or focus another
   application. A test is queued for that transition, not shown over focused MX.
6. Ask another account to send a message or update an accessible record. Verify
   an in-app alert while focused and one OS alert while unfocused. Check both
   browser and operating-system notification settings if the latter is absent.

**Last desktop alert attempt** retains the actual result and time when returning
to MX or switching modules within the open workspace. It is not overwritten by
a generic readiness message on focus. A failed or submitted test also leaves
the queued state, so it can be retried. Reloading the whole page clears this
browser-session diagnostic; it does not clear the server's notification inbox.

If a test fails, report its exact result, Browser permission, Saved MX desktop
alerts, the browser/OS and the address being used. If it says submitted but no
banner appears, inspect browser and OS notification policy rather than assuming
that MX verified a visible banner. Chrome and Brave require their own permission
for each origin; allowing `https://localhost:21001` does not allow a LAN IP or
another browser automatically. See [Brave's site-permission instructions](https://support.brave.app/hc/en-us/articles/360018205431-How-do-I-change-site-permissions).

**Submitted to the browser** is deliberately not labelled “delivered to your
screen.” MX cannot determine whether Windows/Linux displayed a banner, placed
it in a notification center, suppressed it with Do Not Disturb, or blocked it
through browser/organization policy.

## Delivery and recovery

MX saves notifications on its server. An open client receives them over the
authenticated live connection and checks for missed entries when reconnecting,
returning online/focusing, and periodically. Recovery uses a timestamp + unique
ID cursor so equal-timestamp entries are not skipped between batches. The client
does not advance its cursor after a failed request.

Recovery and alert age use server timestamps, so an incorrect workstation clock
does not advance the recovery cursor past server messages. An alert received
while MX is focused stays in-app even if you switch away while it is preparing.

Delivery is deduplicated within the client and across same-account tabs using
unique IndexedDB receipts. Where browser locks/storage are unavailable,
cross-tab coordination is best-effort;
the durable MX inbox remains authoritative. Quiet hours, notification category
preferences, immediate/digest settings and current access permissions still apply.
Older recovered notifications remain in the inbox without an OS-popup flood.

Keep MX open. Closing the browser, putting the computer to sleep, or having the
browser freeze/discard the tab can suspend JavaScript and its network connection.
This local-only design does not use an internet push service and cannot promise
OS delivery while the client is stopped. Open MX again to recover its inbox.

## Release qualification

Automated Chromium, Firefox and WebKit checks instrument the notification API to
verify MX's focus policy, deduplication and disconnect/reconnect recovery. They
do not prove that a visible Windows banner appeared. Test the actual deployment's
Windows/browser versions, trusted HTTPS origin, site permission and OS policy
before relying on desktop alerts operationally.
