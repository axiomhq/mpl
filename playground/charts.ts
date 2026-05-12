// Renders one chart per pipeline step.

import type { Series } from "@axiomhq/mpl-playground";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

const COLORS_LIGHT = [
  "#2563eb",
  "#dc2626",
  "#16a34a",
  "#d97706",
  "#9333ea",
  "#0891b2",
  "#e11d48",
  "#65a30d",
];
const COLORS_DARK = [
  "#60a5fa",
  "#f87171",
  "#4ade80",
  "#fbbf24",
  "#c084fc",
  "#22d3ee",
  "#fb7185",
  "#a3e635",
];

function isDarkTheme(): boolean {
  return document.documentElement.classList.contains("dark-theme");
}

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function tooltipPlugin(): uPlot.Plugin {
  let tooltip: HTMLElement | null = null;
  let over: HTMLElement | null = null;

  function init(u: uPlot) {
    over = u.over;
    tooltip = document.createElement("div");
    tooltip.className = "step-tooltip";
    tooltip.style.display = "none";
    document.body.appendChild(tooltip);
  }

  function nearestValue(data: uPlot.AlignedData, seriesIdx: number, idx: number): number | null {
    const vals = data[seriesIdx];
    if (vals[idx] != null) return vals[idx] as number;
    for (let d = 1; d < 5; d++) {
      if (idx - d >= 0 && vals[idx - d] != null) return vals[idx - d] as number;
      if (idx + d < vals.length && vals[idx + d] != null) return vals[idx + d] as number;
    }
    return null;
  }

  function setCursor(u: uPlot) {
    if (!tooltip || !over) return;
    const idx = u.cursor.idx;
    if (idx == null) {
      tooltip.style.display = "none";
      return;
    }

    const dark = isDarkTheme();
    const palette = dark ? COLORS_DARK : COLORS_LIGHT;
    const lines: string[] = [];
    for (let i = 1; i < u.series.length; i++) {
      const s = u.series[i];
      if (!s.show) continue;
      const val = nearestValue(u.data, i, idx);
      if (val == null) continue;
      const color = palette[(i - 1) % palette.length];
      const formatted = Number.isInteger(val) ? val.toString() : val.toFixed(2);
      lines.push(
        `<span style="color:${color}">${escHtml(String(s.label ?? ""))}</span>: ${formatted}`,
      );
    }

    if (lines.length === 0) {
      tooltip.style.display = "none";
      return;
    }

    tooltip.innerHTML = lines.join("<br>");
    tooltip.style.display = "block";

    const rect = over.getBoundingClientRect();
    const cursorLeft = u.cursor.left ?? 0;
    const cursorTop = u.cursor.top ?? 0;
    const tooltipW = tooltip.offsetWidth;
    const x =
      rect.left + cursorLeft + tooltipW + 16 > window.innerWidth
        ? rect.left + cursorLeft - tooltipW - 8
        : rect.left + cursorLeft + 8;
    tooltip.style.left = `${x}px`;
    tooltip.style.top = `${rect.top + cursorTop - 10}px`;
  }

  function destroy() {
    if (tooltip) {
      tooltip.remove();
      tooltip = null;
    }
  }

  return { hooks: { init, setCursor, destroy } };
}

function alignSeries(series: Series[]): uPlot.AlignedData {
  const tsSet = new Set<number>();
  for (const s of series) {
    // Playground series timestamps are Unix seconds; uPlot's time scale
    // expects Unix milliseconds.
    for (const t of s.timestamps) tsSet.add(t * 1000);
  }
  const allTs = Float64Array.from([...tsSet].sort((a, b) => a - b));

  const aligned: (Float64Array | (number | null)[])[] = [allTs];
  for (const s of series) {
    const lookup = new Map<number, number>();
    for (let i = 0; i < s.timestamps.length; i++) {
      lookup.set(s.timestamps[i] * 1000, s.values[i]);
    }
    const vals: (number | null)[] = Array.from({ length: allTs.length });
    for (let i = 0; i < allTs.length; i++) {
      const v = lookup.get(allTs[i]);
      vals[i] = v != null && !Number.isNaN(v) ? v : null;
    }
    aligned.push(vals);
  }

  return aligned as uPlot.AlignedData;
}

function createChart(
  container: HTMLElement,
  series: Series[],
  width: number,
): uPlot {
  const dark = isDarkTheme();
  const palette = dark ? COLORS_DARK : COLORS_LIGHT;
  const data = alignSeries(series);
  const hasSingleTimestamp = data[0].length === 1;
  const singleTimestamp = hasSingleTimestamp ? Number(data[0][0]) : null;

  const seriesConfig: uPlot.Series[] = [
    { label: "Time" },
    ...series.map((s, i) => ({
      label: s.name,
      stroke: palette[i % palette.length],
      width: 2,
      spanGaps: true,
      points: { show: true, size: 6, fill: dark ? "#1e1e1e" : "#ffffff" },
    })),
  ];

  const gridColor = dark ? "#2a2a2a" : "#f0f0f0";

  return new uPlot(
    {
      width,
      height: 120,
      cursor: {
        show: true,
        drag: { x: false, y: false },
        points: {
          show: true,
          size: 8,
          width: 2,
          fill: (u: uPlot, i: number) => String(u.series[i].stroke ?? ""),
          stroke: (u: uPlot, i: number) => String(u.series[i].stroke ?? ""),
        },
      },
      legend: { show: false },
      scales: {
        x: {
          time: true,
          // When a step produces a single whole-window point, uPlot's default
          // autorange pins it awkwardly near one edge. Give it a symmetric
          // one-hour window so the point renders in the middle.
          range: hasSingleTimestamp && singleTimestamp != null
            ? () => [singleTimestamp - 30 * 60 * 1000, singleTimestamp + 30 * 60 * 1000]
            : undefined,
        },
      },
      axes: [
        {
          show: true,
          stroke: dark ? "#888" : "#999",
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
          font: "10px Ioskeley Mono, ui-monospace, monospace",
          size: 28,
        },
        { show: false, grid: { stroke: gridColor, width: 1 } },
      ],
      series: seriesConfig,
      plugins: [tooltipPlugin()],
    },
    data,
    container,
  );
}

