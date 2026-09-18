import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ButtonHTMLAttributes, Dispatch, SetStateAction } from "react";

export function StudyStartButton({
  hint,
  activeHint,
  onHintChange,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  hint: string;
  activeHint: string | null;
  onHintChange: Dispatch<SetStateAction<string | null>>;
  children: string;
}) {
  const id = useId();
  const trigger = useRef<HTMLSpanElement>(null);
  const bubble = useRef<HTMLSpanElement>(null);
  const focused = useRef(false);
  const closeTimer = useRef<number | undefined>(undefined);
  const open = activeHint === id;
  const [position, setPosition] = useState({ left: 12, top: 12 });

  function show() {
    window.clearTimeout(closeTimer.current);
    onHintChange(id);
  }
  function enter() {
    // Hiding a portal can expose another button beneath the cursor. After
    // Escape, wait for actual pointer movement or focus before reopening.
    if (activeHint !== "dismissed") show();
  }
  function hideLater() {
    window.clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(() => {
      if (!focused.current)
        onHintChange((current) => (current === id ? null : current));
    }, 120);
  }
  useEffect(() => () => window.clearTimeout(closeTimer.current), []);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      if (!trigger.current || !bubble.current) return;
      const button = trigger.current.getBoundingClientRect();
      const tip = bubble.current.getBoundingClientRect();
      const left = Math.max(
        12,
        Math.min(
          button.left + (button.width - tip.width) / 2,
          window.innerWidth - tip.width - 12,
        ),
      );
      const above = button.top - tip.height - 8;
      const top =
        above >= 12
          ? above
          : Math.min(button.bottom + 8, window.innerHeight - tip.height - 12);
      setPosition({ left, top: Math.max(12, top) });
    };
    const dismiss = (event: KeyboardEvent) => {
      if (event.key === "Escape")
        onHintChange((current) => (current === id ? "dismissed" : current));
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    document.addEventListener("keydown", dismiss);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      document.removeEventListener("keydown", dismiss);
    };
  }, [id, onHintChange, open]);

  return (
    <span
      className="study-start-button"
      ref={trigger}
      tabIndex={props.disabled ? 0 : undefined}
      aria-label={props.disabled ? children : undefined}
      aria-describedby={props.disabled ? id : undefined}
      onPointerEnter={enter}
      onPointerMove={show}
      onPointerLeave={hideLater}
      onFocus={() => {
        focused.current = true;
        show();
      }}
      onBlur={() => {
        focused.current = false;
        hideLater();
      }}
      onClickCapture={() =>
        onHintChange((current) => (current === id ? null : current))
      }
    >
      <button {...props} aria-describedby={id}>
        {children}
      </button>
      {createPortal(
        <span
          ref={bubble}
          id={id}
          role="tooltip"
          hidden={!open}
          className="study-start-tooltip"
          style={position}
          onPointerEnter={enter}
          onPointerMove={show}
          onPointerLeave={hideLater}
        >
          {hint}
        </span>,
        document.body,
      )}
    </span>
  );
}
