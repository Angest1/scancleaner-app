import { Lock } from "lucide-react";
import { formatBytes, formatUnixTs } from "../../lib/format";
import type { CleanCandidate } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { PathDisplay } from "../ui/PathDisplay";

interface CleanCandidateRowProps {
  item: CleanCandidate;
  color: string;
  checked: boolean;
  locked: boolean;
  onToggle: (item: CleanCandidate) => void;
  onRelaunchAdmin: () => void;
}

export function CleanCandidateRow({
  item,
  color,
  checked,
  locked,
  onToggle,
  onRelaunchAdmin,
}: CleanCandidateRowProps) {
  return (
    <div
      className={`flex items-center justify-between gap-3 px-3.5 py-2.5 transition-colors duration-140 ${
        checked ? "bg-accent-soft/35" : "hover:bg-elevated/55"
      } ${locked ? "opacity-65" : ""}`}
    >
      <label
        className={`flex items-start gap-2.5 flex-1 min-w-0 ${
          locked ? "cursor-not-allowed" : "cursor-pointer"
        }`}
      >
        <input
          type="checkbox"
          className="mt-0.5 accent-accent shrink-0"
          checked={checked}
          disabled={locked}
          onChange={() => onToggle(item)}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 min-w-0">
            <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: color }} />
            <span className="font-medium text-[13px] text-fg-primary truncate">
              {item.displayName}
            </span>
          </div>
          <PathDisplay path={item.path} maxChars={64} className="text-[11.5px] text-fg-muted" />
          {item.warning && (
            <div className="text-[11.5px] text-warning-text mt-0.5">{item.warning}</div>
          )}
        </div>
      </label>

      <div className="flex items-center gap-2.5 shrink-0">
        {item.lastModifiedTs > 0 && (
          <span className="hidden sm:inline text-[11.5px] text-fg-muted tabular">
            {formatUnixTs(item.lastModifiedTs)}
          </span>
        )}
        <span className="mono font-medium text-[12.5px] text-fg-primary min-w-[78px] text-right">
          {formatBytes(item.sizeBytes)}
        </span>
        {locked && (
          <Button variant="warning" size="sm" onClick={onRelaunchAdmin}>
            <Lock size={12} />
            Admin
          </Button>
        )}
      </div>
    </div>
  );
}
