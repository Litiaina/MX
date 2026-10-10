// Some engines do not expose clipboard permission names through Permissions.
// Their actual clipboard APIs still enforce user gestures/access independently.
// https://developer.mozilla.org/en-US/docs/Web/API/Permissions/query
export function officePermissionQuery(nativeQuery: Permissions['query']): Permissions['query'] {
  return async descriptor => {
    try { return await nativeQuery(descriptor); }
    catch (error) {
      const name = descriptor.name as string;
      if (!(error instanceof TypeError) || !['clipboard-read','clipboard-write'].includes(name)) throw error;
      // Never forge a grant or swallow a real denial. The old Qt engine expects
      // a status object; prompt leaves real clipboard operations browser-gated.
      return Object.assign(new EventTarget(), {state:'prompt' as PermissionState, onchange:null}) as PermissionStatus;
    }
  };
}
export function installOfficeCompatibility(): () => void {
  if (!navigator.permissions?.query) return () => {};
  const native = navigator.permissions.query;
  const wrapped = officePermissionQuery(native.bind(navigator.permissions));
  navigator.permissions.query = wrapped;
  return () => { if (navigator.permissions.query === wrapped) navigator.permissions.query = native; };
}
