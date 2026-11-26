import { AlertTriangle, CheckCircle2, Info, ShieldBan } from "lucide-react";
import type { ReactNode } from "react";

export type AlertTone = "info" | "success" | "warning" | "danger";

const ALERT_STYLES: Record<AlertTone, { box: string; icon: ReactNode }> = {
  info: {
    box: "bg-accent-soft border-accent-border text-fg-primary",
    icon: <Info size={16} className="text-accent shrink-0 mt-0.5" />,
  },
  success: {
    box: "bg-success-soft border-success-border text-fg-primary",
    icon: <CheckCircle2 size={16} className="text-success shrink-0 mt-0.5" />,
  },
  warning: {
    box: "bg-warning-soft border-warning-border text-fg-primary",
    icon: <AlertTriangle size={16} className="text-warning shrink-0 mt-0.5" />,
  },
  danger: {
    box: "bg-danger-soft border-danger-border text-fg-primary",
    icon: <ShieldBan size={16} className="text-danger shrink-0 mt-0.5" />,
  },
};

export function AlertBanner({
  tone = "info",
  children,
  action,
  className = "",
}: {
  tone?: AlertTone;
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  const style = ALERT_STYLES[tone];
  return (
    <div
      role={tone === "danger" || tone === "warning" ? "alert" : "status"}
      aria-live={tone === "danger" || tone === "warning" ? "assertive" : "polite"}
      className={`rounded-[10px] border px-3.5 py-2.5 flex items-start justify-between gap-3 text-[12.5px] leading-relaxed ${style.box} ${className}`}
    >
      <div className="flex items-start gap-2.5 min-w-0">
        {style.icon}
        <div className="min-w-0">{children}</div>
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  fullHeight = false,
}: {
  icon?: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
  fullHeight?: boolean;
}) {
  return (
    <div
      className={`w-full flex items-center justify-center ${
        fullHeight ? "min-h-[calc(100vh-136px)]" : "py-10"
      }`}
    >
      <div className="neu-card rounded-[18px] w-full max-w-[460px] px-7 py-10 text-center flex flex-col items-center gap-2.5 animate-fade-up">
        {icon && (
          <div className="w-12 h-12 rounded-[14px] bg-accent-soft border border-accent-border grid place-items-center text-accent mb-1">
            {icon}
          </div>
        )}
        <h3 className="m-0 text-[15px] font-semibold text-fg-primary">{title}</h3>
        <p className="m-0 text-[13px] text-fg-secondary text-pretty leading-relaxed">
          {description}
        </p>
        {action && <div className="mt-2.5">{action}</div>}
      </div>
    </div>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return (
    <div aria-hidden="true" className={`rounded-[6px] bg-elevated animate-skeleton ${className}`} />
  );
}
