const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const context = vm.createContext({ window: {} });
vm.runInContext(fs.readFileSync('ui/runtime.js', 'utf8'), context);
const Runtime = context.window.FormsRuntime;

test('Oracle Run requires credentials when server credential use is not selected', () => {
  for (const backend of ['oracle', 'oracle-thin']) {
    assert.equal(Runtime.requiresCredentials({ backend }), true);
    assert.equal(Runtime.requiresCredentials({ backend, username: '   ', password: 'test' }), true);
    assert.equal(Runtime.requiresCredentials({ backend, username: 'test' }), true);
    assert.equal(Runtime.requiresCredentials({ backend, username: 'test', password: 'test' }), false);
  }
});
test('offline backends do not require credentials', () => {
  for (const backend of ['sqlite', 'mock', '']) assert.equal(Runtime.requiresCredentials({ backend }), false);
});
test('explicit server credential choice defers authorization to the server', () => {
  assert.equal(Runtime.requiresCredentials({ backend: 'oracle-thin', useServerCredentials: true }), false);
});
