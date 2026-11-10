/* Interactive form controls with live database query and simulation capabilities. */
window.FormsPreview = class FormsPreview {
  constructor(onAction = () => {}, onFocus = () => {}, onEdit = () => {}) {
    this.onAction = onAction;
    this.onFocus = onFocus;
    this.onEdit = onEdit;
    this.values = new Map();
  }

  load(module) {
    this.module = module;
    this.reset();
  }

  reset() {
    this.values.clear();
  }

  setValue(blockName, itemName, row, value) {
    const key = JSON.stringify([blockName.toUpperCase(), itemName.toUpperCase(), row]);
    this.values.set(key, value);
  }

  getValue(blockName, itemName, row) {
    const key = JSON.stringify([blockName.toUpperCase(), itemName.toUpperCase(), row]);
    return this.values.get(key);
  }

  setRecord(blockName, record, row = 0) {
    if (!record) return;
    const bName = blockName.toUpperCase();
    const rowData = (record && typeof record === 'object' && record.values) ? record.values : record;
    for (const [col, val] of Object.entries(rowData)) {
      const v = val != null ? String(val) : '';
      this.values.set(JSON.stringify([bName, col.toUpperCase(), row]), v);
    }
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
    const bName = block.name.toUpperCase();
    const iName = item.name.toUpperCase();
    const colName = (item.columnName || '').toUpperCase();
    const key = JSON.stringify([bName, iName, row]);
    const colKey = colName ? JSON.stringify([bName, colName, row]) : null;

    const dml = this.properties(item);
    const type = (item.itemType || '').toLowerCase();
    const entities = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };
    const label = (item.label || item.prompt || item.name)
      .replace(/&(lt|gt|amp|quot|apos);/g, (_, name) => entities[name]);
    const button = type.includes('button') || item.name.toUpperCase().startsWith('BTN');
    const checkbox = type.includes('check');
    const radio = type.includes('radio');

    if (!this.values.has(key)) {
      let initial = '';
      if (colKey && this.values.has(colKey)) {
        initial = this.values.get(colKey);
      } else {
        initial = dml.initialValue || '';
      }
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
      const curVal = this.values.get(key) || (colKey ? this.values.get(colKey) : null);
      const option = document.createElement('option');
      option.textContent = curVal || 'Select...';
      option.value = curVal || '';
      control.append(option);
      control.title = 'List of Values';
    } else {
      control = document.createElement('input');
      control.type = checkbox ? 'checkbox' : radio ? 'radio' : (item.concealData ? 'password' : 'text');
      if (checkbox || radio) {
        control.checked = !!this.values.get(key);
        control.addEventListener('change', () => {
          this.values.set(key, control.checked);
          this.onEdit(block, item, row, control.checked);
        });
      } else {
        const curVal = this.values.get(key) != null ? this.values.get(key) : (colKey ? this.values.get(colKey) : '');
        control.value = curVal != null ? curVal : '';
        control.readOnly = type.includes('display') || dml.keyboardNavigable === false ||
          (item.propertyClass || '').toUpperCase().includes('NONENTERABLE');
        if (dml.maximumLength > 0) control.maxLength = dml.maximumLength;
        control.addEventListener('input', () => this.values.set(key, control.value));
        // Leaving the field commits the value (and fires WHEN-VALIDATE-ITEM in a live session).
        control.addEventListener('change', () => this.onEdit(block, item, row, control.value));
      }
      if (radio) {
        control.disabled = true;
        control.title = 'Radio choices are not available in the parsed index.';
      }
    }
    const normColor = (c) => {
      if (!c) return null;
      const s = String(c).trim();
      const m = /^r(\d+)g(\d+)b(\d+)$/i.exec(s);
      if (m) {
        const scale = (Number(m[1]) <= 100 && Number(m[2]) <= 100 && Number(m[3]) <= 100 && s === s.toLowerCase()) ? 2.55 : 1;
        return `rgb(${Math.round(Number(m[1]) * scale)}, ${Math.round(Number(m[2]) * scale)}, ${Math.round(Number(m[3]) * scale)})`;
      }
      return s;
    };
    const bg = normColor(item.backColor);
    const fg = normColor(item.foreColor);
    if (bg) control.style.background = bg;
    if (fg) control.style.color = fg;
    if (item.fontName) control.style.fontFamily = item.fontName;
    if (item.fontSize) control.style.fontSize = `${item.fontSize}px`;
    if (item.fontWeight) control.style.fontWeight = item.fontWeight.toLowerCase();
    if (item.fontStyle) control.style.fontStyle = item.fontStyle.toLowerCase();
    if (item.justification) {
      const j = item.justification.toLowerCase();
      control.style.textAlign = (j === 'end' || j === 'right') ? 'right' : (j === 'center' ? 'center' : 'left');
    }
    if (item.caseRestriction) {
      const cr = item.caseRestriction.toLowerCase();
      if (cr === 'upper') control.style.textTransform = 'uppercase';
      else if (cr === 'lower') control.style.textTransform = 'lowercase';
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
