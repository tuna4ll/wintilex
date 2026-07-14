import type { Config, Hotkey, HotkeyIssue } from "../api";
import { card, el, text } from "../dom";

/** Human labels for the action tags serialised by `tilex_core::command`. */
const ACTION_LABELS: Record<string, string> = {
  focus: "Focus",
  move: "Move window",
  grow: "Grow",
  shrink: "Shrink",
  "focus-cycle": "Cycle focus",
  "move-to-monitor": "Send to monitor",
  promote: "Promote to first",
  "toggle-floating": "Toggle floating",
  "toggle-tiling": "Toggle tiling",
  "cycle-layout": "Next layout",
  "set-layout": "Set layout",
  "toggle-reversed": "Mirror layout",
  "reset-ratios": "Reset sizes",
  retile: "Retile",
  "reload-config": "Reload config",
  minimize: "Minimize window",
  quit: "Quit",
};

function describe(hotkey: Hotkey): string {
  const base = ACTION_LABELS[hotkey.kind] ?? hotkey.kind;
  return hotkey.value === undefined || hotkey.value === null
    ? base
    : `${base} – ${String(hotkey.value)}`;
}

export function hotkeysView(
  config: Config,
  issues: HotkeyIssue[],
  onChange: () => void,
  rerender: () => void,
): HTMLElement {
  const issueFor = (binding: string) =>
    issues.find((issue) => issue.binding.toLowerCase() === binding.toLowerCase());

  const rows = config.hotkeys.map((hotkey, index) => {
    const issue = issueFor(hotkey.binding);
    return el(
      "tr",
      {},
      el(
        "td",
        { class: "mono" },
        text(hotkey.binding, "Win+Shift+H", (value) => {
          hotkey.binding = value.trim();
          onChange();
          rerender();
        }),
      ),
      el("td", {}, describe(hotkey)),
      el(
        "td",
        {},
        issue
          ? el(
              "span",
              { class: "row-control" },
              el("span", { class: "pill" }, "inactive"),
              el("small", { class: "dim" }, issue.reason),
            )
          : hotkey.disabled
            ? el("span", { class: "pill" }, "off")
            : el("span", { class: "pill on" }, "active"),
      ),
      el(
        "td",
        {},
        el("input", {
          type: "checkbox",
          checked: !hotkey.disabled,
          onChange: (event: Event) => {
            hotkey.disabled = !(event.target as HTMLInputElement).checked;
            onChange();
            rerender();
          },
        }),
      ),
      el(
        "td",
        {},
        el(
          "button",
          {
            onClick: () => {
              config.hotkeys.splice(index, 1);
              onChange();
              rerender();
            },
          },
          "Remove",
        ),
      ),
    );
  });

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "Hotkeys"),
    el(
      "p",
      { class: "page-subtitle" },
      "Bindings are written the way they read: modifiers joined with a plus, then the key.",
    ),

    card(
      null,
      issues.length > 0 &&
        el(
          "div",
          { class: "warning" },
          `${issues.length} binding${issues.length === 1 ? "" : "s"} ${issues.length === 1 ? "is" : "are"} not active. ` +
            "Bindings left to Windows are the shell shortcuts Tilex refuses to swallow; " +
            "give them a different combination, or turn the protection off on the General page.",
        ),
      el(
        "table",
        {},
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", {}, "Binding"),
            el("th", {}, "Action"),
            el("th", {}, "State"),
            el("th", {}, "On"),
            el("th", {}, ""),
          ),
        ),
        el("tbody", {}, ...rows),
      ),
      rows.length === 0 && el("div", { class: "empty" }, "No hotkeys are configured."),
    ),
  );
}