export interface ChartEntry {
  label: string;
  series: Series[];
  error?: string;
}

/**
 * True when every series in the entry collapses to a single timestamp.
 *
 * That's the shape `align using avg` (no time clause) produces — one
 * whole-window aggregate per series. uPlot can render it, but a single
 * dot in a 120px-tall chart is wasted space; a row of value tiles
 * communicates the result an order of magnitude faster.
 */
export function isScalarEntry(entry: ChartEntry): boolean {
  if (entry.series.length === 0) return false;
  return entry.series.every(s => s.timestamps.length === 1);
}

/**
 * Formats a scalar value for tile display. Integers render bare; small
 * non-integers get two decimals; very large or very small numbers fall
 * back to scientific notation so the tile width stays bounded.
 */
export function formatScalar(val: number): string {
  if (!Number.isFinite(val)) return String(val);
  const abs = Math.abs(val);
  // Scientific notation first so even 1.5e12 (technically an integer)
  // stays narrow enough not to blow the tile width.
  if (abs !== 0 && (abs < 1e-3 || abs >= 1e9)) return val.toExponential(2);
  if (Number.isInteger(val)) return val.toString();
  return val.toFixed(2);
}

function renderScalarTiles(parent: HTMLElement, series: Series[]): void {
  const dark = isDarkTheme();
  const palette = dark ? COLORS_DARK : COLORS_LIGHT;

  const row = document.createElement("div");
  row.className = "scalar-tiles";

  series.forEach((s, i) => {
    const tile = document.createElement("div");
    tile.className = "scalar-tile";

    // Coloured accent at the top of each tile so series identity is
    // visible even when the legend below is truncated.
    const chip = document.createElement("div");
    chip.className = "scalar-tile-chip";
    chip.style.background = palette[i % palette.length];
    tile.appendChild(chip);

    const value = document.createElement("div");
    value.className = "scalar-tile-value";
    const raw = s.values[0];
    value.textContent = raw == null || Number.isNaN(raw) ? "–" : formatScalar(raw);
    tile.appendChild(value);

    const label = document.createElement("div");
    label.className = "scalar-tile-label";
    const displayName = s.name ?? "";
    label.textContent = displayName;
    label.title = displayName; // full name on hover when truncated
    tile.appendChild(label);

    row.appendChild(tile);
  });

  parent.appendChild(row);
}

let activeCharts: uPlot[] = [];
let resizeObserver: ResizeObserver | null = null;

export function renderCharts(container: HTMLElement, entries: ChartEntry[]): void {
  resizeObserver?.disconnect();
  resizeObserver = new ResizeObserver(() => {
    const w = Math.max(200, container.clientWidth - 32);
    for (const chart of activeCharts) chart.setSize({ width: w, height: 120 });
  });
  resizeObserver.observe(container);
  for (const chart of activeCharts) chart.destroy();
  activeCharts = [];
  container.innerHTML = "";

  const width = container.clientWidth - 32;

  for (const entry of entries) {
    if (entry.label) {
      const label = document.createElement("div");
      label.className = "chart-label";
      label.textContent = entry.label;
      container.appendChild(label);
    }

    if (entry.error) {
      const el = document.createElement("div");
      el.className = "chart-error";
      el.textContent = entry.error;
      container.appendChild(el);
      continue;
    }

    if (entry.series.length === 0) {
      const el = document.createElement("div");
      el.className = "chart-empty";
      el.textContent = "All series filtered out";
      container.appendChild(el);
      continue;
    }

    if (entry.series[0].timestamps.length === 0) {
      const el = document.createElement("div");
      el.className = "chart-empty";
      el.textContent = "No data points";
      container.appendChild(el);
      continue;
    }

    // Whole-window aggregates (e.g. `align using avg` with no time)
    // collapse to one point per series — render value tiles instead of a
    // single-dot uPlot chart, which is unreadable at typical heights.
    if (isScalarEntry(entry)) {
      renderScalarTiles(container, entry.series);
      continue;
    }

    const el = document.createElement("div");
    el.className = "chart-widget";
    container.appendChild(el);

    const chart = createChart(el, entry.series, Math.max(200, width));
    activeCharts.push(chart);
  }
}

export function destroyCharts(): void {
  for (const chart of activeCharts) chart.destroy();
  activeCharts = [];
}
