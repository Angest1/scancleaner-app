import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { formatMiddleEllipsis } from "../../lib/format";

interface PathDisplayProps {
  path: string;
  maxChars?: number;
  className?: string;
  showCopy?: boolean;
}

export function PathDisplay({
  path,
  maxChars = 54,
  className = "",
  showCopy = true,
}: PathDisplayProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard?.writeText(path).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    });
  };

  return (
    <span className={`group/path inline-flex items-center gap-1 min-w-0 max-w-full ${className}`}>
      <span className="mono truncate" title={path}>
        {formatMiddleEllipsis(path, maxChars)}
      </span>
      {showCopy && (
        <button
          type="button"
          onClick={handleCopy}
          title={copied ? "Ruta copiada" : "Copiar ruta completa"}
          aria-label={`Copiar ruta ${path}`}
          className="inline-flex items-center justify-center w-5 h-5 rounded-[5px] bg-transparent border-0 p-0 text-fg-muted hover:text-accent hover:bg-elevated cursor-pointer shrink-0 opacity-0 group-hover/path:opacity-100 focus-visible:opacity-100 transition-opacity duration-140"
        >
          {copied ? <Check size={11} className="text-success" /> : <Copy size={11} />}
        </button>
      )}
    </span>
  );
}
