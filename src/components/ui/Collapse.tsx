import { type ReactNode, useEffect, useState } from "react";

interface CollapseProps {
  open: boolean;
  children: ReactNode;
  className?: string;
}

/**
 * Smooth CSS grid-row accordion (`0fr` <-> `1fr` + `opacity`) that retains
 * its previous children during the 240ms close transition so conditional
 * content (`open && <Banner />`) never vanishes abruptly on exit.
 */
export function Collapse({ open, children, className = "" }: CollapseProps) {
  const [rendered, setRendered] = useState<ReactNode>(open ? children : null);
  const [isOpen, setIsOpen] = useState(open);

  useEffect(() => {
    if (open) {
      setRendered(children);
      const raf = window.requestAnimationFrame(() => setIsOpen(true));
      return () => window.cancelAnimationFrame(raf);
    }
    setIsOpen(false);
    const t = window.setTimeout(() => setRendered(null), 250);
    return () => window.clearTimeout(t);
  }, [open, children]);

  return (
    <div
      className={`collapse-grid ${className}`}
      data-open={isOpen ? "true" : "false"}
      aria-hidden={!isOpen}
    >
      <div className="collapse-inner">{open ? children : rendered}</div>
    </div>
  );
}
