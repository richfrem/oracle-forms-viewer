const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('ui/app.js', 'utf8');
const html = fs.readFileSync('ui/index.html', 'utf8');
function setup() {
  const nodes = new Map([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], {
    value: '', handlers: {}, addEventListener(event, handler) { this.handlers[event] = handler; }, classList: { toggle() {} },
  }]));
  const swatches = [...html.matchAll(/class="swatch-btn"[^>]*data-fill="([^"]+)" data-fg="([^"]+)"/g)].map(match => ({
    handlers: {}, addEventListener(event, handler) { this.handlers[event] = handler; },
    getAttribute(name) { return name === 'data-fill' ? match[1] : match[2]; },
  }));
  let saves = 0, renders = 0;
  const context = vm.createContext({ document: { getElementById: id => nodes.get(id), querySelectorAll: selector => selector === '.swatch-btn' ? swatches : [] },
    selectedObject: { type: 'ITEM', item: {} }, renderCanvasSvg() { renders++; }, renderPropertyInspector() {},
    syncModuleToServer: async () => { saves++; }, normalizeHexColor: value => value, showBanner() {}, setTimeout() {}, hideBanner() {},
  });
  vm.runInContext(source.slice(source.indexOf('    // Wire Top Formatting Ribbon'), source.indexOf('    const addInFormNavBarBtn')), context);
  return { nodes, swatches, context, saves: () => saves, renders: () => renders };
}
test('toolbar color picker and preset update the selected item and save it', async () => {
  const h = setup();
  h.nodes.get('fmtBackColor').value = '#123456';
  await h.nodes.get('fmtBackColor').handlers.input();
  assert.equal(h.context.selectedObject.item.backColor, '#123456');
  await h.swatches[1].handlers.click();
  assert.equal(h.context.selectedObject.item.backColor, '#2563eb');
  assert.equal(h.context.selectedObject.item.foreColor, '#ffffff');
  assert.equal(h.nodes.get('fmtBackColor').value, '#2563eb');
  assert.equal(h.saves(), 2);
  assert.equal(h.renders(), 2);
});
test('toolbar font controls are connected to the visible HTML controls', async () => {
  const h = setup();
  h.nodes.get('fmtFontSize').value = '16';
  await h.nodes.get('fmtFontSize').handlers.change();
  await h.nodes.get('fmtBoldBtn').handlers.click();
  assert.equal(h.context.selectedObject.item.fontSize, 16);
  assert.equal(h.context.selectedObject.item.fontWeight, 'bold');
  assert.equal(h.saves(), 2);
});
