import type { Config, Environment, HotkeyBackend } from "../api";
import { card, checkbox, el, row, select } from "../dom";

export function generalView(
  config: Config,
  environment: Environment,
  onChange: () => void,
  actions: { reveal: () => void; reset: () => void },
): HTMLElement {
  const general = config.general;

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "General"),
    el("p", { class: "page-subtitle" }, "How WinTilex behaves while it is running."),

    card(
      "Tiling",
      row(
        "Enable tiling",
        "When off, windows are still tracked but never moved.",
        checkbox(general["tiling-enabled"], (value) => {
          general["tiling-enabled"] = value;
          onChange();
        }),
      ),
      row(
        "Keep manual resizes",
        "Dragging a tile edge changes the split instead of snapping back.",
        checkbox(general["absorb-manual-resize"], (value) => {
          general["absorb-manual-resize"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Focus",
      row(
        "Focus follows mouse",
        "Hovering a window gives it the keyboard focus.",
        checkbox(general["focus-follows-mouse"], (value) => {
          general["focus-follows-mouse"] = value;
          onChange();
        }),
      ),
      row(
        "Move pointer with focus",
        "Warps the pointer to the middle of the window you focus.",
        checkbox(general["warp-cursor-to-focus"], (value) => {
          general["warp-cursor-to-focus"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Startup and tray",
      row(
        "Start with Windows",
        "Adds a per-user entry to the registry Run key.",
        checkbox(general["start-on-login"], (value) => {
          general["start-on-login"] = value;
          onChange();
        }),
      ),
      row(
        "Close to tray",
        "Closing this window leaves WinTilex running in the tray.",
        checkbox(general["minimize-to-tray"], (value) => {
          general["minimize-to-tray"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Hotkeys",
      row(
        "Capture method",
        "The keyboard hook sees keys before the shell does, which is what makes Win+arrow bindings possible.",
        select<HotkeyBackend>(
          [
            { value: "hook", label: "Keyboard hook" },
            { value: "system", label: "RegisterHotKey" },
          ],
          general["hotkey-backend"],
          (value) => {
            general["hotkey-backend"] = value;
            onChange();
          },
        ),
      ),
      row(
        "Leave Windows shortcuts alone",
        "Keeps Win+L, Win+Tab, Win+Ctrl+Arrow, Win+Space and Alt+Tab with Windows. Turn this off only if you want WinTilex to take them over.",
        checkbox(general["protect-system-shortcuts"], (value) => {
          general["protect-system-shortcuts"] = value;
          onChange();
        }),
      ),
    ),

    card(
      "Configuration file",
      row(
        "Location",
        environment.configPath,
        el("button", { onClick: actions.reveal }, "Open folder"),
      ),
      row(
        "Reset",
        "Replaces every setting, hotkey and rule with the defaults.",
        el("button", { class: "danger", onClick: actions.reset }, "Restore defaults"),
      ),
    ),
  );
}
