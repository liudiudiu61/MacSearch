export type VirtualWindowInput = {
  total: number;
  rowHeight: number;
  viewportHeight: number;
  scrollTop: number;
  overscan: number;
};

export type VirtualWindow = {
  start: number;
  end: number;
  offsetTop: number;
  totalHeight: number;
};

export function calculateVirtualWindow(input: VirtualWindowInput): VirtualWindow {
  const safeTotal = Math.max(0, input.total);
  const rowHeight = Math.max(1, input.rowHeight);
  const visibleRows = Math.ceil(input.viewportHeight / rowHeight);
  const firstVisible = Math.floor(Math.max(0, input.scrollTop) / rowHeight);
  const start = Math.max(0, firstVisible - input.overscan);
  const end = Math.min(safeTotal, firstVisible + visibleRows + input.overscan + 1);

  return {
    start,
    end,
    offsetTop: start * rowHeight,
    totalHeight: safeTotal * rowHeight
  };
}
