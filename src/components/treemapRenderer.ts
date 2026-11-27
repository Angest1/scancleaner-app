import { colorForCategory, formatBytes } from "../lib/format";
import type { TreemapNodeDto } from "../lib/ipc";
import type { TreemapRect } from "../lib/treemapLayout";

export interface PositionedTreemapNode extends TreemapNodeDto {
  id: string;
  value: number;
  isCloudOnly?: boolean;
  isPendingTrash?: boolean;
}

const CATEGORY_LABELS: Record<string, string> = {
  video: "Videos",
  image: "Imágenes",
  audio: "Audio",
  documents: "Documentos",
  archives: "Comprimidos",
  executables: "Ejecutables",
  code: "Código y datos",
  installedapps: "Aplicaciones",
  system: "Sistema Windows",
  temp: "Temporales",
  browsercache: "Caché navegador",
  devartifact: "Artefactos dev",
  recyclebin: "Papelera",
  downloads: "Descargas",
  userother: "Archivos usuario",
  other: "Otros",
};

function drawRoundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  const rad = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rad, y);
  ctx.lineTo(x + w - rad, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + rad);
  ctx.lineTo(x + w, y + h - rad);
  ctx.quadraticCurveTo(x + w, y + h, x + w - rad, y + h);
  ctx.lineTo(x + rad, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - rad);
  ctx.lineTo(x, y + rad);
  ctx.quadraticCurveTo(x, y, x + rad, y);
  ctx.closePath();
}

function hashId(str: string): number {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/**
 * Draws a crisp, edge-to-edge defragmentation cluster map strictly inside the
 * dedicated lower body area BELOW the tile's typography header (so text is never
 * obscured and clusters fill 100% of the available tile area symmetrically).
 */
function drawDefragClusterMap(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  item: PositionedTreemapNode,
  isHovered: boolean,
) {
  if (w < 18 || h < 14) return;

  // Adaptive cell size so both huge and compact tiles look balanced and crisp
  const targetCell = w * h > 45_000 ? 9 : w * h > 14_000 ? 7 : 5;
  const gap = targetCell >= 7 ? 2.5 : 2;
  const step = targetCell + gap;

  const cols = Math.floor((w + gap) / step);
  const rows = Math.floor((h + gap) / step);
  if (cols <= 0 || rows <= 0) return;

  // Center the matrix symmetrically inside the dedicated cluster box
  const gridW = cols * step - gap;
  const gridH = rows * step - gap;
  const startX = x + (w - gridW) / 2;
  const startY = y + (h - gridH) / 2;

  const totalCells = cols * rows;
  const shareRatio = Math.min(1, Math.max(0.14, item.percentage / 100));
  const isSingleFile = item.kind !== "dir" && !item.isOtherBucket;
  const occupiedTarget = isSingleFile
    ? Math.max(1, Math.round(totalCells * Math.max(0.55, shareRatio)))
    : Math.max(1, Math.round(totalCells * Math.min(0.95, 0.3 + Math.sqrt(shareRatio) * 0.65)));

  const seed = hashId(item.id);
  const rad = targetCell >= 7 ? 2 : 1;

  // Subtle recessed tray behind the defrag clusters
  drawRoundedRect(ctx, x - 2, y - 2, w + 4, h + 4, 5);
  ctx.fillStyle = "rgba(10, 14, 26, 0.22)";
  ctx.fill();

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const idx = r * cols + c;
      const bx = startX + c * step;
      const by = startY + r * step;

      const mix = ((seed ^ Math.imul(idx + 1, 2654435761)) >>> 0) % 100;
      const occupied = isSingleFile
        ? idx < occupiedTarget
        : idx < occupiedTarget * 0.78 || (idx < occupiedTarget * 1.15 && mix < 58);

      if (item.isCloudOnly) {
        ctx.strokeStyle = occupied
          ? isHovered
            ? "rgba(255, 255, 255, 0.55)"
            : "rgba(255, 255, 255, 0.34)"
          : "rgba(255, 255, 255, 0.1)";
        ctx.lineWidth = 1;
        ctx.strokeRect(bx + 0.5, by + 0.5, targetCell - 1, targetCell - 1);
      } else if (occupied) {
        let alpha: number;
        if (isSingleFile) {
          alpha = isHovered ? 0.42 : 0.3;
        } else if (mix > 76) {
          alpha = isHovered ? 0.5 : 0.38;
        } else if (mix > 38) {
          alpha = isHovered ? 0.34 : 0.24;
        } else {
          alpha = isHovered ? 0.22 : 0.15;
        }

        ctx.fillStyle = item.isProtected
          ? `rgba(253, 224, 71, ${alpha})`
          : `rgba(255, 255, 255, ${alpha})`;
        drawRoundedRect(ctx, bx, by, targetCell, targetCell, rad);
        ctx.fill();
      } else {
        ctx.fillStyle = "rgba(15, 23, 42, 0.24)";
        drawRoundedRect(ctx, bx, by, targetCell, targetCell, rad);
        ctx.fill();
      }
    }
  }
}

