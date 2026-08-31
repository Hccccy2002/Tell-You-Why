import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";

interface Props {
  id: string;
  eyebrow?: string;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  busyLabel: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmationDialog({
  id,
  eyebrow = "操作确认",
  title,
  children,
  confirmLabel,
  busyLabel,
  busy,
  onCancel,
  onConfirm,
}: Props) {
  const dialogRef = useRef<HTMLElement>(null);
  const focusRestoreFrameRef = useRef<number | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  );
  const busyRef = useRef(busy);
  const onCancelRef = useRef(onCancel);

  useEffect(() => {
    busyRef.current = busy;
    onCancelRef.current = onCancel;
  }, [busy, onCancel]);

  useEffect(() => {
    if (focusRestoreFrameRef.current != null) {
      cancelAnimationFrame(focusRestoreFrameRef.current);
      focusRestoreFrameRef.current = null;
    }
    const appRoot = document.getElementById("root");
    const previousFocus = previousFocusRef.current;
    const rootWasInert = appRoot?.inert ?? false;
    const previousBodyOverflow = document.body.style.overflow;
    if (appRoot) appRoot.inert = true;
    document.body.style.overflow = "hidden";

    function containKeyboardFocus(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (!busyRef.current) onCancelRef.current();
        return;
      }
      if (event.key !== "Tab") return;

      const focusable = Array.from(
        dialogRef.current?.querySelectorAll<HTMLElement>(
          "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
        ) ?? [],
      );
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) {
        event.preventDefault();
        return;
      }
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", containKeyboardFocus, true);
    return () => {
      document.removeEventListener("keydown", containKeyboardFocus, true);
      if (appRoot) appRoot.inert = rootWasInert;
      document.body.style.overflow = previousBodyOverflow;
      if (previousFocus?.isConnected) {
        focusRestoreFrameRef.current = requestAnimationFrame(() => {
          focusRestoreFrameRef.current = null;
          if (previousFocus.isConnected) {
            previousFocus.focus({ preventScroll: true });
          }
        });
      }
    };
  }, []);

  return createPortal(
    <div
      className="confirmation-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <section
        ref={dialogRef}
        className="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-busy={busy}
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-description`}
      >
        <span className="eyebrow">{eyebrow}</span>
        <h2 id={`${id}-title`}>{title}</h2>
        <div id={`${id}-description`} className="confirmation-description">
          {children}
        </div>
        <div className="confirmation-actions">
          <button
            className="confirmation-cancel"
            disabled={busy}
            autoFocus
            onClick={onCancel}
          >
            取消
          </button>
          <button
            className="confirmation-danger"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? busyLabel : confirmLabel}
          </button>
        </div>
      </section>
    </div>,
    document.body,
  );
}
