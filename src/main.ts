import "./styles.css";

import {
  getConfig,
  getEnvironment,
  getHotkeyIssues,
  getSnapshot,
  resetConfig,
  revealConfig,
  saveConfig,
  type Config,
  type Environment,
  type HotkeyIssue,
  type Snapshot,
} from "./api";
import { clear, el } from "./dom";
import { generalView } from "./views/general";
import { hotkeysView } from "./views/hotkeys";
import { layoutView } from "./views/layout";
import { rulesView } from "./views/rules";
import { windowsView } from "./views/windows";

type Page = "general" | "layout" | "hotkeys" | "rules" | "windows";

const PAGES: { id: Page; label: string }[] = [
  { id: "general", label: "General" },
  { id: "layout", label: "Layout" },
  { id: "hotkeys", label: "Hotkeys" },
  { id: "rules", label: "Rules" },
  { id: "windows", label: "Windows" },
];

/** How long the manager is left alone between snapshot polls. */
const SNAPSHOT_INTERVAL_MS = 1500;

let config: Config;
let environment: Environment;
let snapshot: Snapshot;
let issues: HotkeyIssue[] = [];
let page: Page = "general";
let dirty = false;
let savedAt = 0;

const root = document.getElementById("app")!;

async function boot(): Promise<void> {
  [config, environment, snapshot, issues] = await Promise.all([
    getConfig(),
    getEnvironment(),
    getSnapshot(),
    getHotkeyIssues(),
  ]);
  render();

  window.setInterval(() => {
    void refreshSnapshot();
  }, SNAPSHOT_INTERVAL_MS);
}

async function refreshSnapshot(): Promise<void> {
  snapshot = await getSnapshot();
  // Only the pages that show live state need redrawing, and redrawing while
  // somebody is typing in a rule would throw away their cursor.
  if (page === "windows") render();
}

function markDirty(): void {
  dirty = true;
  renderStatus();
}

async function save(): Promise<void> {
  await saveConfig(config);
  dirty = false;
  savedAt = Date.now();
  issues = await getHotkeyIssues();
  render();
}

async function discard(): Promise<void> {
  config = await getConfig();
  dirty = false;
  render();
}

async function restoreDefaults(): Promise<void> {
  config = await resetConfig();
  dirty = false;
  savedAt = Date.now();
  issues = await getHotkeyIssues();
  render();
}

function render(): void {
  clear(root);
  root.appendChild(sidebar());
  root.appendChild(content());
}

function sidebar(): HTMLElement {
  return el(
    "nav",
    { class: "sidebar" },
    el(
      "div",
      { class: "brand" },
      el("span", { class: "brand-mark" }, el("i", {}), el("i", {}), el("i", {})),
      "Tilex",
    ),
    ...PAGES.map((entry) =>
      el(
        "button",
        {
          class: "nav-item",
          "aria-current": String(entry.id === page),
          onClick: () => {
            page = entry.id;
            render();
          },
        },
        entry.label,
      ),
    ),
    el(
      "div",
      { class: "sidebar-footer" },
      `Version ${environment.version}`,
      el("br"),
      snapshot.tilingEnabled ? "Tiling is on" : "Tiling is paused",
    ),
  );
}

function content(): HTMLElement {
  const container = el("main", { class: "content" });

  switch (page) {
    case "general":
      container.appendChild(
        generalView(config, environment, markDirty, {
          reveal: () => void revealConfig(),
          reset: () => void restoreDefaults(),
        }),
      );
      break;
    case "layout":
      container.appendChild(layoutView(config, environment, markDirty));
      break;
    case "hotkeys":
      container.appendChild(hotkeysView(config, issues, markDirty, render));
      break;
    case "rules":
      container.appendChild(rulesView(config, snapshot, markDirty, render));
      break;
    case "windows":
      container.appendChild(windowsView(snapshot, environment, render));
      break;
  }

  container.appendChild(statusbar());
  return container;
}

function statusbar(): HTMLElement {
  const recentlySaved = Date.now() - savedAt < 3000;
  return el(
    "div",
    { class: "statusbar", id: "statusbar" },
    el(
      "span",
      { class: `status-text${!dirty && recentlySaved ? " saved" : ""}` },
      dirty ? "Unsaved changes" : recentlySaved ? "Saved" : "All changes saved",
    ),
    el(
      "span",
      { class: "row-control" },
      el("button", { onClick: () => void discard(), disabled: !dirty }, "Discard"),
      el("button", { class: "primary", onClick: () => void save(), disabled: !dirty }, "Apply"),
    ),
  );
}

function renderStatus(): void {
  const existing = document.getElementById("statusbar");
  existing?.replaceWith(statusbar());
}

void boot();
