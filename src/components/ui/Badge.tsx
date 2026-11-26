import type { HTMLAttributes, ReactNode } from "react";

export type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
  children: ReactNode;
}

const TONE_CLASSES: Record<BadgeTone, string> = {
  neutral: "bg-elevated text-fg-secondary border-border-default",
  accent: "bg-accent-soft text-accent border-accent-border",
  success: "bg-success-soft text-success-text border-success-border",
  warning: "bg-warning-soft text-warning-text border-warning-border",
  danger: "bg-danger-soft text-danger-text border-danger-border",
};

export function Badge({ tone = "neutral", className = "", children, ...props }: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full border text-[11px] font-medium leading-4 whitespace-nowrap ${TONE_CLASSES[tone]} ${className}`}
      {...props}
    >
      {children}
    </span>
  );
}
