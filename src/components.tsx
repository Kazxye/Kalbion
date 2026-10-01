import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { ChevronDown, CircleAlert, CircleCheck, X } from 'lucide-react';

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

/** Modal dialog: focus moves in, Tab stays inside, focus returns to the opener on close. */
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
  const titleId = useId();
  // Captured on first render, before autoFocus moves focus into the dialog.
  const [opener] = useState(() => document.activeElement as HTMLElement | null);
  useEffect(() => {
    const panel = container.current;
    if (!panel) return;
    if (!panel.contains(document.activeElement)) {
      panel
        .querySelector<HTMLElement>(
          '.dialog-body input, .dialog-body select, .dialog-body textarea, .dialog-body button',
        )
        ?.focus();
    }
    const trap = (event: globalThis.KeyboardEvent) => {
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
      // The shell stops being inert in the same commit, so the opener can take focus again.
      if (opener?.isConnected) opener.focus();
    };
  }, [opener]);
  return (
    <div className="overlay" onClick={close}>
      <section
        ref={container}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <h2 id={titleId}>{title}</h2>
          <button className="ghost" aria-label="Fechar" onClick={close}>
            <X size={18} aria-hidden />
          </button>
        </header>
        <div className="dialog-body">{children}</div>
      </section>
    </div>
  );
}

export function InlineError({ message }: { message: string }) {
  if (!message) return null;
  return (
    <p role="alert" className="inline-error">
      <CircleAlert size={16} aria-hidden />
      <span>{message}</span>
    </p>
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
      <InlineError message={error} />
      <p>{confirmation.message}</p>
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

export type MenuEntry =
  | {
      label: string;
      icon?: ReactNode;
      onSelect: () => void;
      disabled?: boolean;
      /** Shown next to a disabled entry so the reason is visible, not only hovered. */
      hint?: string;
    }
  | 'separator';

/** Menu button following the WAI-ARIA menu pattern: arrows move, Escape closes. */
export function MenuButton({
  label,
  icon,
  entries,
  disabled = false,
}: {
  label: string;
  icon?: ReactNode;
  entries: MenuEntry[];
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const items = () =>
    Array.from(
      list.current?.querySelectorAll<HTMLButtonElement>(
        '[role="menuitem"]:not(:disabled)',
      ) ?? [],
    );
  useEffect(() => {
    if (!open) return;
    items()[0]?.focus();
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!list.current?.contains(target) && !button.current?.contains(target))
        setOpen(false);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);
  const close = () => {
    setOpen(false);
    button.current?.focus();
  };
  const onKeyDown = (event: KeyboardEvent) => {
    const all = items();
    const index = all.indexOf(document.activeElement as HTMLButtonElement);
    const move = (next: number) => {
      event.preventDefault();
      all[(next + all.length) % all.length]?.focus();
    };
    if (event.key === 'ArrowDown') move(index + 1);
    else if (event.key === 'ArrowUp') move(index - 1);
    else if (event.key === 'Home') move(0);
    else if (event.key === 'End') move(all.length - 1);
    else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === 'Tab') setOpen(false);
  };
  return (
    <div className="menu">
      <button
        ref={button}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown') {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        {icon}
        {label}
        <ChevronDown size={14} aria-hidden />
      </button>
      {open && (
        <div
          ref={list}
          id={menuId}
          role="menu"
          aria-label={label}
          className="menu-list"
          onKeyDown={onKeyDown}
        >
          {entries.map((entry, index) =>
            entry === 'separator' ? (
              <div role="separator" key={`separator-${index}`} />
            ) : (
              <button
                role="menuitem"
                key={entry.label}
                disabled={entry.disabled}
                onClick={() => {
                  close();
                  entry.onSelect();
                }}
              >
                {entry.icon}
                {entry.label}
                {entry.disabled && entry.hint && (
                  <span className="hint">{entry.hint}</span>
                )}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  children,
  action,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty" role="status">
      {icon}
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}

export function Notices({
  error,
  notice,
  retry,
  dismissNotice,
}: {
  error: string;
  notice: string;
  retry: () => void;
  dismissNotice: () => void;
}) {
  if (!error && !notice) return null;
  return (
    <div className="notices">
      {error && (
        <div role="alert" className="notice error">
          <CircleAlert size={16} aria-hidden />
          <span>{error}</span>
          <button className="small" onClick={retry}>
            Tentar novamente
          </button>
        </div>
      )}
      {notice && (
        <div role="status" className="notice success">
          <CircleCheck size={16} aria-hidden />
          <span>{notice}</span>
          <button
            className="ghost small"
            aria-label="Dispensar aviso"
            onClick={dismissNotice}
          >
            <X size={14} aria-hidden />
          </button>
        </div>
      )}
    </div>
  );
}
