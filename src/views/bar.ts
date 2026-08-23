import type { BarModule, BarSlot, Config, Environment } from "../api";
import { card, checkbox, colour, el, number, row, select, text } from "../dom";

const SLOTS: { value: BarSlot; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "left", label: "Left" },
  { value: "center", label: "Centre" },
  { value: "right", label: "Right" },
];

const THEME_ROWS: { key: keyof Config["bar"]["theme"]; label: string; hint: string }[] = [
  { key: "background", label: "Background", hint: "The bar itself." },
  { key: "foreground", label: "Text", hint: "Anything meant to be read at a glance." },
  { key: "muted", label: "Dimmed text", hint: "The clock, the readings, minimized windows." },
  { key: "surface", label: "Window pill", hint: "Behind a window that is not focused." },
  { key: "accent", label: "Focused window", hint: "The pill behind whatever has the focus." },
  { key: "accent-text", label: "Text on accent", hint: "Reads on top of the two colours above." },
  { key: "urgent", label: "Warning", hint: "Tiling paused, and a battery about to run out." },
];

export function barView(
  config: Config,
  environment: Environment,
  onChange: () => void,
  onToggle: (enabled: boolean) => void,
): HTMLElement {
  const bar = config.bar;

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "Bar"),
    el(
      "p",
      { class: "page-subtitle" },
      "A strip along one edge of every display, showing what WinTilex is doing.",
    ),

    card(
      "Bar",
      row(
        "Show the bar",
        "One bar per display. This switch takes effect straight away; the rest of the page waits for Apply.",
        checkbox(bar.enabled, onToggle),
      ),
      row(
        "Edge",
        "Which side of the screen the bar sits on.",
        select(
          [
            { value: "top" as const, label: "Top" },
            { value: "bottom" as const, label: "Bottom" },
          ],
          bar.position,
          (value) => {
            bar.position = value;
            onChange();
          },
        ),
      ),
      row(
        "Height",
        "In pixels, scaled up on a high-DPI display.",
        number(bar.height, 16, 96, (value) => {
          bar.height = value;
          onChange();
        }),
      ),
      row(
        "Opacity",
        "Below 100 the windows behind show through.",
        number(Math.round(bar.opacity * 100), 20, 100, (value) => {
          bar.opacity = value / 100;
          onChange();
        }),
      ),
      row(
        "Reserve the space",
        "Takes the strip out of the work area, so nothing is ever drawn under the bar. Turn this off to let the bar float over the windows instead.",
        checkbox(bar["reserve-space"], (value) => {
          bar["reserve-space"] = value;
          onChange();
        }),
      ),
      row(
        "Primary display only",
        "Leaves the other displays without a bar.",
        checkbox(bar["primary-only"], (value) => {
          bar["primary-only"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Modules",
      el(
        "p",
        { class: "card-note" },
        "Each side is drawn in the order the modules were added to it.",
      ),
      ...environment.barModules.map((module) =>
        row(
          module.label,
          describe(module.id),
          select(SLOTS, slotOf(config, module.id), (slot) => {
            move(config, module.id, slot);
            onChange();
          }),
        ),
      ),
    ),

    card(
      "Text",
      row(
        "Font",
        "Any family installed on the machine.",
        text(bar["font-family"], "Segoe UI", (value) => {
          bar["font-family"] = value;
          onChange();
        }),
      ),
      row(
        "Font size",
        "In points.",
        number(bar["font-size"], 6, 32, (value) => {
          bar["font-size"] = value;
          onChange();
        }),
      ),
      row(
        "Clock",
        "%H %I %M %S %p %d %m %y %Y %a %b",
        text(bar["clock-format"], "%a %d %b  %H:%M", (value) => {
          bar["clock-format"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Colours",
      ...THEME_ROWS.map((entry) =>
        row(
          entry.label,
          entry.hint,
          colour(bar.theme[entry.key], (value) => {
            bar.theme[entry.key] = value;
            onChange();
          }),
        ),
      ),
    ),
  );
}

function describe(module: BarModule): string {
  switch (module) {
    case "layout":
      return "The layout of this display. Click it to move to the next one.";
    case "windows":
      return "One pill per window, in tiling order. Click one to focus it.";
    case "title":
      return "The title of the focused window, cut short when there is no room.";
    case "tiling":
      return "Appears only while tiling is paused. Click it to resume.";
    case "monitor":
      return "Which display this bar belongs to.";
    case "cpu":
      return "Processor load, taken once a second.";
    case "memory":
      return "Share of the physical memory in use.";
    case "battery":
      return "Charge left, and whether it is plugged in. Hidden on a desktop.";
    case "clock":
      return "The time, in the format below.";
  }
}

function slotOf(config: Config, module: BarModule): BarSlot {
  if (config.bar.left.includes(module)) return "left";
  if (config.bar.center.includes(module)) return "center";
  if (config.bar.right.includes(module)) return "right";
  return "off";
}

/** Move a module to one side, or off the bar. A module only ever appears once,
 *  so it is taken off the other two sides on the way. */
function move(config: Config, module: BarModule, slot: BarSlot): void {
  const bar = config.bar;
  bar.left = bar.left.filter((entry) => entry !== module);
  bar.center = bar.center.filter((entry) => entry !== module);
  bar.right = bar.right.filter((entry) => entry !== module);

  if (slot === "left") bar.left.push(module);
  if (slot === "center") bar.center.push(module);
  if (slot === "right") bar.right.push(module);
}
