import { FolderOpen, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { colorForCategory, formatBytes } from "../lib/format";
import type { TreemapNodeDto } from "../lib/ipc";
import { type TreemapRect, layoutSquarifiedTreemap } from "../lib/treemapLayout";

interface TreemapCanvasProps {
  nodes: TreemapNodeDto[];
  colorBy: "type" | "origin";
  onDrillDown: (nodeId: number) => void;
  onOpenExplorer: (path: string) => void;
  onSendToCleanup: (node: TreemapNodeDto) => void;
}

interface PositionedNode extends TreemapNodeDto {
  id: string;
  value: number;
}

export function TreemapCanvas({
  nodes,
  colorBy,
  onDrillDown,
  onOpenExplorer,
  onSendToCleanup,
}: TreemapCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 800, height: 400 });
  const [hovered, setHovered] = useState<{
    rect: TreemapRect<PositionedNode>;
    x: number;
    y: number;
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

  useEffect(() => {
    const closeMenu = () => setContextMenu(null);
    window.addEventListener("click", closeMenu);
    return () => window.removeEventListener("click", closeMenu);
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
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.width * dpr;
    canvas.height = size.height * dpr;
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size.width, size.height);
    ctx.fillStyle = "#0b1120";
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
      const baseColor = r.item.isOtherBucket ? "#334155" : colorForCategory(catKey);

      ctx.fillStyle = baseColor;
      ctx.globalAlpha = isHovered ? 0.96 : 0.78;
      ctx.fillRect(rx, ry, rw, rh);

      ctx.globalAlpha = 1;
      ctx.strokeStyle = isHovered ? "#ffffff" : "#0b1120";
      ctx.lineWidth = isHovered ? 2 : 1;
      ctx.strokeRect(rx, ry, rw, rh);

      if (rw > 68 && rh > 28) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(rx + 4, ry + 3, rw - 8, rh - 6);
        ctx.clip();

        ctx.fillStyle = "#f8fafc";
        ctx.font = "600 12px Inter, Segoe UI, sans-serif";
        const titlePrefix = r.item.kind === "dir" ? "📁 " : "";
        ctx.fillText(`${titlePrefix}${r.item.name}`, rx + 6, ry + 17);

        if (rh > 42) {
          ctx.fillStyle = "rgba(241, 245, 249, 0.85)";
          ctx.font = "500 11px JetBrains Mono, Consolas, monospace";
          ctx.fillText(formatBytes(r.item.allocatedBytes), rx + 6, ry + 33);
        }
        ctx.restore();
      }
    }
  }, [rects, size, hovered, colorBy]);

  const hitTest = (clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const bounds = canvas.getBoundingClientRect();
    const x = clientX - bounds.left;
    const y = clientY - bounds.top;
    return rects.find((r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) ?? null;
  };

  return (
    <div
      ref={containerRef}
      style={{
        position: "relative",
        width: "100%",
        height: 410,
        borderRadius: 8,
        overflow: "hidden",
        border: "1px solid var(--border)",
      }}
    >
      <canvas
        ref={canvasRef}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Escape") setContextMenu(null);
        }}
        style={{ display: "block", cursor: hovered ? "pointer" : "default" }}
        onMouseMove={(e) => {
          const found = hitTest(e.clientX, e.clientY);
          if (found) {
            setHovered({ rect: found, x: e.clientX, y: e.clientY });
          } else {
            setHovered(null);
          }
        }}
        onMouseLeave={() => setHovered(null)}
        onClick={(e) => {
          const found = hitTest(e.clientX, e.clientY);
          if (found && found.item.kind === "dir" && found.item.nodeId !== null) {
            onDrillDown(found.item.nodeId);
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          const found = hitTest(e.clientX, e.clientY);
          if (found && !found.item.isOtherBucket) {
            setContextMenu({ node: found.item, x: e.clientX, y: e.clientY });
          }
        }}
      />

      {hovered && (
        <div
          style={{
            position: "fixed",
            left: Math.min(hovered.x + 14, window.innerWidth - 280),
            top: Math.min(hovered.y + 14, window.innerHeight - 120),
            background: "rgba(15, 23, 42, 0.96)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            padding: "8px 11px",
            pointerEvents: "none",
            zIndex: 90,
            maxWidth: 280,
          }}
        >
          <div style={{ fontWeight: 600, fontSize: 13, wordBreak: "break-word" }}>
            {hovered.rect.item.name}
          </div>
          <div className="mono" style={{ fontSize: 12, color: "#38bdf8", marginTop: 2 }}>
            {formatBytes(hovered.rect.item.allocatedBytes)} (
            {hovered.rect.item.percentage.toFixed(1)}%)
          </div>
          <div style={{ fontSize: 11.5, color: "#94a3b8", marginTop: 2 }}>
            {hovered.rect.item.kind === "dir"
              ? `${hovered.rect.item.fileCount.toLocaleString()} archivos · Clic para entrar`
              : "Archivo · Clic derecho para acciones"}
          </div>
        </div>
      )}

      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === "Escape") setContextMenu(null);
          }}
        >
          <button
            type="button"
            onClick={() => {
              onOpenExplorer(contextMenu.node.fullPath);
              setContextMenu(null);
            }}
          >
            <FolderOpen size={14} />
            Abrir en Explorador
          </button>
          <button
            type="button"
            onClick={() => {
              onSendToCleanup(contextMenu.node);
              setContextMenu(null);
            }}
          >
            <Trash2 size={14} color="#fca5a5" />
            Enviar a Limpieza segura
          </button>
        </div>
      )}
    </div>
  );
}
