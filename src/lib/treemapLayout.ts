export interface TreemapInputItem {
  id: string;
  value: number;
}

export interface TreemapRect<T extends TreemapInputItem> {
  item: T;
  x: number;
  y: number;
  w: number;
  h: number;
}

function worstAspectRatio(row: number[], side: number): number {
  if (row.length === 0 || side <= 0) return Number.POSITIVE_INFINITY;
  let sum = 0;
  let min = Number.POSITIVE_INFINITY;
  let max = 0;
  for (const v of row) {
    sum += v;
    if (v < min) min = v;
    if (v > max) max = v;
  }
  if (sum <= 0 || min <= 0) return Number.POSITIVE_INFINITY;
  const s2 = side * side;
  const sum2 = sum * sum;
  return Math.max((s2 * max) / sum2, sum2 / (s2 * min));
}

export function layoutSquarifiedTreemap<T extends TreemapInputItem>(
  items: T[],
  width: number,
  height: number,
): TreemapRect<T>[] {
  const positive = items.filter((i) => i.value > 0);
  if (positive.length === 0 || width <= 0 || height <= 0) {
    return [];
  }

  const totalValue = positive.reduce((acc, item) => acc + item.value, 0);
  if (totalValue <= 0) return [];

  const totalArea = width * height;
  const scaled = positive.map((item) => ({
    item,
    area: item.value / totalArea > 0 ? (item.value / totalValue) * totalArea : 0,
  }));

  const rects: TreemapRect<T>[] = [];
  let rx = 0;
  let ry = 0;
  let rw = width;
  let rh = height;
  let idx = 0;

  while (idx < scaled.length) {
    const horizontal = rw >= rh;
    const side = horizontal ? rh : rw;
    const rowAreas: number[] = [];
    const rowItems: T[] = [];

    rowAreas.push(scaled[idx].area);
    rowItems.push(scaled[idx].item);
    idx += 1;

    while (idx < scaled.length) {
      const candidate = scaled[idx].area;
      const curWorst = worstAspectRatio(rowAreas, side);
      const nextWorst = worstAspectRatio([...rowAreas, candidate], side);
      if (nextWorst <= curWorst) {
        rowAreas.push(candidate);
        rowItems.push(scaled[idx].item);
        idx += 1;
      } else {
        break;
      }
    }

    const rowSum = rowAreas.reduce((a, b) => a + b, 0);
    if (horizontal) {
      const colW = side > 0 ? rowSum / side : 0;
      let curY = ry;
      for (let i = 0; i < rowItems.length; i++) {
        const itemH = colW > 0 ? rowAreas[i] / colW : 0;
        rects.push({ item: rowItems[i], x: rx, y: curY, w: colW, h: itemH });
        curY += itemH;
      }
      rx += colW;
      rw = Math.max(0, rw - colW);
    } else {
      const rowH = side > 0 ? rowSum / side : 0;
      let curX = rx;
      for (let i = 0; i < rowItems.length; i++) {
        const itemW = rowH > 0 ? rowAreas[i] / rowH : 0;
        rects.push({ item: rowItems[i], x: curX, y: ry, w: itemW, h: rowH });
        curX += itemW;
      }
      ry += rowH;
      rh = Math.max(0, rh - rowH);
    }
  }

  return rects;
}