export function renderTreemapCanvas(
  canvas: HTMLCanvasElement,
  rects: TreemapRect<PositionedTreemapNode>[],
  size: { width: number; height: number },
  hoveredId: string | null,
  colorBy: "type" | "origin",
) {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(size.width * dpr);
  canvas.height = Math.round(size.height * dpr);
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
    const pad = 2.5;
    const rx = r.x + pad;
    const ry = r.y + pad;
    const rw = Math.max(0, r.w - pad * 2);
    const rh = Math.max(0, r.h - pad * 2);
    if (rw <= 1 || rh <= 1) continue;

    const isHovered = hoveredId === r.item.id;
    const catKey = colorBy === "type" ? r.item.typeCat : r.item.originCat;
    const baseColor = r.item.isOtherBucket ? "#576175" : colorForCategory(catKey);
    const corner = rw > 24 && rh > 24 ? 8 : 3;

    const canDrawBar = rw >= 18 && rh >= 12;
    const barHeight = rh > 70 ? 4 : rh > 30 ? 3 : 2;
    const barInset = rw > 80 ? 10 : rw > 34 ? 6 : 3;
    const barBottomPad = rh > 70 ? 8 : rh > 30 ? 5 : 3;
    const barReserve = canDrawBar ? barHeight + barBottomPad + 6 : 6;

    // Base Bento tile fill + subtle top highlight
    drawRoundedRect(ctx, rx, ry, rw, rh, corner);
    ctx.fillStyle = baseColor;
    ctx.globalAlpha = isHovered ? 0.98 : 0.9;
    ctx.fill();
    ctx.globalAlpha = 1;

    if (rw > 48 && rh > 32) {
      const sheen = ctx.createLinearGradient(rx, ry, rx, ry + Math.min(rh, 80));
      sheen.addColorStop(0, isHovered ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.10)");
      sheen.addColorStop(1, "rgba(255,255,255,0)");
      ctx.fillStyle = sheen;
      ctx.fill();
    }

    ctx.strokeStyle = isHovered ? hoverStroke : sepStroke;
    ctx.lineWidth = isHovered ? 2 : 1;
    ctx.stroke();

    if (r.item.isProtected && rw > 18 && rh > 18) {
      ctx.fillStyle = "#f59e0b";
      ctx.beginPath();
      ctx.moveTo(rx + rw - 1, ry + 1);
      ctx.lineTo(rx + rw - 12, ry + 1);
      ctx.lineTo(rx + rw - 1, ry + 12);
      ctx.closePath();
      ctx.fill();
    }

    // Track exact height consumed by header text so the defrag cluster zone sits cleanly below it
    let headerBottom = ry + 8;

    if (rw > 64 && rh > 28) {
      ctx.save();
      drawRoundedRect(ctx, rx + 6, ry + 5, rw - 12, Math.max(8, rh - barReserve - 5), 3);
      ctx.clip();

      ctx.fillStyle = "#ffffff";
      const titleFontSize = rw > 220 && rh > 110 ? 13.5 : 12;
      ctx.font = `600 ${titleFontSize}px "Geist Sans", "Segoe UI", sans-serif`;
      const suffix = r.item.kind === "dir" ? "/" : "";
      ctx.fillText(`${r.item.name}${suffix}`, rx + 10, ry + 20);
      headerBottom = ry + 26;

      if (rh > 48) {
        ctx.fillStyle = "rgba(248, 250, 252, 0.94)";
        ctx.font = '600 11.5px "JetBrains Mono", Consolas, monospace';
        const sizeStr = formatBytes(r.item.allocatedBytes);
        const pctStr = rw > 135 ? ` · ${r.item.percentage.toFixed(1)}%` : "";
        ctx.fillText(`${sizeStr}${pctStr}`, rx + 10, ry + 37);
        headerBottom = ry + 43;
      }

      if (rw > 135 && rh > 74) {
        ctx.fillStyle = "rgba(241, 245, 249, 0.84)";
        ctx.font = '500 11px "Geist Sans", "Segoe UI", sans-serif';
        const catLabel = CATEGORY_LABELS[catKey] ?? catKey;
        const detailLine =
          r.item.kind === "dir"
            ? `${r.item.fileCount.toLocaleString()} archivos`
            : r.item.isCloudOnly
              ? "En la nube (placeholder)"
              : catLabel;
        ctx.fillText(detailLine, rx + 10, ry + 54);
        headerBottom = ry + 60;
      }

      if (rw > 205 && rh > 116) {
        const catLabel = CATEGORY_LABELS[catKey] ?? catKey;
        const badgeText = r.item.isProtected
          ? `${catLabel} · Protegida`
          : r.item.isCloudOnly
            ? `${catLabel} · En la nube`
            : catLabel;
        ctx.font = '500 10.5px "Geist Sans", "Segoe UI", sans-serif';
        const textW = Math.min(ctx.measureText(badgeText).width + 14, rw - 24);
        const pillY = ry + 63;
        drawRoundedRect(ctx, rx + 10, pillY, textW, 20, 999);
        ctx.fillStyle = r.item.isProtected ? "rgba(245, 158, 11, 0.34)" : "rgba(15, 23, 42, 0.34)";
        ctx.fill();
        ctx.fillStyle = "#f8fafc";
        ctx.fillText(badgeText, rx + 17, pillY + 13.5);
        headerBottom = pillY + 26;
      }
      ctx.restore();
    }

    // Dedicated, edge-to-edge defragmentation cluster map BELOW the header text
    const clusterPadX = rw > 80 ? 12 : 6;
    const clusterY = headerBottom + 4;
    const clusterW = rw - clusterPadX * 2;
    const clusterH = ry + rh - barReserve - clusterY;
    if (clusterW >= 18 && clusterH >= 14) {
      ctx.save();
      drawRoundedRect(ctx, rx + 2, ry + 2, rw - 4, rh - 4, Math.max(2, corner - 2));
      ctx.clip();
      drawDefragClusterMap(ctx, rx + clusterPadX, clusterY, clusterW, clusterH, r.item, isHovered);
      ctx.restore();
    }

    // Space-share bar on EVERY tile unless physically too tiny (< 18x12px)
    if (canDrawBar) {
      const barW = Math.max(6, rw - barInset * 2);
      const barY = ry + rh - barBottomPad - barHeight;
      const barRad = barHeight / 2;
      drawRoundedRect(ctx, rx + barInset, barY, barW, barHeight, barRad);
      ctx.fillStyle = "rgba(15, 23, 42, 0.32)";
      ctx.fill();
      const fillRatio = Math.max(0, Math.min(1, r.item.percentage / 100));
      const fillW = Math.max(barHeight, barW * fillRatio);
      drawRoundedRect(ctx, rx + barInset, barY, fillW, barHeight, barRad);
      ctx.fillStyle = isHovered ? "rgba(255, 255, 255, 0.98)" : "rgba(255, 255, 255, 0.86)";
      ctx.fill();
    }
  }
}
