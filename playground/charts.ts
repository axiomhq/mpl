// Renders one chart per pipeline step.

import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { Series } from "./datasets";

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
    for (const t of s.timestamps) tsSet.add(t);
  }
  const allTs = Float64Array.from([...tsSet].sort((a, b) => a - b));

  const aligned: (Float64Array | (number | null)[])[] = [allTs];
  for (const s of series) {
    const lookup = new Map<number, number>();
    for (let i = 0; i < s.timestamps.length; i++) {
      lookup.set(s.timestamps[i], s.values[i]);
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

function createChart(container: HTMLElement, series: Series[], width: number): uPlot {
  const dark = isDarkTheme();
  const palette = dark ? COLORS_DARK : COLORS_LIGHT;
  const data = alignSeries(series);

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
      scales: { x: { time: true } },
      axes: [
        {
          show: true,
          stroke: dark ? "#888" : "#999",
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor, width: 1 },
          font: "10px system-ui, sans-serif",
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
