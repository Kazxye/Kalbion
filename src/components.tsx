import { useEffect, useRef, type ReactNode } from 'react';
import { X } from 'lucide-react';

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
