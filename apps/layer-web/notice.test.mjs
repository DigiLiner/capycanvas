import { FakeElement as SharedElement } from "./fake-dom.mjs";
import assert from "node:assert/strict";
import test from "node:test";
import { createNotice, noticePlacement, TIMEOUT_MS, MARGIN, GAP, PADDING } from "./notice.js";

class FakeElement extends SharedElement {
  constructor(tag, className = "") {
    super();
    this.tagName = tag.toUpperCase(); this.className = className; this.children = []; this.parentNode = null;
    this.attributes = new Map(); this.style = {}; this.hidden = false; this.listeners = {}; this.textContent = ""; this.width = 0;
    const names = new Set();
    this.classList = { toggle: (name, on) => on ? names.add(name) : names.delete(name), remove: name => names.delete(name), contains: name => names.has(name) };
  }
  getBoundingClientRect() { return { width: this.width }; }
  dispatch(type) {
    const event = { type, target: this, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    for (let node = this; node; node = node.parentNode) for (const listener of node.listeners[type] ?? []) listener(event);
    return event;
  }
  click() { this.dispatch("click"); }
}

const area = { x: 60, y: 48, width: 1000, height: 800 }, status = { x: 60, y: 820, width: 1000, height: 28 }, floor = status.y;
const placement = { x: 560, y: floor - MARGIN, max_width: 720, min_text_width: 200, window: false, bar_floor: floor - 72 };
const layout = { work_area: area, status, notice: placement };
function harness({ bar = null } = {}) {
  const workspace = new FakeElement("main"), answers = [], timers = [];
  let clock = 0, barBounds = bar;
  const element = (tag, className, text) => { const node = new FakeElement(tag, className); if (text != null) node.textContent = text; return node; };
  const button = (text, click, className = "") => { const node = element("button", className, text); node.addEventListener("click", click); return node; };
  const notice = createNotice({
    workspace, element, button, answer: (id, accept, action) => answers.push(action === undefined ? [id, accept] : [id, accept, action]),
    layout: () => layout, bar: () => barBounds,
    setTimer: (callback, ms) => { const timer = { id: timers.length + 1, callback, at: clock + ms }; timers.push(timer); return timer.id; },
    clearTimer: id => { const timer = timers.find(t => t.id === id); if (timer) timer.cleared = true; },
  });
  const advance = ms => {
    clock += ms;
    for (const timer of timers.filter(t => !t.cleared && !t.fired && t.at <= clock)) { timer.fired = true; timer.callback(); }
  };
  const [text, actions] = notice.root.children;
  return { notice, workspace, answers, advance, text, actions, get action() { return actions.children[0] ?? { hidden: true, click() {}, dispatch: () => ({}) }; },
    setBar: b => { barBounds = b; }, pending: () => timers.filter(t => !t.cleared && !t.fired) };
}
const refusal = id => ({ id, name: "Notice", text: "The active layer is locked", actions: [] });
const offer = id => ({ id, text: "This tool samples reference layers, and none is marked", actions: [{ id: "use_reference", label: "Use Photo as Reference", enabled: true, reason: null }] });
const objects = id => ({ id, text: "“Images” holds placed images, not paint.", actions: [
  { id: "add_mask", label: "Add Mask", enabled: true, reason: null },
  { id: "new_paint_layer", label: "New Paint Layer", enabled: true, reason: null },
  { id: "rasterize_layer", label: "Rasterize Layer", enabled: false, reason: "The active layer is locked" },
] });

test("a notice shows its text over the canvas without an action or a popover", () => {
  const h = harness();
  assert.equal(h.notice.root.parentNode, h.workspace);
  assert.equal(h.notice.root.hidden, true);
  assert.equal(h.notice.root.getAttribute("role"), "status");
  assert.equal(h.notice.root.popover, undefined, "never counts as an open popup");
  h.notice.publish(null);
  assert.equal(h.notice.root.hidden, true);
  h.notice.publish(refusal(1n));
  assert.equal(h.notice.root.hidden, false);
  assert.equal(h.text.textContent, "The active layer is locked");
  assert.equal(h.action.hidden, true);
  assert.equal(h.notice.root.style.transform, `translate(560px, ${floor - MARGIN}px) translate(-50%, -100%)`, "at the shared anchor");
  assert.equal(h.notice.root.style.maxWidth, `${placement.max_width}px`);
  assert.equal(h.notice.root.getAttribute("aria-label"), "Notice", "named by the shared notice name");
  assert.deepEqual(h.answers, []);
});

test("the action accepts its notice once, without taking focus", () => {
  const h = harness();
  h.notice.publish(offer(3n));
  assert.equal(h.actions.hidden, false);
  assert.equal(h.action.textContent, "Use Photo as Reference");
  assert.notEqual(h.action.tabIndex, -1, "Tab reaches the action");
  assert.equal(h.action.dispatch("mousedown").defaultPrevented, true, "a press does not move focus from the canvas");
  h.action.click();
  assert.equal(h.notice.root.hidden, true);
  assert.deepEqual(h.answers, [[3n, true, "use_reference"]]);
  h.action.click();
  h.advance(TIMEOUT_MS);
  assert.deepEqual(h.answers, [[3n, true, "use_reference"]], "neither a second tap nor the timeout answers again");
  h.notice.publish(offer(3n));
  assert.equal(h.notice.root.hidden, true, "the core clears an accepted notice; a republished id is not shown again");
});

test("a relocalized notice updates its text in place without restarting it", () => {
  const h = harness();
  h.notice.publish(offer(4n));
  const timers = h.pending().length;
  h.notice.publish({ id: 4n, text: "Dieses Werkzeug nutzt Referenzebenen", actions: [{ id: "use_reference", label: "Foto als Referenz verwenden", enabled: true, reason: null }] });
  assert.equal(h.text.textContent, "Dieses Werkzeug nutzt Referenzebenen");
  assert.equal(h.action.textContent, "Foto als Referenz verwenden");
  assert.equal(h.pending().length, timers, "the same notice keeps its timeout");
  assert.deepEqual(h.answers, []);
});

test("the timeout hides the notice and declines it", () => {
  const h = harness();
  h.notice.publish(offer(5n));
  h.advance(TIMEOUT_MS - 1);
  assert.equal(h.notice.root.hidden, false);
  assert.deepEqual(h.answers, []);
  h.advance(1);
  assert.equal(h.notice.root.hidden, true);
  assert.deepEqual(h.answers, [[5n, false]]);
  h.notice.publish(offer(5n));
  assert.equal(h.notice.root.hidden, true, "each id shows once");
  h.notice.publish(null);
  h.notice.publish(refusal(6n));
  assert.equal(h.notice.root.hidden, false, "a repeated refusal has a new id and shows again");
});

test("a canvas contact hides the notice without answering it", () => {
  const h = harness();
  h.notice.publish(offer(7n));
  h.notice.hide();
  assert.equal(h.notice.root.hidden, true);
  assert.equal(h.pending().length, 0);
  h.advance(TIMEOUT_MS * 2);
  assert.deepEqual(h.answers, [], "the core dismisses the notice at the same contact");
  h.notice.publish(offer(7n));
  assert.equal(h.notice.root.hidden, true, "a contact that raised nothing new keeps it hidden");
  h.notice.publish(refusal(8n));
  assert.equal(h.notice.root.hidden, false, "a notice raised by the contact shows");
});

test("a replacement notice restarts the timeout, and only the current id is answered", () => {
  const h = harness();
  h.notice.publish(offer(10n));
  h.advance(TIMEOUT_MS - 100);
  h.notice.publish(refusal(11n));
  assert.equal(h.text.textContent, "The active layer is locked");
  assert.equal(h.actions.hidden, true, "the replacement has no action");
  assert.equal(h.pending().length, 1);
  h.advance(TIMEOUT_MS - 1);
  assert.equal(h.notice.root.hidden, false);
  h.advance(1);
  assert.deepEqual(h.answers, [[11n, false]], "the replaced notice is never answered");
  h.notice.publish(offer(12n));
  h.notice.publish(offer(13n));
  h.action.click();
  assert.deepEqual(h.answers, [[11n, false], [13n, true, "use_reference"]]);
});

test("ordered actions keep their tokens, and a disabled choice shows its reason", () => {
  const h = harness();
  h.notice.publish(objects(20n));
  const [mask, paint, rasterize] = h.actions.children;
  assert.deepEqual([mask.textContent, paint.textContent, rasterize.textContent], ["Add Mask", "New Paint Layer", "Rasterize Layer"]);
  assert.equal(rasterize.disabled, true);
  assert.equal(rasterize.title, "The active layer is locked");
  assert.notEqual(paint.tabIndex, -1);
  paint.click();
  assert.deepEqual(h.answers, [[20n, true, "new_paint_layer"]]);
});

test("the notice follows the shared placement and sits above a canvas action bar along the bottom edge", () => {
  assert.deepEqual(noticePlacement(layout, null), { x: 560, y: floor - MARGIN, width: 720, minText: 200 });
  const edge = { x: 400, y: floor - MARGIN - 48, width: 300, height: 48 };
  assert.deepEqual(noticePlacement(layout, edge), { x: 560, y: edge.y - MARGIN, width: 720, minText: 200 });
  const near = { x: 400, y: 300, width: 300, height: 48 };
  assert.deepEqual(noticePlacement(layout, near).y, floor - MARGIN, "a bar beside an object elsewhere does not move it");
  assert.equal(noticePlacement({ notice: { ...placement, max_width: 376 } }, null).width, 376, "the shared width bounds the notice");
  const window = { ...placement, x: 550, y: 888, window: true, bar_floor: 900 };
  assert.deepEqual(noticePlacement({ notice: window }, edge), { x: 550, y: 888, width: 720, minText: 200 }, "a notice at the window's bottom ignores the canvas bar");
  const h = harness();
  h.notice.publish(refusal(1n));
  h.setBar(edge);
  h.notice.place();
  assert.equal(h.notice.root.style.transform, `translate(560px, ${edge.y - MARGIN}px) translate(-50%, -100%)`, "it follows a bar that appears later");
  h.notice.hide();
  h.setBar(null);
  h.notice.place();
  assert.equal(h.notice.root.style.transform, `translate(560px, ${edge.y - MARGIN}px) translate(-50%, -100%)`, "a hidden notice is not placed");
});

test("actions take their own row only when the text would be narrower than the shared minimum beside them", () => {
  const h = harness();
  h.text.width = 300; h.actions.width = 240;
  h.notice.publish(objects(30n));
  assert.equal(h.notice.root.classList.contains("stacked"), false, "short text and actions share a row");
  const content = placement.max_width - PADDING;
  h.text.width = 900; h.actions.width = content - GAP - placement.min_text_width;
  h.notice.place();
  assert.equal(h.notice.root.classList.contains("stacked"), false, "long text wraps beside actions that leave it the minimum width");
  h.actions.width += 1;
  h.notice.place();
  assert.equal(h.notice.root.classList.contains("stacked"), true, "wider actions move under the text");
  assert.equal(h.notice.root.style.maxWidth, `${placement.max_width}px`);
});
