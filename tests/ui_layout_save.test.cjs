const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('ui/app.js', 'utf8');
function setup(fail = false) {
  const module = { key: { name: 'ORDERS' }, canvases: [{ name: 'MAIN', tabPages: [] }], blocks: [
    { name: 'ORDERS', items: [
      { name: 'ORDER_ID', canvasName: 'MAIN', width: 60, height: 17 },
      { name: 'CUSTOMER', canvasName: 'MAIN' },
      { name: 'EXPLICIT', canvasName: 'MAIN', xPosition: 400, yPosition: 200, width: 100, height: 20 },
    ] },
    { name: 'LINES', items: [ { name: 'HIDDEN_ID', canvasName: 'MAIN', visible: false }, { name: 'SAVE', prompt: 'Save record', itemType: 'Push Button', canvasName: 'MAIN' } ] },
  ] };
  const requests = [];
  const context = vm.createContext({ currentPayload: { moduleIndex: module }, checkedSave: async (url, options) => {
    if (fail) throw new Error('Server unavailable');
    requests.push(JSON.parse(options.body));
  }, renderCanvasSvg() {} });
  vm.runInContext(source.slice(source.indexOf('  function canvasCoordinateScale('), source.indexOf('  // Visual Canvas 2D SVG Rendering')), context);
  return { context, module, requests };
}
test('save persists the same fallback positions used by the canvas and preserves explicit geometry/visibility', async () => {
  const h = setup();
  const count = await vm.runInContext('persistDesignerAutoLayout()', h.context);
  assert.equal(count, 4);
  assert.equal(h.requests[0].x, 140);
  assert.equal(h.requests[0].y, 60);
  assert.equal(h.requests[1].y, 91);
  assert.equal(h.module.blocks[0].items[2].xPosition, 400);
  assert.equal(h.module.blocks[1].items[0].visible, false);
  assert.equal(h.module.blocks[1].items[1].height, 33);
  assert.equal(h.module.blocks[1].items[1].label, 'Save record');
  assert.ok(h.requests.every(request => request.width > 0 && request.height > 0));
  const again = await vm.runInContext('persistDesignerAutoLayout()', h.context);
  assert.equal(again, 0);
});
test('failed persistence does not mark the item geometry as saved', async () => {
  const h = setup(true);
  await assert.rejects(vm.runInContext('persistDesignerAutoLayout()', h.context), /Server unavailable/);
  assert.equal(h.module.blocks[0].items[0].xPosition, undefined);
});

test('fallback pixel positions convert back to Oracle units before persistence', async () => {
  const h = setup();
  h.module.coordinate = { coordinateSystem: 'Real', realUnit: 'Centimeter' };
  h.module.canvases[0].width = 10000;
  await vm.runInContext('persistDesignerAutoLayout()', h.context);
  assert.equal(Math.round(h.requests[0].x * 37.79527559 / 1000), 140);
  assert.equal(Math.round(h.requests[0].y * 37.79527559 / 1000), 60);
});

test('already positioned buttons acquire their visible caption without moving or resizing', async () => {
  const h = setup();
  const button = h.module.blocks[1].items[1];
  Object.assign(button, { xPosition: 300, yPosition: 400, width: 110, height: 25 });
  await vm.runInContext('persistDesignerAutoLayout()', h.context);
  assert.equal(button.xPosition, 300);
  assert.equal(button.yPosition, 400);
  assert.equal(button.width, 110);
  assert.equal(button.height, 25);
  assert.equal(button.label, 'Save record');
  assert.equal(button.prompt, '');
  const request = h.requests.find(value => value.itemName === 'SAVE');
  assert.equal(request.label, 'Save record');
  assert.equal(request.prompt, '');
});

function renderLayer(items, options = {}) {
  const canvas = { name: 'MAIN', width: 300, height: 150, tabPages: [] };
  const context = vm.createContext({
    currentPayload: { moduleIndex: { blocks: [{ name: 'ORDERS', items }], graphics: [] } },
    showNullCanvas: false, layoutEditMode: false, previewEnabled: false, selectedObject: null,
    queryRecordsByBlock: new Map(), activeRecordIndexByBlock: new Map(),
    escapeHtml: value => String(value), normalizeOracleColor: value => value,
    canvas,
  });
  vm.runInContext(source.slice(source.indexOf('  function canvasCoordinateScale('), source.indexOf('  // Visual Canvas 2D SVG Rendering')), context);
  vm.runInContext(source.slice(source.indexOf('  function buildCanvasLayer('), source.indexOf('  /** Index of the first record shown')), context);
  context.options = options;
  return vm.runInContext('buildCanvasLayer(canvas, null, options)', context);
}

test('opening a form with unpositioned items renders auto-layout within the canvas bounds', () => {
  const items = Array.from({ length: 20 }, (_, i) => ({
    name: `FIELD_${i}`, canvasName: 'MAIN', itemType: 'Text Item', width: 130, height: 22,
  }));
  const layer = renderLayer(items);
  assert.equal((layer.svgHtml.match(/class="svg-interactive-item/g) || []).length, items.length);
  const lastItemBottom = 60 + (items.length - 1) * 36 + 22;
  assert.ok(layer.height >= lastItemBottom + 30, 'canvas contains the final auto-positioned field');
  assert.doesNotMatch(layer.svgHtml, /NaN|undefined/);
});

test('positioned and empty canvas layers still render, including overlay layers', () => {
  const positioned = renderLayer([{ name: 'ORDER_ID', canvasName: 'MAIN', xPosition: 140, yPosition: 400, width: 60, height: 22 }]);
  assert.equal(positioned.height, 452);
  assert.match(positioned.svgHtml, /data-item="ORDER_ID"/);
  const empty = renderLayer([], { overlay: true });
  assert.equal(empty.height, 180);
  assert.equal(empty.svgHtml, '');
});
