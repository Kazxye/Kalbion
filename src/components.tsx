import { useEffect, useRef, useState, type ReactNode } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Boxes, X } from 'lucide-react';
import { desktop } from './api';
import type { Item } from './types';

export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Stat({
  label,
  value,
  hint,
  accent = false,
}: {
  label: string;
  value: string;
  hint: string;
  accent?: boolean;
}) {
  return (
    <div className={`stat ${accent ? 'accent' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{hint}</small>
    </div>
  );
}
export function Dialog({
  title,
  close,
  children,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
}) {
  const container = useRef<HTMLElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const panel = container.current;
    if (!panel) return;
    if (!panel.contains(document.activeElement)) {
      panel
        .querySelector<HTMLElement>('input, select, textarea, button')
        ?.focus();
    }
    const trap = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const elements = Array.from(
        panel.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled)',
        ),
      );
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    panel.addEventListener('keydown', trap);
    return () => {
      panel.removeEventListener('keydown', trap);
      previous?.focus();
    };
  }, []);
  return (
    <div className="overlay" onClick={close}>
      <section
        ref={container}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <h2>{title}</h2>
          <button aria-label="Fechar" onClick={close}>
            <X size={18} />
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
export interface Confirmation {
  title: string;
  message: string;
  confirmLabel: string;
  action: () => Promise<unknown>;
  notice: string;
}
export function ConfirmDialog({
  confirmation,
  error,
  busy,
  confirm,
  cancel,
}: {
  confirmation: Confirmation;
  error: string;
  busy: boolean;
  confirm: () => void;
  cancel: () => void;
}) {
  return (
    <Dialog title={confirmation.title} close={cancel}>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <p className="help">{confirmation.message}</p>
      <div className="form-actions">
        <button type="button" disabled={busy} onClick={cancel}>
          Cancelar
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy}
          onClick={confirm}
        >
          {confirmation.confirmLabel}
        </button>
      </div>
    </Dialog>
  );
}

/** Just past the Rust cooldown for failed downloads (120 s), so a retry can reach the network. */
const ICON_RETRY_MS = 130_000;
function iconUrl(id: string, quality: number | null, attempt: number) {
  const params = new URLSearchParams();
  if (quality) params.set('quality', String(quality));
  // A new URL per attempt keeps the webview from reusing a cached failure.
  if (attempt) params.set('attempt', String(attempt));
  const query = params.toString();
  return `${convertFileSrc(id, 'icon')}${query ? `?${query}` : ''}`;
}

/**
 * Official render served through the Rust icon cache. On failure it shows a generic icon
 * and retries in the background with an off-screen probe, swapping only if it loads.
 */
export function ItemIcon({
  item,
  quality,
}: {
  item: Item;
  quality: number | null;
}) {
  const key = `${item.id}:${quality ?? ''}`;
  const [state, setState] = useState({ key, attempt: 0, failed: false });
  const current =
    state.key === key ? state : { key, attempt: 0, failed: false };
  useEffect(() => {
    if (!desktop || !current.failed) return;
    let probe: HTMLImageElement | undefined;
    const timer = setTimeout(() => {
      const attempt = current.attempt + 1;
      probe = new Image();
      probe.onload = () => setState({ key, attempt, failed: false });
      probe.onerror = () => setState({ key, attempt, failed: true });
      probe.src = iconUrl(item.id, quality, attempt);
    }, ICON_RETRY_MS);
    return () => {
      clearTimeout(timer);
      if (probe) probe.onload = probe.onerror = null;
    };
  }, [key, current.attempt, current.failed, item.id, quality]);
  const showImage = desktop && !current.failed;
  return (
    <span
      className={`item-icon ${item.tier ? `tier-${item.tier}` : ''} ${showImage ? 'with-image' : ''}`}
    >
      {showImage ? (
        <img
          src={iconUrl(item.id, quality, current.attempt)}
          alt=""
          loading="lazy"
          decoding="async"
          onError={() =>
            setState({ key, attempt: current.attempt, failed: true })
          }
        />
      ) : (
        <Boxes size={20} />
      )}
      <small>{item.tier ? `T${item.tier}` : '—'}</small>
    </span>
  );
}
