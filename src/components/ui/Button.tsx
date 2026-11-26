import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "default" | "primary" | "ghost" | "warning" | "danger";
export type ButtonSize = "sm" | "md" | "icon";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  children?: ReactNode;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  default:
    "bg-tile border-border-default text-fg-primary shadow-[0_2px_8px_-3px_rgba(32,28,76,0.12)] hover:bg-elevated hover:border-border-strong",
  primary:
    "bg-accent-strong border-accent/60 text-white shadow-[0_6px_16px_-6px_rgba(59,110,245,0.55)] hover:bg-accent-hover hover:border-accent",
  ghost:
    "bg-transparent border-transparent text-fg-secondary hover:bg-elevated hover:text-fg-primary",
  warning:
    "bg-warning-soft border-warning-border text-warning-text hover:bg-warning/20 hover:border-warning/60",
  danger:
    "bg-danger-soft border-danger-border text-danger-text hover:bg-danger/20 hover:border-danger/60",
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "h-7.5 px-2.5 text-[12px] gap-1.5 rounded-[8px]",
  md: "h-8.5 px-3.5 text-[12.5px] gap-1.5 rounded-[9px]",
  icon: "h-8 w-8 p-0 rounded-[9px]",
};

export function Button({
  variant = "default",
  size = "md",
  type = "button",
  className = "",
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`inline-flex items-center justify-center border font-medium leading-none select-none cursor-pointer transition-[background-color,border-color,color,transform,box-shadow] duration-140 ease-out active:scale-[0.97] disabled:opacity-45 disabled:cursor-not-allowed disabled:active:scale-100 ${VARIANT_CLASSES[variant]} ${SIZE_CLASSES[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}
