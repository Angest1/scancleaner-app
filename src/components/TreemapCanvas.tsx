import { FolderOpen, Lock, ShieldPlus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { colorForCategory, formatBytes } from "../lib/format";
import type { TreemapNodeDto } from "../lib/ipc";
import { type TreemapRect, layoutSquarifiedTreemap } from "../lib/treemapLayout";

interface TreemapCanvasProps {
  nodes: TreemapNodeDto[];
  colorBy: "type" | "origin";
  onDrillDown: (nodeId: number) => void;
  onOpenExplorer: (path: string) => void;
  onSendToCleanup: (node: TreemapNodeDto) => void;
  onProtectPath: (node: TreemapNodeDto) => void;
}

interface PositionedNode extends TreemapNodeDto {
  id: string;
  value: number;
}

const TOOLTIP_MAX_W = 264;
const TOOLTIP_EST_H = 96;
const TOOLTIP_OFFSET = 14;
const MENU_EST_W = 220;
const MENU_EST_H = 132;

export function TreemapCanvas({
  nodes,
  colorBy,
  onDrillDown,
  onOpenExplorer,
  onSendToCleanup,
  onProtectPath,
}: TreemapCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 800, height: 400 });
  const [themeTick, setThemeTick] = useState(0);
  const [hovered, setHovered] = useState<{
    rect: TreemapRect<PositionedNode>;
    localX: number;
    localY: number;
  } | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    node: TreemapNodeDto;
    x: number;
    y: number;
  } | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const w = Math.max(240, Math.floor(entry.contentRect.width));
        const h = Math.max(260, Math.floor(entry.contentRect.height));
        setSize({ width: w, height: h });
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  // Re-render canvas when user toggles light/dark theme
  useEffect(() => {
    const obs = new MutationObserver(() => setThemeTick((t) => t + 1));
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    const closeMenu = () => setContextMenu(null);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setContextMenu(null);
    };
    window.addEventListener("click", closeMenu);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  const rects = useMemo(() => {
    const input: PositionedNode[] = nodes
      .filter((n) => n.allocatedBytes > 0)
      .map((n, i) => ({
        ...n,
        id: `${n.nodeId ?? "other"}-${i}`,
        value: n.allocatedBytes,
      }));
    return layoutSquarifiedTreemap(input, size.width, size.height);
  }, [nodes, size.width, size.height]);

  useEffect(() => {
    void themeTick;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.width * dpr;
    canvas.height = size.height * dpr;
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const styles = getComputedStyle(document.documentElement);
    const canvasBg = styles.getPropertyValue("--treemap-bg").trim() || "#0a0c13";
    const sepStroke = styles.getPropertyValue("--treemap-sep").trim() || "rgba(10, 12, 19, 0.9)";
    const hoverStroke = styles.getPropertyValue("--treemap-hover-stroke").trim() || "#f1f3fa";

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.width, size.height);
    ctx.fillStyle = canvasBg;
    ctx.fillRect(0, 0, size.width, size.height);

    for (const r of rects) {
      const pad = 1.5;
      const rx = r.x + pad;
      const ry = r.y + pad;
      const rw = Math.max(0, r.w - pad * 2);
      const rh = Math.max(0, r.h - pad * 2);
      if (rw <= 1 || rh <= 1) continue;

      const isHovered = hovered?.rect.item.id === r.item.id;
      const catKey = colorBy === "type" ? r.item.typeCat : r.item.originCat;
      const baseColor = r.item.isOtherBucket ? "#576175" : colorForCategory(catKey);

      ctx.fillStyle = baseColor;
      ctx.globalAlpha = isHovered ? 0.98 : 0.86;
      ctx.fillRect(rx, ry, rw, rh);

      ctx.globalAlpha = 1;
      ctx.strokeStyle = isHovered ? hoverStroke : sepStroke;
      ctx.lineWidth = isHovered ? 2 : 1;
      ctx.strokeRect(rx + 0.5, ry + 0.5, Math.max(0, rw - 1), Math.max(0, rh - 1));

      if (r.item.isProtected && rw > 18 && rh > 18) {
        ctx.fillStyle = "#e09f3e";
        ctx.beginPath();
        ctx.moveTo(rx + rw, ry);
        ctx.lineTo(rx + rw - 10, ry);
        ctx.lineTo(rx + rw, ry + 10);
        ctx.closePath();
        ctx.fill();
      }

      if (rw > 68 && rh > 28) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(rx + 5, ry + 4, rw - 10, rh - 8);
        ctx.clip();

        ctx.fillStyle = "#f8fafc";
        ctx.font = '600 12px "Geist Sans", "Segoe UI", sans-serif';
        const suffix = r.item.kind === "dir" ? "/" : "";
        ctx.fillText(`${r.item.name}${suffix}`, rx + 7, ry + 17);

        if (rh > 42) {
          ctx.fillStyle = "rgba(238, 242, 247, 0.86)";
          ctx.font = '500 11px "JetBrains Mono", Consolas, monospace';
          ctx.fillText(formatBytes(r.item.allocatedBytes), rx + 7, ry + 33);
        }
        ctx.restore();
      }
    }
  }, [rects, size, hovered, colorBy, themeTick]);

  const hitTest = (clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const bounds = canvas.getBoundingClientRect();
    const localX = clientX - bounds.left;
    const localY = clientY - bounds.top;
    const rect =
      rects.find(
        (r) => localX >= r.x && localX <= r.x + r.w && localY >= r.y && localY <= r.y + r.h,
      ) ?? null;
    return rect ? { rect, localX, localY } : null;
  };

  // Smart container-relative tooltip clamping & edge flipping (never leaves canvas)
  const tooltipPos = useMemo(() => {
    if (!hovered) return null;
    const pad = 8;
    const estH = hovered.rect.item.isProtected ? TOOLTIP_EST_H + 22 : TOOLTIP_EST_H;

    let left = hovered.localX + TOOLTIP_OFFSET;
    if (left + TOOLTIP_MAX_W + pad > size.width) {
      left = hovered.localX - TOOLTIP_MAX_W - TOOLTIP_OFFSET;
    }
    left = Math.max(pad, Math.min(left, size.width - TOOLTIP_MAX_W - pad));

    let top = hovered.localY + TOOLTIP_OFFSET;
    if (top + estH + pad > size.height) {
      top = hovered.localY - estH - TOOLTIP_OFFSET;
    }
    top = Math.max(pad, Math.min(top, size.height - estH - pad));

    return { left, top };
  }, [hovered, size.width, size.height]);

  const menuPortal =
    contextMenu && typeof document !== "undefined"
      ? createPortal(
          <div
            role="menu"
            className="fixed z-120 min-w-[210px] rounded-[11px] bg-tile border border-border-default p-1.5 animate-popover-in"
            style={{
              left: Math.max(8, Math.min(contextMenu.x, window.innerWidth - MENU_EST_W - 8)),
              top: Math.max(8, Math.min(contextMenu.y, window.innerHeight - MENU_EST_H - 8)),
              boxShadow: "var(--shadow-floating)",
            }}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Escape") setContextMenu(null);
            }}
          >
            <button
              type="button"
              role="menuitem"
              className="flex items-center gap-2 w-full px-2.5 py-2 rounded-[7px] text-[12.5px] text-fg-primary text-left cursor-pointer hover:bg-elevated transition-colors duration-140"
              onClick={() => {
                onOpenExplorer(contextMenu.node.fullPath);
                setContextMenu(null);
              }}
            >
              <FolderOpen size={14} className="text-fg-secondary" />
              Abrir en Explorador
            </button>
            <button
              type="button"
              role="menuitem"
              disabled={contextMenu.node.isProtected}
              title={
                contextMenu.node.isProtected
                  ? (contextMenu.node.protectionReason ?? "Ruta protegida")
                  : "Enviar a Limpieza segura"
              }
              className="flex items-center gap-2 w-full px-2.5 py-2 rounded-[7px] text-[12.5px] text-fg-primary text-left cursor-pointer hover:bg-elevated transition-colors duration-140 disabled:opacity-45 disabled:cursor-not-allowed"
              onClick={() => {
                if (contextMenu.node.isProtected) return;
                onSendToCleanup(contextMenu.node);
                setContextMenu(null);
              }}
            >
              {contextMenu.node.isProtected ? (
                <Lock size={14} className="text-warning" />
              ) : (
                <Trash2 size={14} className="text-danger" />
              )}
              {contextMenu.node.isProtected ? "Eliminación bloqueada" : "Enviar a Limpieza segura"}
            </button>
            {!contextMenu.node.isProtected && (
              <button
                type="button"
                role="menuitem"
                className="flex items-center gap-2 w-full px-2.5 py-2 rounded-[7px] text-[12.5px] text-fg-primary text-left cursor-pointer hover:bg-elevated transition-colors duration-140"
                onClick={() => {
                  onProtectPath(contextMenu.node);
                  setContextMenu(null);
                }}
              >
                <ShieldPlus size={14} className="text-accent" />
                Proteger esta ruta
              </button>
            )}
          </div>,
          document.body,
        )
      : null;

  return (
    <div
      ref={containerRef}
      className="relative w-full h-[380px] sm:h-[420px] rounded-[10px] overflow-hidden border border-border-default bg-canvas"
    >
      <canvas
        ref={canvasRef}
        tabIndex={0}
        aria-label="Mapa de bloques de espacio en disco"
        onKeyDown={(e) => {
          if (e.key === "Escape") setContextMenu(null);
        }}
        className={`block ${hovered ? "cursor-pointer" : "cursor-default"}`}
        onMouseMove={(e) => {
          const hit = hitTest(e.clientX, e.clientY);
          setHovered(hit);
        }}
        onMouseLeave={() => setHovered(null)}
        onClick={(e) => {
          const hit = hitTest(e.clientX, e.clientY);
          if (hit && hit.rect.item.kind === "dir" && hit.rect.item.nodeId !== null) {
            onDrillDown(hit.rect.item.nodeId);
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          const hit = hitTest(e.clientX, e.clientY);
          if (hit && !hit.rect.item.isOtherBucket) {
            setContextMenu({ node: hit.rect.item, x: e.clientX, y: e.clientY });
          }
        }}
      />

      {hovered && tooltipPos && (
        <div
          className="absolute z-40 rounded-[10px] bg-tile/96 border border-border-default px-3 py-2 pointer-events-none backdrop-blur-md transition-[transform] duration-75"
          style={{
            left: tooltipPos.left,
            top: tooltipPos.top,
            width: TOOLTIP_MAX_W,
            boxShadow: "var(--shadow-floating)",
          }}
        >
          <div className="font-semibold text-[12.5px] text-fg-primary break-words leading-snug">
            {hovered.rect.item.name}
          </div>
          <div className="mono text-[12px] text-accent mt-0.5">
            {formatBytes(hovered.rect.item.allocatedBytes)} (
            {hovered.rect.item.percentage.toFixed(1)}%)
          </div>
          <div className="text-[11.5px] text-fg-secondary mt-0.5 tabular">
            {hovered.rect.item.kind === "dir"
              ? `${hovered.rect.item.fileCount.toLocaleString()} archivos · Clic para entrar`
              : "Archivo · Clic derecho para acciones"}
          </div>
          {hovered.rect.item.isProtected && (
            <div className="flex items-center gap-1 text-[11.5px] text-warning-text mt-1">
              <Lock size={11} className="shrink-0" />
              <span className="truncate">
                Protegida: {hovered.rect.item.protectionReason ?? "En lista negra"}
              </span>
            </div>
          )}
        </div>
      )}

      {menuPortal}
    </div>
  );
}
