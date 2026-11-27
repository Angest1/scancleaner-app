import { ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { formatBytes } from "../../lib/format";
import { Button } from "./Button";

interface SelectionActionBarProps {
  count: number;
  bytes: number;
  noun: string;
  actionLabel: string;
  onClear: () => void;
  onAction: () => void;
}

export function SelectionActionBar({
  count,
  bytes,
  noun,
  actionLabel,
  onClear,
  onAction,
}: SelectionActionBarProps) {
  const open = count > 0;
  const [mounted, setMounted] = useState(open);
  const [visible, setVisible] = useState(open);
  const [snapshot, setSnapshot] = useState({ count, bytes });

  useEffect(() => {
    if (count > 0) setSnapshot({ count, bytes });
  }, [count, bytes]);

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
    <div
      data-open={visible ? "true" : "false"}
      className="sc-presence-bar sticky bottom-3 z-20 rounded-[12px] bg-tile/95 backdrop-blur-md border border-accent-border shadow-[var(--shadow-floating)] px-4 py-2.5 flex items-center justify-between gap-3 flex-wrap"
    >
      <div className="text-[12.5px] text-fg-secondary">
        <strong className="text-fg-primary mono">{snapshot.count}</strong> {noun} ·{" "}
        <strong className="text-accent mono">{formatBytes(snapshot.bytes)}</strong> listos para
        Dry-run
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm" variant="ghost" onClick={onClear}>
          Deseleccionar todo
        </Button>
        <Button variant="primary" size="sm" onClick={onAction}>
          <ShieldCheck size={13} />
          {actionLabel}
        </Button>
      </div>
    </div>
  );
}
