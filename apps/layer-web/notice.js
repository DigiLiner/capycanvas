export const TIMEOUT_MS = 4000, MARGIN = 12, GAP = 12, PADDING = 28;

// The shared placement, lifted above a canvas action bar along the canvas bottom.
export function noticePlacement({ notice }, bar) {
  const y = bar && bar.y + bar.height > notice.bar_floor ? Math.min(notice.y, bar.y - MARGIN) : notice.y;
  return { x: notice.x, y, width: notice.max_width, minText: notice.min_text_width };
}

export function createNotice({ workspace, element, button, answer, layout, bar,
  timeoutMs = TIMEOUT_MS, setTimer = setTimeout, clearTimer = clearTimeout }) {
  const root = element("div", "canvas-notice");
  root.setAttribute("role", "status");
  root.hidden = true;
  const text = element("span", "canvas-notice-text");
  const actions = element("span", "canvas-notice-actions");
  actions.hidden = true;
  root.append(text, actions); workspace.append(root);
  root.addEventListener("mousedown", e => e.preventDefault());
  root.addEventListener("contextmenu", e => e.preventDefault());
  let shown = null, timer = 0, transform = "";

  function accept(action) {
    if (shown === null || root.hidden) return;
    hide();
    answer(shown, true, action);
  }
  function show(list) {
    actions.replaceChildren(...list.map(({ id, label, enabled, reason }) => {
      const node = button(label, () => accept(id), "canvas-notice-action");
      node.disabled = !enabled;
      if (reason) node.title = reason;
      return node;
    }));
    actions.hidden = list.length === 0;
  }
  function hide() {
    clearTimer(timer); timer = 0;
    if (!root.hidden) root.hidden = true;
  }
  function place() {
    const resolved = layout();
    if (root.hidden || !resolved) return;
    const placement = noticePlacement(resolved, bar());
    const ratio = globalThis.devicePixelRatio || 1, align = v => Math.round(v * ratio) / ratio;
    root.style.maxWidth = "none"; root.classList.remove("stacked");
    const natural = node => node.hidden ? 0 : node.getBoundingClientRect().width;
    const content = placement.width - PADDING, words = natural(text), offers = natural(actions);
    const beside = !offers || words + GAP + offers <= content || content - offers - GAP >= placement.minText;
    root.classList.toggle("stacked", !beside);
    root.style.maxWidth = `${placement.width}px`;
    const next = `translate(${align(placement.x)}px, ${align(placement.y)}px) translate(-50%, -100%)`;
    if (next !== transform) root.style.transform = transform = next;
  }
  function publish(notice) {
    if (!notice) { hide(); shown = null; return; }
    text.textContent = notice.text;
    root.setAttribute("aria-label", notice.name);
    show(notice.actions ?? []);
    if (!root.hidden) place();
    if (shown === notice.id) return;
    const id = shown = notice.id;
    root.hidden = false;
    place();
    clearTimer(timer);
    timer = setTimer(() => { timer = 0; root.hidden = true; answer(id, false); }, timeoutMs);
  }
  return { root, publish, hide, place };
}
