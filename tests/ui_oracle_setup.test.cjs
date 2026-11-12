const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('ui/app.js', 'utf8');
function setup(fetch) {
  const elements = new Map();
  const get = id => {
    if (!elements.has(id)) elements.set(id, { disabled: false, textContent: '', children: [], replaceChildren() { this.children = []; }, append(item) { this.children.push(item); } });
    return elements.get(id);
  };
  const context = vm.createContext({ fetch, oraclePreviewGeneration: 0, oraclePreviewBusy: false, oraclePreviewReady: false, document: { getElementById: get, createElement: () => ({ textContent: '' }) } });
  vm.runInContext(source.slice(source.indexOf('    async function checkOracleReadiness()'), source.indexOf("    document.getElementById('checkOracleReadinessBtn').addEventListener")), context);
  return { context, get };
}
test('missing required Oracle dependencies disable Launch and explain setup', async () => {
  const h = setup(async () => ({ ok: true, json: async () => ({ ready: false, checks: [{ label: 'Java 17+', ready: false, required: true }], note: 'Checks run on server' }) }));
  await vm.runInContext('checkOracleReadiness()', h.context);
  assert.equal(h.get('launchOraclePreviewBtn').disabled, true);
  assert.match(h.get('oracleReadinessChecks').children[0].textContent, /Missing.*Java 17/);
  assert.match(h.get('oracleReadinessStatus').textContent, /setup steps/);
});
test('available dependencies enable Launch without requiring the optional local DB', async () => {
  const h = setup(async () => ({ ok: true, json: async () => ({ ready: true, checks: [{ label: 'Local DB', ready: false, required: false }], note: '' }) }));
  await vm.runInContext('checkOracleReadiness()', h.context);
  assert.equal(h.get('launchOraclePreviewBtn').disabled, false);
  assert.match(h.get('oracleReadinessChecks').children[0].textContent, /Optional/);
});
test('unavailable readiness endpoint leaves Launch disabled with restart instructions', async () => {
  const h = setup(async () => ({ ok: false }));
  await vm.runInContext('checkOracleReadiness()', h.context);
  assert.equal(h.get('launchOraclePreviewBtn').disabled, true);
  assert.equal(h.get('checkOracleReadinessBtn').disabled, false);
  assert.match(h.get('oracleReadinessStatus').textContent, /Restart the Rust server/);
});
