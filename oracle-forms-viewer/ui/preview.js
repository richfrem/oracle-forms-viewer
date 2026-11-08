/* Local-only form controls. No source code or database actions are executed. */
window.FormsPreview = class FormsPreview {
  constructor(onAction = () => {}, onFocus = () => {}) {
    this.onAction = onAction;
    this.onFocus = onFocus;
    this.values = new Map();
  }

  load(module) {
    this.module = module;
    this.reset();
  }

  reset() {
    this.values.clear();
  }

  properties(item) {
    const classes = this.module?.propertyClassDetails || [];
    const seen = new Set();
    let name = item.propertyClass;
    const dml = { ...(item.dml || {}) };
    while (name && !seen.has(name.toUpperCase()) && seen.size < 10) {
      seen.add(name.toUpperCase());
      const cls = classes.find(c => c.name.toUpperCase() === name.toUpperCase());
      if (!cls) break;
      for (const [key, value] of Object.entries(cls.item || {})) {
        if (dml[key] == null) dml[key] = value;
      }
      name = cls.propertyClass;
    }
    return dml;
  }

  control(block, item, row) {
    const key = JSON.stringify([block.name, item.name, row]);
    const dml = this.properties(item);
    const type = (item.itemType || '').toLowerCase();
    const entities = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };
    const label = (item.label || item.prompt || item.name)
      .replace(/&(lt|gt|amp|quot|apos);/g, (_, name) => entities[name]);
    const button = type.includes('button') || item.name.toUpperCase().startsWith('BTN');
    const checkbox = type.includes('check');
    const radio = type.includes('radio');
    if (!this.values.has(key)) {
      const initial = dml.initialValue || '';
      this.values.set(key, checkbox || radio ? /^(true|yes|y|1)$/i.test(initial) : initial);
    }
    const wrapper = document.createElement('div');
    wrapper.className = 'preview-widget';
    let control;
    if (button) {
      control = document.createElement('button');
      control.type = 'button';
      control.textContent = label;
      control.addEventListener('click', () => this.onAction(block, item, row));
    } else if (type.includes('list')) {
      control = document.createElement('select');
      const option = document.createElement('option');
      option.textContent = this.values.get(key) || 'List values not exported';
      option.value = this.values.get(key);
      control.append(option);
      control.disabled = true;
      control.title = 'List options are not available in the parsed index. No query is executed.';
    } else {
      control = document.createElement('input');
      control.type = checkbox ? 'checkbox' : radio ? 'radio' : 'text';
      if (checkbox || radio) {
        control.checked = !!this.values.get(key);
        control.addEventListener('change', () => this.values.set(key, control.checked));
      } else {
        control.value = this.values.get(key);
        control.readOnly = type.includes('display') || dml.keyboardNavigable === false ||
          (item.propertyClass || '').toUpperCase().includes('NONENTERABLE');
        if (dml.maximumLength > 0) control.maxLength = dml.maximumLength;
        control.addEventListener('input', () => this.values.set(key, control.value));
      }
      if (radio) {
        control.disabled = true;
        control.title = 'Radio choices are not available in the parsed index.';
      }
    }
    control.disabled = control.disabled || item.enabled === false || dml.enabled === false;
    control.setAttribute('aria-label', `${block.name}.${item.name}, record ${row + 1}`);
    control.dataset.block = block.name;
    control.dataset.item = item.name;
    control.dataset.record = row;
    control.addEventListener('focus', () => this.onFocus(block, item, row));
    wrapper.append(control);
    if (checkbox || radio) {
      const text = document.createElement('span');
      text.textContent = label;
      wrapper.append(text);
    }
    return wrapper;
  }
};
