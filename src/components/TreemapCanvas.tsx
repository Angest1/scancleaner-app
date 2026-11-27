import { FolderOpen, Lock, ShieldPlus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { formatBytes } from "../lib/format";
import type { TreemapLayoutRectDto, TreemapNodeDto } from "../lib/ipc";
import { type TreemapRect, layoutSquarifiedTreemap } from "../lib/treemapLayout";
import { type PositionedTreemapNode, renderTreemapCanvas } from "./treemapRenderer";

interface TreemapCanvasProps {
  nodes?: TreemapNodeDto[];
  nativeRects?: TreemapLayoutRectDto[];
  loading?: boolean;
  colorBy: "type" | "origin";
  onViewportChange?: (width: number, height: number) => void;
  onDrillDown: (nodeId: number) => void;
  onOpenExplorer: (path: string) => void;
  onSendToCleanup: (node: TreemapNodeDto) => void;
  onProtectPath: (node: TreemapNodeDto) => void;
}

const TOOLTIP_MAX_W = 264;
const TOOLTIP_EST_H = 96;
const TOOLTIP_OFFSET = 14;
const MENU_EST_W = 220;
const MENU_EST_H = 132;

export function TreemapCanvas({
  nodes = [],
  nativeRects,
  loading = false,
  colorBy,
  onViewportChange,
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
    rect: TreemapRect<PositionedTreemapNode>;
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
        onViewportChange?.(w, h);
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, [onViewportChange]);

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

  const rects = useMemo<TreemapRect<PositionedTreemapNode>[]>(() => {
    if (nativeRects && nativeRects.length > 0) {
      // Proportionally fit native layout rects to current canvas dimensions so
      // resizes or initial measurements NEVER leave an empty gap on any edge.
      let maxRight = 0;
      let maxBottom = 0;
      for (const r of nativeRects) {
        if (r.x + r.w > maxRight) maxRight = r.x + r.w;
        if (r.y + r.h > maxBottom) maxBottom = r.y + r.h;
      }
      const scaleX = maxRight > 0 ? size.width / maxRight : 1;
      const scaleY = maxBottom > 0 ? size.height / maxBottom : 1;

      return nativeRects.map((r) => ({
        item: {
          ...r,
          value: r.allocatedBytes,
        },
        x: r.x * scaleX,
        y: r.y * scaleY,
        w: r.w * scaleX,
        h: r.h * scaleY,
      }));
    }
    const input: PositionedTreemapNode[] = nodes
      .filter((n) => n.allocatedBytes > 0)
      .map((n, i) => ({
        ...n,
        id: `${n.nodeId ?? "other"}-${i}`,
        value: n.allocatedBytes,
      }));
    return layoutSquarifiedTreemap(input, size.width, size.height);
  }, [nativeRects, nodes, size.width, size.height]);

  useEffect(() => {
    void themeTick;
    const canvas = canvasRef.current;
    if (!canvas) return;
    renderTreemapCanvas(canvas, rects, size, hovered?.rect.item.id ?? null, colorBy);
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

  const showSkeleton = rects.length === 0;
  const isUpdating = loading && rects.length > 0;

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
        className={`block transition-[opacity,transform] duration-220 ease-out ${
          showSkeleton
            ? "opacity-0 scale-[0.992]"
            : isUpdating
              ? "opacity-70 scale-100"
              : "opacity-100 scale-100"
        } ${hovered ? "cursor-pointer" : "cursor-default"}`}
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

      {/* Animated Bento skeleton overlay (prevents blank canvas / abrupt pop-in) */}
      <div
        aria-hidden={!showSkeleton}
        className={`absolute inset-0 p-2 grid grid-cols-12 grid-rows-6 gap-1.5 bg-canvas transition-opacity duration-200 ease-out pointer-events-none ${
          showSkeleton ? "opacity-100" : "opacity-0"
        }`}
      >
        <div className="col-span-6 row-span-4 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton" />
        <div
          className="col-span-6 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "90ms" }}
        />
        <div
          className="col-span-3 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "160ms" }}
        />
        <div
          className="col-span-3 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "220ms" }}
        />
        <div
          className="col-span-4 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "120ms" }}
        />
        <div
          className="col-span-5 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "200ms" }}
        />
        <div
          className="col-span-3 row-span-2 rounded-[7px] bg-elevated border border-border-subtle animate-skeleton"
          style={{ animationDelay: "260ms" }}
        />
      </div>

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
