import assert from 'node:assert/strict';

export function photoDimensions(actual, width, height) {
  assert.equal(width == null, height == null, 'Set both LAYER_PEN_PHOTO_WIDTH and LAYER_PEN_PHOTO_HEIGHT');
  const expected = width == null ? actual : { width: Number(width), height: Number(height) };
  for (const key of ['width', 'height']) {
    assert(Number.isSafeInteger(expected[key]) && expected[key] > 0, `Invalid photo ${key}`);
    assert.equal(actual[key], expected[key], `Imported photo ${key} differs from expected image`);
  }
  return expected;
}

export async function settleStroke(read, { before, timeoutMs = 30000,
  now = () => performance.now(), delay = ms => new Promise(resolve => setTimeout(resolve, ms)) } = {}) {
  const start = now();
  let state;
  do {
    state = await read();
    assert(!state.failure, `Canvas failed: ${state.failure}`);
    const pending = state.pending || state.busy || !state.ready ||
      ['input_pending', 'stroke_active', 'raster_backing_pending', 'file_pending',
        'painted_selection_pending', 'selection_edit_pending'].some(key => state.session?.[key]) ||
      state.session?.deferred_edits > 0;
    if (!pending && state.revision > before) return { ...state, elapsed_ms: now() - start };
    await delay(25);
  } while (now() - start < timeoutMs);
  throw Error(`Stroke did not settle within ${timeoutMs} ms: ${JSON.stringify({ before, outstanding: state })}`);
}
