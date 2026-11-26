import type { ReactNode } from "react";

interface CollapseProps {
  open: boolean;
  children: ReactNode;
  className?: string;
}

export function Collapse({ open, children, className = "" }: CollapseProps) {
  return (
    <div
      className={`collapse-grid ${className}`}
      data-open={open ? "true" : "false"}
      aria-hidden={!open}
    >
      <div className="collapse-inner">{children}</div>
    </div>
  );
}
