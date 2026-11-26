import { X } from "lucide-react";
import { type ReactNode, useEffect, useId } from "react";
import { createPortal } from "react-dom";
import { Button } from "./Button";

interface ModalProps {
  title: ReactNode;
  subtitle?: string;
  maxWidthClass?: string;
  onClose: () => void;
  footer?: ReactNode;
  children: ReactNode;
}

export function Modal({
  title,
  subtitle,
  maxWidthClass = "max-w-[760px]",
  onClose,
  footer,
  children,
}: ModalProps) {
  const titleId = useId();

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const content = (
    <div
      className="fixed inset-0 z-100 flex items-center justify-center bg-black/50 backdrop-blur-[4px] p-4 sm:p-6 animate-backdrop-in"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <dialog
        open
        aria-modal="true"
        aria-labelledby={titleId}
        className={`relative mx-auto my-0 p-0 border-0 text-fg-primary w-full ${maxWidthClass} max-h-[86vh] bg-tile rounded-[16px] flex flex-col overflow-hidden animate-modal-in`}
        style={{ boxShadow: "var(--shadow-floating)" }}
      >
        <div className="px-5 py-3.5 border-b border-border-subtle flex items-center justify-between gap-3">
          <div className="min-w-0">
            <h2 id={titleId} className="m-0 text-[15px] font-semibold text-fg-primary truncate">
              {title}
            </h2>
            {subtitle && <p className="m-0 mt-0.5 text-[12px] text-fg-secondary">{subtitle}</p>}
          </div>
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="Cerrar diálogo">
            <X size={15} />
          </Button>
        </div>

        <div className="p-5 overflow-auto overscroll-contain flex flex-col gap-3.5">{children}</div>

        {footer && (
          <div className="px-5 py-3 border-t border-border-subtle bg-elevated/70 flex items-center justify-end gap-2">
            {footer}
          </div>
        )}
      </dialog>
    </div>
  );

  return typeof document !== "undefined" ? createPortal(content, document.body) : content;
}
