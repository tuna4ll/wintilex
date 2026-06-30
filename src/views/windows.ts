import type { Environment, Snapshot } from "../api";
import { runAction } from "../api";
import { card, el } from "../dom";

export function windowsView(
  snapshot: Snapshot,
  environment: Environment,
  rerender: () => void,
): HTMLElement {
  const layoutLabel = (id: string) =>
    environment.layouts.find((layout) => layout.id === id)?.label ?? id;

  const act = async (kind: string) => {
    await runAction({ kind });
    setTimeout(rerender, 120);
  };

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "Windows"),
    el("p", { class: "page-subtitle" }, "What Tilex is managing right now."),

    card(
      "Displays",
      el(
        "table",
        {},
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", {}, "Display"),
            el("th", {}, "Work area"),
            el("th", {}, "Layout"),
            el("th", {}, "Tiled"),
          ),
        ),
        el(
          "tbody",
          {},
          ...snapshot.monitors.map((monitor) =>
            el(
              "tr",
              {},
              el(
                "td",
                { class: "mono" },
                monitor.id,
                monitor.isPrimary && el("span", { class: "pill" }, "primary"),
              ),
              el(
                "td",
                { class: "dim" },
                `${monitor.workArea.width} x ${monitor.workArea.height} at ${monitor.workArea.x}, ${monitor.workArea.y}`,
              ),
              el("td", {}, layoutLabel(monitor.layout)),
              el("td", {}, String(monitor.tiledWindows)),
            ),
          ),
        ),
      ),
      el(
        "div",
        { class: "toolbar" },
        el("button", { onClick: () => void act("retile") }, "Retile now"),
        el("button", { onClick: () => void act("cycle-layout") }, "Next layout"),
        el("button", { onClick: () => void act("reset-ratios") }, "Reset sizes"),
        el(
          "button",
          { onClick: () => void act("toggle-tiling") },
          snapshot.tilingEnabled ? "Pause tiling" : "Resume tiling",
        ),
      ),
    ),

    card(
      `Tracked windows (${snapshot.windows.length})`,
      snapshot.windows.length === 0
        ? el("div", { class: "empty" }, "Nothing is being managed.")
        : el(
            "table",
            {},
            el(
              "thead",
              {},
              el(
                "tr",
                {},
                el("th", {}, "Process"),
                el("th", {}, "Title"),
                el("th", {}, "State"),
                el("th", {}, "Tile"),
              ),
            ),
            el(
              "tbody",
              {},
              ...snapshot.windows.map((window) =>
                el(
                  "tr",
                  {},
                  el("td", { class: "mono" }, window.process),
                  el("td", {}, window.title || el("span", { class: "dim" }, "untitled")),
                  el(
                    "td",
                    {},
                    window.minimized
                      ? el("span", { class: "pill" }, "minimized")
                      : window.floating
                        ? el("span", { class: "pill" }, "floating")
                        : el("span", { class: "pill on" }, "tiled"),
                    snapshot.focused === window.id && el("span", { class: "pill on" }, "focused"),
                  ),
                  el(
                    "td",
                    { class: "dim mono" },
                    window.tile
                      ? `${window.tile.width} x ${window.tile.height}`
                      : el("span", { class: "dim" }, "—"),
                  ),
                ),
              ),
            ),
          ),
    ),
  );
}
