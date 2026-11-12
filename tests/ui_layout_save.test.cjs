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
