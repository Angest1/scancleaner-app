import { X } from "lucide-react";
import { useEffect, useState } from "react";

interface ToastOverlayProps {
  message: string | null;
  onDismiss: () => void;
}

export function ToastOverlay({ message, onDismiss }: ToastOverlayProps) {
  const open = Boolean(message);
  const [mounted, setMounted] = useState(open);
  const [visible, setVisible] = useState(open);
  const [lastMsg, setLastMsg] = useState<string>("");

  useEffect(() => {
    if (message) setLastMsg(message);
  }, [message]);

  useEffect(() => {
    if (open) {
      setMounted(true);
      const raf = window.requestAnimationFrame(() => setVisible(true));
      return () => window.cancelAnimationFrame(raf);
    }
    setVisible(false);
    const t = window.setTimeout(() => setMounted(false), 170);
    return () => window.clearTimeout(t);
  }, [open]);

  if (!mounted) return null;

  return (
    <output
      aria-live="polite"
      data-open={visible ? "true" : "false"}
      className="sc-presence-bar fixed bottom-5 right-5 z-90 max-w-[380px] rounded-[12px] bg-tile/95 backdrop-blur-md border border-border-default px-4 py-2.5 text-[12.5px] text-fg-primary shadow-[var(--shadow-floating)] flex items-center gap-3"
    >
      <span className="flex-1">{lastMsg}</span>
      <button
        type="button"
        onClick={onDismiss}
        aria-label="Cerrar notificación"
        className="text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0.5 cursor-pointer"
      >
        <X size={13} />
      </button>
    </output>
  );
}
