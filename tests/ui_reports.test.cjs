// Optional UI regression checks: node --test tests/ui_reports.test.cjs
// Uses only Node's built-in test tools; the shipped workbench remains runtime-independent.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync('ui/app.js', 'utf8');

function reportHarness(fetch) {
  const elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, {
      textContent: '', disabled: false, srcdoc: '', classList: { contains: () => false },
      replaceChildren() {},
    });
    return elements.get(id);
  };
  const context = vm.createContext({
    fetch, document: { getElementById: element, querySelectorAll: () => [{ name: 'P_MIN', value: '25' }] },
    reportSelect: { value: 'items.xml' }, reportBackendSelect: { value: 'sqlite' },
    runReportBtn: element('run'), printReportBtn: element('print'), reportPreviewFrame: element('frame'),
    reportDataTree: element('tree'), currentReportPayload: null,
    applyReportTemplateTheme: html => html, renderReportDataTree() {}, renderReportPlsqlStudio() {},
  });
  vm.runInContext(source.slice(source.indexOf('  let reportRequestVersion'), source.indexOf('  function renderReportPlsqlStudio')), context);
  vm.runInContext("reportDefinitionName = 'items.xml'", context);
  return { context, element };
}

test('report parameters reach the API and successful output enables printing', async () => {
  let request;
  const { context, element } = reportHarness(async (_, options) => {
    request = JSON.parse(options.body);
    return { ok: true, json: async () => ({ html: '<p>Filtered output</p>', report: {}, execution: {} }) };
  });
  await vm.runInContext('executeReport()', context);
  assert.deepEqual(request.parameters, { P_MIN: '25' });
  assert.equal(element('frame').srcdoc, '<p>Filtered output</p>');
  assert.equal(element('print').disabled, false);
  assert.equal(element('run').disabled, false);
});

test('a failed report clears stale output and exposes the server error', async () => {
  const { context, element } = reportHarness(async () => ({ ok: false, json: async () => ({ error: 'Connection refused' }) }));
  element('frame').srcdoc = '<p>Old output</p>';
  await vm.runInContext('executeReport()', context);
  assert.equal(element('frame').srcdoc, '');
  assert.equal(element('print').disabled, true);
  assert.match(element('reportStatus').textContent, /Connection refused/);
  assert.equal(context.currentReportPayload, null);
});

test('a result arriving after inputs change cannot restore stale output', async () => {
  let complete;
  const { context, element } = reportHarness(() => new Promise(resolve => { complete = resolve; }));
  const running = vm.runInContext('executeReport()', context);
  vm.runInContext("invalidateReport('Parameters changed')", context);
  complete({ ok: true, json: async () => ({ html: '<p>Outdated</p>', report: {}, execution: {} }) });
  await running;
  assert.equal(element('frame').srcdoc, '');
  assert.equal(element('print').disabled, true);
  assert.equal(context.currentReportPayload, null);
});

test('HTTP save failures reject instead of allowing a saved confirmation', async () => {
  const context = vm.createContext({ fetch: async () => ({ ok: false, status: 500, json: async () => ({ error: 'Cannot save module' }) }) });
  vm.runInContext(source.slice(source.indexOf('  async function checkedSave'), source.indexOf('  async function syncModuleToServer')), context);
  await assert.rejects(vm.runInContext("checkedSave('/save', {})", context), /Cannot save module/);
});

test('report source inspection never invents missing formulas or lifecycle triggers', () => {
  const elements = new Map();
  const context = vm.createContext({
    document: { getElementById(id) {
      if (!elements.has(id)) elements.set(id, { textContent: '', innerHTML: '', querySelectorAll: () => [] });
      return elements.get(id);
    } },
    escapeHtml: value => String(value), highlightPlsql: value => value,
  });
  vm.runInContext(source.slice(source.indexOf('  function renderReportPlsqlStudio'), source.indexOf('  function renderReportDataTree')), context);
  context.report = { name: 'R', data: { data_sources: [{ name: 'Q', select_sql: 'SELECT 1', groups: [{ name: 'G', formulas: [{ name: 'CF_EMPTY', datatype: 'number' }] }] }] } };
  vm.runInContext('renderReportPlsqlStudio(report, null)', context);
  assert.match(elements.get('reportPlsqlDisplay').innerHTML, /No formula source was extracted/);
  assert.doesNotMatch(elements.get('reportCodeUnitsList').innerHTML, /BEFORE_REPORT|AFTER_REPORT|AFTER_PARAMETER_FORM|SALARY/);
});
