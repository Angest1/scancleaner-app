import { X } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "./Button";

interface DetailDrawerProps {
  title: string;
  subtitle?: string;
  badge?: ReactNode;
  onClose: () => void;
  footer?: ReactNode;
  children: ReactNode;
}

export function DetailDrawer({
  title,
  subtitle,
  badge,
  onClose,
  footer,
  children,
}: DetailDrawerProps) {
  const titleId = useId();
  const [closing, setClosing] = useState(false);
  const timerRef = useRef<number | undefined>(undefined);

  const requestClose = useCallback(() => {
    if (closing) return;
    setClosing(true);
    timerRef.current = window.setTimeout(() => {
      onClose();
    }, 160);
  }, [closing, onClose]);

  useEffect(() => {
    return () => {
      if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") requestClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [requestClose]);

  const content = (
    <div
      className={`fixed inset-0 z-100 flex justify-end bg-black/40 backdrop-blur-[3px] ${
        closing ? "animate-backdrop-out pointer-events-none" : "animate-backdrop-in"
      }`}
      onClick={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") requestClose();
      }}
    >
      <dialog
        open
        aria-modal="true"
        aria-labelledby={titleId}
        className={`relative m-0 p-0 border-0 border-l border-border-default text-fg-primary w-full max-w-[460px] h-full bg-tile flex flex-col overflow-hidden ${
          closing ? "animate-drawer-out" : "animate-drawer-in"
        }`}
        style={{ boxShadow: "var(--shadow-floating)" }}
      >
        <div className="px-5 py-4 border-b border-border-subtle flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <h2
                id={titleId}
                className="m-0 text-[15px] font-semibold text-fg-primary break-words"
              >
                {title}
              </h2>
              {badge}
            </div>
            {subtitle && <p className="m-0 mt-0.5 text-[12px] text-fg-secondary">{subtitle}</p>}
          </div>
          <Button variant="ghost" size="icon" onClick={requestClose} aria-label="Cerrar inspector">
            <X size={15} />
          </Button>
        </div>

        <div className="p-5 flex-1 overflow-auto overscroll-contain flex flex-col gap-4">
          {children}
        </div>

        {footer && (
          <div className="px-5 py-3.5 border-t border-border-subtle bg-elevated/70 flex items-center justify-end gap-2 flex-wrap">
            {footer}
          </div>
        )}
      </dialog>
    </div>
  );

  return typeof document !== "undefined" ? createPortal(content, document.body) : content;
}

export function DetailField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] font-medium text-fg-muted">{label}</span>
      <div className="text-[12.5px] text-fg-primary break-words">{children}</div>
    </div>
  );
}
