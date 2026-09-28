import { get, writable } from 'svelte/store';

export type ConfirmationTone = 'danger' | 'primary';

export type ConfirmationRequest = {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: ConfirmationTone;
};

type ActiveConfirmation = ConfirmationRequest & {
  resolve: (confirmed: boolean) => void;
};

export const activeConfirmation = writable<ActiveConfirmation | null>(null);

export function requestConfirmation(request: ConfirmationRequest): Promise<boolean> {
  const active = get(activeConfirmation);
  if (active) active.resolve(false);

  return new Promise((resolve) => {
    activeConfirmation.set({ ...request, resolve });
  });
}

export function answerConfirmation(confirmed: boolean) {
  const active = get(activeConfirmation);
  if (!active) return;
  activeConfirmation.set(null);
  active.resolve(confirmed);
}
