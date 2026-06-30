import { invoke } from "@tauri-apps/api/core";

import type { Config, LayoutKind, Environment, Rect } from "../api";
import { card, clear, el, number, row, select } from "../dom";

const PREVIEW_WIDTH = 118;
const PREVIEW_HEIGHT = 74;
const PREVIEW_COUNTS = [1, 2, 3, 4, 5];

export function layoutView(
  config: Config,
  environment: Environment,
  onChange: () => void,
): HTMLElement {
  const preview = el("div", { class: "preview" });

  const refresh = () => {
    void renderPreview(preview, config);
  };
  refresh();

  const changed = () => {
    onChange();
    refresh();
  };

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "Layout"),
    el("p", { class: "page-subtitle" }, "The starting layout and how much room windows leave each other."),

    card(
      "Algorithm",
      row(
        "Default layout",
        "Each display can be switched separately from the tray or with Win+Space.",
        select<LayoutKind>(
          environment.layouts.map((layout) => ({ value: layout.id, label: layout.label })),
          config.layout,
          (value) => {
            config.layout = value;
            changed();
          },
        ),
      ),
      row(
        "Main area share",
        "Only used by the main and stack layout.",
        el("input", {
          type: "range",
          min: "20",
          max: "80",
          value: Math.round(config["main-ratio"] * 100),
          onInput: (event: Event) => {
            config["main-ratio"] = Number((event.target as HTMLInputElement).value) / 100;
            changed();
          },
        }),
      ),
      row(
        "Mirror",
        "Puts the main area on the right or at the bottom.",
        el("input", {
          type: "checkbox",
          checked: config.reversed,
          onChange: (event: Event) => {
            config.reversed = (event.target as HTMLInputElement).checked;
            changed();
          },
        }),
      ),
    ),

    card(
      "Spacing",
      row(
        "Gap between windows",
        "In pixels.",
        number(config.gap, 0, 64, (value) => {
          config.gap = value;
          changed();
        }),
      ),
      row(
        "Gap around the screen",
        "Extra space at the edge of the work area.",
        number(config["outer-gap"], 0, 96, (value) => {
          config["outer-gap"] = value;
          changed();
        }),
      ),
    ),

    card("Preview", preview),
  );
}

async function renderPreview(container: HTMLElement, config: Config): Promise<void> {
  const options = {
    // The preview screen is tiny, so scale the gaps down with it or a
    // 8 pixel gap would swallow the whole thing.
    gap: Math.round(config.gap / 4),
    "outer-gap": Math.round(config["outer-gap"] / 4),
    "main-ratio": config["main-ratio"],
    reversed: config.reversed,
  };

  const results = await Promise.all(
    PREVIEW_COUNTS.map((count) =>
      invoke<Rect[]>("preview_layout", {
        layout: config.layout,
        count,
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        options,
      }),
    ),
  );

  clear(container);
  results.forEach((tiles, index) => {
    const count = PREVIEW_COUNTS[index] ?? 0;
    container.appendChild(
      el(
        "figure",
        {},
        svgPreview(tiles),
        el("figcaption", {}, count === 1 ? "1 window" : `${count} windows`),
      ),
    );
  });
}

function svgPreview(tiles: Rect[]): SVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("width", String(PREVIEW_WIDTH));
  svg.setAttribute("height", String(PREVIEW_HEIGHT));
  svg.setAttribute("viewBox", `0 0 ${PREVIEW_WIDTH} ${PREVIEW_HEIGHT}`);

  for (const tile of tiles) {
    const rect = document.createElementNS(ns, "rect");
    rect.setAttribute("class", "tile");
    rect.setAttribute("x", String(tile.x));
    rect.setAttribute("y", String(tile.y));
    rect.setAttribute("width", String(Math.max(tile.width, 1)));
    rect.setAttribute("height", String(Math.max(tile.height, 1)));
    rect.setAttribute("rx", "1");
    svg.appendChild(rect);
  }

  return svg;
}
