// Minimal element builder. The settings window is small enough that a
// framework would be more code than the app itself.

type Attributes = Record<string, string | number | boolean | EventListener | null | undefined>;
type Child = Node | string | number | null | undefined | false;

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attributes: Attributes = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);

  for (const [key, value] of Object.entries(attributes)) {
    if (value === null || value === undefined || value === false) continue;
    if (key.startsWith("on") && typeof value === "function") {
      node.addEventListener(key.slice(2).toLowerCase(), value as EventListener);
    } else if (key === "class") {
      node.className = String(value);
    } else if (key === "value" && node instanceof HTMLInputElement) {
      node.value = String(value);
    } else if (key === "checked" && node instanceof HTMLInputElement) {
      node.checked = Boolean(value);
    } else {
      node.setAttribute(key, String(value));
    }
  }

  append(node, children);
  return node;
}

export function append(parent: Node, children: Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    parent.appendChild(typeof child === "object" ? child : document.createTextNode(String(child)));
  }
}

export function clear(node: Node): void {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/** A labelled settings row with its control on the right. */
export function row(label: string, description: string | null, ...controls: Child[]): HTMLElement {
  return el(
    "div",
    { class: "row" },
    el("div", { class: "row-label" }, el("b", {}, label), description && el("small", {}, description)),
    el("div", { class: "row-control" }, ...controls),
  );
}

export function card(title: string | null, ...children: Child[]): HTMLElement {
  return el("section", { class: "card" }, title && el("div", { class: "card-title" }, title), ...children);
}

export function checkbox(checked: boolean, onChange: (value: boolean) => void): HTMLInputElement {
  return el("input", {
    type: "checkbox",
    checked,
    onChange: (event: Event) => onChange((event.target as HTMLInputElement).checked),
  });
}

export function select<T extends string>(
  options: { value: T; label: string }[],
  current: T,
  onChange: (value: T) => void,
): HTMLSelectElement {
  const node = el("select", {
    onChange: (event: Event) => onChange((event.target as HTMLSelectElement).value as T),
  });
  for (const option of options) {
    node.appendChild(el("option", { value: option.value, selected: option.value === current }, option.label));
  }
  node.value = current;
  return node;
}

export function number(
  value: number,
  min: number,
  max: number,
  onChange: (value: number) => void,
): HTMLInputElement {
  return el("input", {
    type: "number",
    value,
    min,
    max,
    onChange: (event: Event) => {
      const raw = Number((event.target as HTMLInputElement).value);
      onChange(Math.min(max, Math.max(min, Number.isFinite(raw) ? raw : min)));
    },
  });
}

export function text(
  value: string,
  placeholder: string,
  onChange: (value: string) => void,
): HTMLInputElement {
  return el("input", {
    type: "text",
    value,
    placeholder,
    spellcheck: "false",
    onChange: (event: Event) => onChange((event.target as HTMLInputElement).value),
  });
}

/** A colour swatch. The picker only speaks `#rrggbb`, which is what the bar
 *  theme is written in; a hand-edited file may use short or alpha forms and
 *  those are normalised on the way in. */
export function colour(value: string, onChange: (value: string) => void): HTMLInputElement {
  return el("input", {
    type: "color",
    class: "swatch",
    value: normalizeColour(value),
    onChange: (event: Event) => onChange((event.target as HTMLInputElement).value),
  });
}

function normalizeColour(value: string): string {
  const digits = value.trim().replace(/^#/, "");
  if (digits.length === 3 || digits.length === 4) {
    return `#${[...digits.slice(0, 3)].map((c) => c + c).join("")}`;
  }
  if (digits.length === 6 || digits.length === 8) return `#${digits.slice(0, 6)}`;
  return "#000000";
}
