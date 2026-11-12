const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('ui/app.js', 'utf8');
function setup(checkedSave) {
  const nodes = new Map();
  const get = id => {
    if (!nodes.has(id)) nodes.set(id, { textContent: '', value: id === 'oracleCompileMode' ? 'offline' : '', focus() {}, disabled: false, open: false, handlers: {}, addEventListener(type, callback) { this.handlers[type] = callback; } });
    return nodes.get(id);
  };
  const context = vm.createContext({ document: { getElementById: get },
    currentPayload: { moduleIndex: { key: { name: 'EMPLOYEE' } } },
    dbUserInput: { value: '' }, dbPassInput: { value: '' },
    syncModuleToServer: async () => {}, persistDesignerAutoLayout: async () => 0, checkedSave, fetch: async () => ({ ok: true, json: async () => ({ running: false, success: true, log: 'finished' }) }),
    setTimeout: callback => callback(),
  });
  vm.runInContext(source.slice(source.indexOf('    let oraclePreviewGeneration'), source.indexOf('    async function checkOracleReadiness')), context);
  vm.runInContext(source.slice(source.indexOf("    document.getElementById('launchOraclePreviewBtn').addEventListener"), source.indexOf("    document.addEventListener('click', event =>")), context);
  return { context, get };
}
test('switching forms clears old logs and closes diagnostics', () => {
  const h = setup();
  vm.runInContext("resetOraclePreview('EMPLOYEE')", h.context);
  h.get('oraclePreviewLog').textContent = 'Employee output';
  h.get('oraclePreviewDiagnostics').open = true;
  vm.runInContext("resetOraclePreview('CLAIMS')", h.context);
  assert.equal(h.get('oraclePreviewLog').textContent, 'No launch output yet.');
  assert.match(h.get('oraclePreviewSummary').textContent, /CLAIMS/);
  assert.equal(h.get('oraclePreviewDiagnostics').open, false);
});
test('old launch response cannot replace the selected forms status', async () => {
  let finish;
  const h = setup(() => new Promise(resolve => { finish = resolve; }));
  vm.runInContext("resetOraclePreview('EMPLOYEE'); oraclePreviewReady = true; document.getElementById('oracleCompileMode').value = 'offline'", h.context);
  const button = h.get('launchOraclePreviewBtn');
  const pending = button.handlers.click({ target: button });
  await new Promise(resolve => setImmediate(resolve));
  h.context.currentPayload.moduleIndex.key.name = 'CLAIMS';
  vm.runInContext("resetOraclePreview('CLAIMS')", h.context);
  finish({ json: async () => ({ jobId: 'old', fileName: 'employee.xml', mode: 'summary' }) });
  await pending;
  assert.equal(h.get('oraclePreviewLog').textContent, 'No launch output yet.');
  assert.match(h.get('oraclePreviewSummary').textContent, /CLAIMS/);
});
test('stylesheet remains CSS and essential page IDs stay unique', () => {
  const css = fs.readFileSync('ui/styles.css', 'utf8');
  assert.doesNotMatch(css, /<!DOCTYPE|<html|<body/i);
  assert.match(css, /:root\s*\{/);
  const ids = [...fs.readFileSync('ui/index.html', 'utf8').matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
});

test('connected preview requires its own password before saving or starting a job', async () => {
  let started = false;
  const h = setup(() => { started = true; });
  vm.runInContext("resetOraclePreview('EMPLOYEE')", h.context);
  h.get('oracleCompileMode').value = 'connected';
  h.get('oracleCompileUser').value = 'forms_user';
  h.get('oracleCompilePassword').value = '';
  await h.get('launchOraclePreviewBtn').handlers.click({ target: h.get('launchOraclePreviewBtn') });
  assert.equal(started, false);
  assert.match(h.get('oraclePreviewSummary').textContent, /Enter the database password/);
});
test('connected preview sends dedicated compile credentials and clears the password afterward', async () => {
  let sent;
  const h = setup(async (_, options) => {
    sent = JSON.parse(options.body);
    return { json: async () => ({ jobId: 'test', fileName: 'employee.xml', mode: 'patchedOriginal' }) };
  });
  vm.runInContext("resetOraclePreview('EMPLOYEE'); oraclePreviewReady = true; document.getElementById('oracleCompileMode').value = 'offline'", h.context);
  h.get('oracleCompileMode').value = 'connected';
  h.get('oracleCompileUser').value = 'compile_user';
  h.get('oracleCompilePassword').value = 'test-only';
  await h.get('launchOraclePreviewBtn').handlers.click({ target: h.get('launchOraclePreviewBtn') });
  assert.equal(sent.offline, false);
  assert.equal(sent.username, 'compile_user');
  assert.equal(sent.password, 'test-only');
  assert.equal(h.get('oracleCompilePassword').value, '');
});
