import type { Config, RuleAction, Snapshot, WindowRule } from "../api";
import { card, el, select, text } from "../dom";

const ACTIONS: { value: RuleAction; label: string }[] = [
  { value: "tile", label: "Tile" },
  { value: "float", label: "Float" },
  { value: "ignore", label: "Ignore" },
];

export function rulesView(
  config: Config,
  snapshot: Snapshot,
  onChange: () => void,
  rerender: () => void,
): HTMLElement {
  const monitorOptions = [
    { value: "", label: "Any" },
    ...snapshot.monitors.map((monitor, index) => ({
      value: String(index),
      label: `${index + 1}${monitor.isPrimary ? " (primary)" : ""}`,
    })),
  ];

  const rows = config.rules.map((rule, index) =>
    el(
      "tr",
      {},
      el(
        "td",
        {},
        text(rule.process ?? "", "code.exe", (value) => {
          rule.process = value.trim() || null;
          onChange();
        }),
      ),
      el(
        "td",
        {},
        text(rule.class ?? "", "Window class", (value) => {
          rule.class = value.trim() || null;
          onChange();
        }),
      ),
      el(
        "td",
        {},
        text(rule["title-contains"] ?? "", "Part of the title", (value) => {
          rule["title-contains"] = value.trim() || null;
          onChange();
        }),
      ),
      el(
        "td",
        {},
        select(ACTIONS, rule.action, (value) => {
          rule.action = value;
          onChange();
        }),
      ),
      el(
        "td",
        {},
        select(monitorOptions, rule.monitor === null ? "" : String(rule.monitor), (value) => {
          rule.monitor = value === "" ? null : Number(value);
          onChange();
        }),
      ),
      el(
        "td",
        {},
        el("input", {
          type: "checkbox",
          checked: !rule.disabled,
          onChange: (event: Event) => {
            rule.disabled = !(event.target as HTMLInputElement).checked;
            onChange();
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
              config.rules.splice(index, 1);
              onChange();
              rerender();
            },
          },
          "Remove",
        ),
      ),
    ),
  );

  const addRule = (rule: WindowRule) => {
    config.rules.push(rule);
    onChange();
    rerender();
  };

  return el(
    "div",
    {},
    el("h1", { class: "page-title" }, "Application rules"),
    el(
      "p",
      { class: "page-subtitle" },
      "Rules are checked from the top and the first match decides. Leave a field empty to ignore it.",
    ),

    card(
      null,
      el(
        "table",
        {},
        el(
          "thead",
          {},
          el(
            "tr",
            {},
            el("th", {}, "Process"),
            el("th", {}, "Class"),
            el("th", {}, "Title contains"),
            el("th", {}, "Action"),
            el("th", {}, "Monitor"),
            el("th", {}, "On"),
            el("th", {}, ""),
          ),
        ),
        el("tbody", {}, ...rows),
      ),
      rows.length === 0 && el("div", { class: "empty" }, "Every window is tiled."),
      el(
        "div",
        { class: "toolbar" },
        el(
          "button",
          {
            onClick: () =>
              addRule({
                process: null,
                class: null,
                "title-contains": null,
                action: "float",
                monitor: null,
                disabled: false,
              }),
          },
          "Add rule",
        ),
      ),
    ),

    snapshot.windows.length > 0 &&
      card(
        "Add from an open window",
        el(
          "table",
          {},
          el(
            "thead",
            {},
            el("tr", {}, el("th", {}, "Process"), el("th", {}, "Title"), el("th", {}, "")),
          ),
          el(
            "tbody",
            {},
            ...snapshot.windows.map((window) =>
              el(
                "tr",
                {},
                el("td", { class: "mono" }, window.process),
                el("td", { class: "dim" }, window.title),
                el(
                  "td",
                  {},
                  el(
                    "button",
                    {
                      onClick: () =>
                        addRule({
                          process: window.process,
                          class: null,
                          "title-contains": null,
                          action: "float",
                          monitor: null,
                          disabled: false,
                        }),
                    },
                    "Float this app",
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
  );
}
