/**
 * Oracle Forms Local Viewer POC
 */

(function () {
  let currentPayload = null;
  let activeCanvas = null;
  let activeTabPage = null;
  let selectedObject = null;
  let zoomLevel = 1.0;
  let showNullCanvas = false;
  let previewEnabled = false;
  let layoutEditMode = false;
  let menuSourceModule = null;
  let previewSource = null;
  const preview = new FormsPreview(
    (block, item) => {
      const trigger = currentPayload.moduleIndex.triggers.find(t =>
        t.level === 'ITEM' && t.blockName === block.name && t.itemName === item.name &&
        t.name.toUpperCase() === 'WHEN-BUTTON-PRESSED');
      showPreviewDialog(item.label || item.name,
        'Button action not executed. Application navigation and trigger effects are not simulated.',
        trigger?.textRef ? { name: trigger.name, file: trigger.textRef.file, module: currentPayload.summary.moduleName } : null);
    },
    (block, item, row) => {
      selectedObject = { type: 'ITEM', block, item };
      renderPropertyInspector(selectedObject);
      selectedItemStats.textContent = `Preview: ${block.name}.${item.name}, record ${row + 1}`;
      oracleStatusMsg.textContent = 'Local preview only - no database or PL/SQL execution';
    }
  );

  // DOM Elements
  const fileInput = document.getElementById('fileInput');
  const filePathInput = document.getElementById('filePathInput');
  const openPathBtn = document.getElementById('openPathBtn');
  const sampleSelect = document.getElementById('sampleSelect');
  const loadSampleBtn = document.getElementById('loadSampleBtn');
  const moduleBadge = document.getElementById('moduleBadge');
  const canvasSelect = document.getElementById('canvasSelect');
  const tabPagePills = document.getElementById('tabPagePills');
  const zoomSlider = document.getElementById('zoomSlider');
  const zoomValue = document.getElementById('zoomValue');
  const showNullCanvasToggle = document.getElementById('showNullCanvasToggle');
  const runtimeFrameToggle = document.getElementById('runtimeFrameToggle');
  const oracleWindowShell = document.getElementById('oracleWindowShell');
  const oracleWindowTitle = document.getElementById('oracleWindowTitle');
  const oracleStatusMsg = document.getElementById('oracleStatusMsg');
  const conversionBanner = document.getElementById('conversionBanner');
  const bannerMessage = document.getElementById('bannerMessage');
  const bannerDismissBtn = document.getElementById('bannerDismissBtn');
  const bannerSampleBtn = document.getElementById('bannerSampleBtn');

  // DB Query Controls
  const dbBackendSelect = document.getElementById('dbBackendSelect');
  const dbQueryBtn = document.getElementById('dbQueryBtn');
  const recordNavControls = document.getElementById('recordNavControls');
  const prevRecordBtn = document.getElementById('prevRecordBtn');
  const nextRecordBtn = document.getElementById('nextRecordBtn');
  const recordNavBadge = document.getElementById('recordNavBadge');

  const runtimePrevRecordBtn = document.getElementById('runtimePrevRecordBtn');
  const runtimeNextRecordBtn = document.getElementById('runtimeNextRecordBtn');
  const runtimeExecuteQueryBtn = document.getElementById('runtimeExecuteQueryBtn');

  // Query engine state
  const queryRecordsByBlock = new Map();
  const activeRecordIndexByBlock = new Map();

  const treeFilterInput = document.getElementById('treeFilterInput');
  const navigatorTree = document.getElementById('navigatorTree');
  const formsSvg = document.getElementById('formsSvg');
  const svgContainer = document.getElementById('svgContainer');
  const canvasStats = document.getElementById('canvasStats');
  const selectedItemStats = document.getElementById('selectedItemStats');

  const propertyInspector = document.getElementById('propertyInspector');
  const inspectorTargetName = document.getElementById('inspectorTargetName');

  const plsqlCodeDisplay = document.getElementById('plsqlCodeDisplay');
  const codeTitle = document.getElementById('codeTitle');
  const codeScopeBadge = document.getElementById('codeScopeBadge');
  const codeTabBadge = document.getElementById('codeTabBadge');

  const xmlSliceDisplay = document.getElementById('xmlSliceDisplay');
  const xmlTitle = document.getElementById('xmlTitle');
  const xmlLinesBadge = document.getElementById('xmlLinesBadge');

  const editLayoutToggleBtn = document.getElementById('editLayoutToggleBtn');
  const exportXmlBtn = document.getElementById('exportXmlBtn');
  const diffCompareSelect = document.getElementById('diffCompareSelect');
  const runDiffBtn = document.getElementById('runDiffBtn');
  const diffContainer = document.getElementById('diffContainer');

  const reportSelect = document.getElementById('reportSelect');
  const reportBackendSelect = document.getElementById('reportBackendSelect');
  const runReportBtn = document.getElementById('runReportBtn');
  const printReportBtn = document.getElementById('printReportBtn');
  const reportPreviewFrame = document.getElementById('reportPreviewFrame');
  const reportDataTree = document.getElementById('reportDataTree');

  // Initialize
  document.addEventListener('DOMContentLoaded', () => {
    setupTabSwitching();
    setupControls();
    loadSampleList();
    loadReportsList();
    // Pre-load default sample
    loadSample('orders_fmb.xml');
  });

  // Tab switching
  function setupTabSwitching() {
    document.querySelectorAll('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
        document.querySelectorAll('.tab-view').forEach(v => v.classList.remove('active'));
        btn.classList.add('active');
        const viewId = btn.getAttribute('data-tab');
        document.getElementById(viewId).classList.add('active');
      });
    });
  }

  function switchTab(tabId) {
    const btn = document.querySelector(`.tab-btn[data-tab="${tabId}"]`);
    if (btn) btn.click();
  }

  // Controls setup
  function setupControls() {
    const previewToggle = document.getElementById('previewToggle');
    previewToggle.addEventListener('change', () => {
      previewEnabled = previewToggle.checked;
      document.getElementById('previewResetBtn').disabled = !previewEnabled;
      document.getElementById('previewAlertSelect').disabled = !previewEnabled;
      if (previewEnabled) {
        runtimeFrameToggle.checked = true;
        oracleWindowShell.classList.remove('hidden-frame');
      }
      renderCanvasSvg();
      if (previewEnabled) setTimeout(zoomFit, 50);
    });
    document.getElementById('previewResetBtn').addEventListener('click', () => {
      preview.reset();
      renderCanvasSvg();
    });
    document.getElementById('previewAlertSelect').addEventListener('change', (event) => {
      const alert = currentPayload?.moduleIndex.alerts.find(a => a.name === event.target.value);
      if (alert) showPreviewDialog(alert.name, alert.message || 'No alert message was exported.');
      event.target.value = '';
    });
    document.getElementById('previewCloseBtn').addEventListener('click', () => document.getElementById('previewDialog').close());
    document.getElementById('previewSourceBtn').addEventListener('click', () => {
      document.getElementById('previewDialog').close();
      if (previewSource) loadCode(previewSource.name, previewSource.file, 'PREVIEW ACTION', previewSource.module);
    });
    document.getElementById('loadMenuBtn').addEventListener('click', () => {
      if (!currentPayload) return showBanner('Open a form before loading its companion menu XML.');
      document.getElementById('menuFileInput').click();
    });
    document.getElementById('menuFileInput').addEventListener('change', async (event) => {
      const file = event.target.files[0];
      if (!file || !currentPayload) return;
      const form = currentPayload;
      try {
        const data = new FormData();
        data.append('file', file);
        const response = await fetch('/api/open-file', { method: 'POST', body: data });
        const payload = await response.json();
        if (!response.ok) throw new Error(payload.error || 'Failed to load menu XML.');
        if (currentPayload !== form) return;
        if (!payload.moduleIndex.menus?.length) throw new Error('This XML contains no menu definitions.');
        menuSourceModule = payload.summary.moduleName;
        form.moduleIndex.menus = payload.moduleIndex.menus;
        form.moduleIndex.menuModule = menuSourceModule;
        renderRuntimeMenuBar(form.moduleIndex);
        renderNavigatorTree();
        hideBanner();
      } catch (error) {
        showBanner(error.message);
      } finally {
        event.target.value = '';
      }
    });
    document.querySelector('.oracle-action-toolbar').addEventListener('click', (event) => {
      const button = event.target.closest('button');
      if (!button) return;
      if (button.id === 'runtimeExecuteQueryBtn' || button.id === 'runtimePrevRecordBtn' || button.id === 'runtimeNextRecordBtn') {
        return;
      }
      if (previewEnabled && button) showPreviewDialog(button.title, 'Toolbar action not executed in this local preview.');
    });

    // Database query and navigation controls
    if (dbQueryBtn) {
      dbQueryBtn.addEventListener('click', () => executeBlockQuery());
    }
    if (prevRecordBtn) {
      prevRecordBtn.addEventListener('click', () => navigateRecord('prev'));
    }
    if (nextRecordBtn) {
      nextRecordBtn.addEventListener('click', () => navigateRecord('next'));
    }
    if (runtimeExecuteQueryBtn) {
      runtimeExecuteQueryBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        executeBlockQuery();
      });
    }
    if (runtimePrevRecordBtn) {
      runtimePrevRecordBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        navigateRecord('prev');
      });
    }
    if (runtimeNextRecordBtn) {
      runtimeNextRecordBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        navigateRecord('next');
      });
    }
    window.addEventListener('keydown', (e) => {
      if ((e.ctrlKey && e.key === 'F11') || e.key === 'F8') {
        e.preventDefault();
        executeBlockQuery();
      }
    });
    fileInput.addEventListener('change', (e) => {
      if (e.target.files.length > 0) {
        uploadFile(e.target.files[0]);
      }
    });

    openPathBtn.addEventListener('click', () => {
      const path = filePathInput.value.trim();
      if (path) openFilePath(path);
    });

    filePathInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const path = filePathInput.value.trim();
        if (path) openFilePath(path);
      }
    });

    loadSampleBtn.addEventListener('click', () => {
      const selected = sampleSelect.value;
      if (selected) loadSample(selected);
    });

    if (editLayoutToggleBtn) {
      editLayoutToggleBtn.addEventListener('click', () => {
        layoutEditMode = !layoutEditMode;
        document.body.classList.toggle('edit-layout-active', layoutEditMode);
        editLayoutToggleBtn.textContent = layoutEditMode ? '✔️ Done Editing' : '✏️ Edit Layout';
        editLayoutToggleBtn.classList.toggle('btn-primary', layoutEditMode);
        editLayoutToggleBtn.classList.toggle('btn-secondary', !layoutEditMode);
        if (selectedObject) {
          renderPropertyInspector(selectedObject);
        }
        renderCanvasSvg();
      });
    }

    if (exportXmlBtn) {
      exportXmlBtn.addEventListener('click', async () => {
        if (!currentPayload) return showBanner('No module open to export.');
        try {
          const res = await fetch('/api/forms/export-xml', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ module: currentPayload.moduleIndex })
          });
          if (!res.ok) throw new Error('Export XML failed');
          const data = await res.json();
          const blob = new Blob([data.xml], { type: 'application/xml' });
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `${currentPayload.moduleIndex.key.name}_fmb.xml`;
          document.body.appendChild(a);
          a.click();
          document.body.removeChild(a);
          URL.revokeObjectURL(url);
          showBanner(`Exported ${currentPayload.moduleIndex.key.name}_fmb.xml successfully.`);
          setTimeout(hideBanner, 3000);
        } catch (err) {
          showBanner("Failed to export XML: " + err.message);
        }
      });
    }

    if (runDiffBtn) {
      runDiffBtn.addEventListener('click', async () => {
        if (!currentPayload) return showBanner('Open a base form first.');
        const compareTarget = diffCompareSelect?.value;
        if (!compareTarget) return showBanner('Select a module to compare against.');
        try {
          const res = await fetch('/api/forms/diff', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              base: currentPayload.moduleIndex,
              modifiedModule: compareTarget
            })
          });
          if (!res.ok) throw new Error('Diff calculation failed');
          const diff = await res.json();
          renderDiffReport(diff);
        } catch (err) {
          showBanner("Diff calculation failed: " + err.message);
        }
      });
    }

    if (runReportBtn) {
      runReportBtn.addEventListener('click', executeReport);
    }

    if (printReportBtn) {
      printReportBtn.addEventListener('click', () => {
        if (reportPreviewFrame && reportPreviewFrame.contentWindow) {
          reportPreviewFrame.contentWindow.print();
        }
      });
    }

    if (reportSelect) {
      reportSelect.addEventListener('change', executeReport);
    }

    sampleSelect.addEventListener('change', () => {
      const selected = sampleSelect.value;
      if (selected) loadSample(selected);
    });

    canvasSelect.addEventListener('change', () => {
      const name = canvasSelect.value;
      const found = currentPayload?.moduleIndex?.canvases?.find(c => c.name === name);
      setActiveCanvas(found);
    });

    zoomSlider.addEventListener('input', (e) => {
      zoomLevel = parseInt(e.target.value) / 100;
      zoomValue.textContent = `${e.target.value}%`;
      applyZoom();
    });

    const zoomInBtn = document.getElementById('zoomInBtn');
    const zoomOutBtn = document.getElementById('zoomOutBtn');
    const zoomFitBtn = document.getElementById('zoomFitBtn');

    if (zoomInBtn) {
      zoomInBtn.addEventListener('click', () => {
        const next = Math.min(250, parseInt(zoomSlider.value) + 15);
        zoomSlider.value = next;
        zoomLevel = next / 100;
        zoomValue.textContent = `${next}%`;
        applyZoom();
      });
    }

    if (zoomOutBtn) {
      zoomOutBtn.addEventListener('click', () => {
        const next = Math.max(20, parseInt(zoomSlider.value) - 15);
        zoomSlider.value = next;
        zoomLevel = next / 100;
        zoomValue.textContent = `${next}%`;
        applyZoom();
      });
    }

    if (zoomFitBtn) {
      zoomFitBtn.addEventListener('click', zoomFit);
    }

    showNullCanvasToggle.addEventListener('change', (e) => {
      showNullCanvas = e.target.checked;
      renderCanvasSvg();
    });

    if (runtimeFrameToggle && oracleWindowShell) {
      runtimeFrameToggle.addEventListener('change', (e) => {
        if (e.target.checked) {
          oracleWindowShell.classList.remove('hidden-frame');
        } else {
          oracleWindowShell.classList.add('hidden-frame');
        }
        setTimeout(zoomFit, 50);
      });
    }

    bannerDismissBtn.addEventListener('click', () => {
      conversionBanner.classList.add('hidden');
    });

    bannerSampleBtn.addEventListener('click', () => {
      conversionBanner.classList.add('hidden');
      loadSample('orders_fmb.xml');
    });

    treeFilterInput.addEventListener('input', () => {
      filterTree(treeFilterInput.value.trim().toLowerCase());
    });
  }

  function applyZoom() {
    if (previewEnabled && formsSvg.viewBox.baseVal.width) {
      // Native SVG scaling keeps HTML controls' hit regions aligned with the canvas.
      svgContainer.style.transform = 'none';
      formsSvg.setAttribute('width', formsSvg.viewBox.baseVal.width * zoomLevel);
      formsSvg.setAttribute('height', formsSvg.viewBox.baseVal.height * zoomLevel);
      return;
    }
    svgContainer.style.transform = `scale(${zoomLevel})`;
  }

  function zoomFit() {
    if (!activeCanvas || !currentPayload) return;
    const viewport = document.getElementById('canvasViewport');
    const width = formsSvg.viewBox.baseVal.width || 800;
    const height = formsSvg.viewBox.baseVal.height || 600;
    const pad = 60;
    const frameWidth = previewEnabled ? Math.max(0, oracleWindowShell.offsetWidth - formsSvg.getBoundingClientRect().width) : 0;
    const frameHeight = previewEnabled ? Math.max(0, oracleWindowShell.offsetHeight - formsSvg.getBoundingClientRect().height) : 0;
    const availW = Math.max(200, (viewport?.clientWidth || 800) - pad - frameWidth);
    const availH = Math.max(200, (viewport?.clientHeight || 600) - pad - frameHeight);
    const scaleW = availW / width;
    const scaleH = availH / height;
    const fitScale = Math.min(scaleW, scaleH, 1.4);
    const pct = Math.max(20, Math.min(250, Math.round(fitScale * 100)));
    zoomSlider.value = pct;
    zoomLevel = pct / 100;
    zoomValue.textContent = `${pct}%`;
    applyZoom();
    if (previewEnabled) viewport.scrollTop = 0;
  }

  // Sample listing
  async function loadSampleList() {
    try {
      const res = await fetch('/api/samples');
      if (res.ok) {
        const samples = await res.json();
        sampleSelect.innerHTML = samples.map(s => `<option value="${s}">${s}</option>`).join('');
        if (diffCompareSelect) {
          diffCompareSelect.innerHTML = samples.map(s => `<option value="${s}">${s}</option>`).join('');
        }
        // Automatically load rccm0000.fmb on startup if no module is open yet
        const defaultSample = samples.find(s => s === 'rccm0000.fmb') || samples[0];
        if (defaultSample && !currentPayload) {
          sampleSelect.value = defaultSample;
          loadSample(defaultSample);
        }
      }
    } catch (e) {
      console.warn("Could not load sample list:", e);
    }
  }

  async function loadSample(sampleName) {
    try {
      hideBanner();
      const res = await fetch(`/api/open-sample/${encodeURIComponent(sampleName)}`, { method: 'POST' });
      if (!res.ok) {
        const err = await res.json();
        showBanner(err.error || "Failed to load sample");
        return;
      }
      const payload = await res.json();
      setModulePayload(payload);
    } catch (e) {
      showBanner("Network error while loading sample: " + e.message);
    }
  }

  async function uploadFile(file) {
    try {
      hideBanner();
      const formData = new FormData();
      formData.append('file', file);
      const res = await fetch('/api/open-file', {
        method: 'POST',
        body: formData
      });
      handleOpenResponse(res);
    } catch (e) {
      showBanner("File upload failed: " + e.message);
    }
  }

  async function openFilePath(path) {
    try {
      hideBanner();
      const res = await fetch('/api/open-file', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filePath: path })
      });
      handleOpenResponse(res);
    } catch (e) {
      showBanner("Failed to open path: " + e.message);
    }
  }

  async function handleOpenResponse(res) {
    if (res.status === 422) {
      const err = await res.json();
      showBanner(err.error, true);
      return;
    }
    if (!res.ok) {
      const err = await res.json();
      showBanner(err.error || "Failed to open file");
      return;
    }
    const payload = await res.json();
    setModulePayload(payload);
  }

  function showBanner(message, isConversion = false) {
    bannerMessage.textContent = message;
    conversionBanner.classList.remove('hidden');
    bannerSampleBtn.style.display = isConversion ? 'inline-block' : 'none';
  }

  function hideBanner() {
    conversionBanner.classList.add('hidden');
  }

  // Set active module
  function setModulePayload(payload) {
    currentPayload = payload;
    selectedObject = null;
    previewSource = null;
    queryRecordsByBlock.clear();
    activeRecordIndexByBlock.clear();
    if (recordNavControls) recordNavControls.classList.add('hidden');
    document.getElementById('previewDialog').close();
    menuSourceModule = payload.summary.moduleName;
    preview.load(payload.moduleIndex);
    const alerts = document.getElementById('previewAlertSelect');
    alerts.replaceChildren(new Option('Preview Alert...', ''));
    for (const alert of payload.moduleIndex.alerts || []) alerts.add(new Option(alert.name, alert.name));
    const summary = payload.summary;
    const mod = payload.moduleIndex;

    moduleBadge.textContent = `${summary.moduleName} (${summary.moduleType})`;
    codeTabBadge.textContent = summary.triggerCount + summary.programUnitCount;

    // Populate Canvas dropdown and select the canvas with the most items
    if (mod.canvases && mod.canvases.length > 0) {
      let bestCanvas = mod.canvases[0];
      let maxItems = -1;

      const canvasOptions = mod.canvases.map(c => {
        let itemCount = 0;
        if (mod.blocks) {
          for (const b of mod.blocks) {
            if (b.items) {
              itemCount += b.items.filter(it => it.canvasName === c.name).length;
            }

          }
        }
        if (itemCount > maxItems) {
          maxItems = itemCount;
          bestCanvas = c;
        }
        return `<option value="${c.name}">${c.name} (${c.canvasType || 'Content'} · ${itemCount} items)</option>`;
      });

      canvasSelect.innerHTML = canvasOptions.join('');
      canvasSelect.value = bestCanvas.name;
      setActiveCanvas(bestCanvas);
    } else {
      canvasSelect.innerHTML = '<option value="">(No Canvases)</option>';
      setActiveCanvas(null);
    }

    renderRuntimeMenuBar(mod);
    renderNavigatorTree();
    renderCanvasSvg();
    setTimeout(zoomFit, 60);
  }

  function showPreviewDialog(title, message, source = null) {
    previewSource = source;
    document.getElementById('previewDialogTitle').textContent = title;
    document.getElementById('previewDialogMessage').textContent = message;
    document.getElementById('previewSourceBtn').hidden = !source;
    document.getElementById('previewDialog').showModal();
  }

  function switchTabPage(tabName) {
    if (!activeCanvas || !activeCanvas.tabPages) return;
    activeTabPage = tabName;
    document.querySelectorAll('.tab-pill').forEach(p => {
      p.classList.toggle('active', p.dataset.tabName === tabName || p.textContent === tabName);
    });
    renderCanvasSvg();
  }
  window.switchSvgTabPage = switchTabPage;

  function setActiveCanvas(canvas) {
    activeCanvas = canvas;
    activeTabPage = null;
    tabPagePills.innerHTML = '';

    if (canvas && canvas.tabPages && canvas.tabPages.length > 0) {
      canvas.tabPages.forEach((tp, i) => {
        const pill = document.createElement('span');
        pill.className = `tab-pill ${i === 0 ? 'active' : ''}`;
        pill.dataset.tabName = tp.name;
        pill.textContent = unescapeXml(tp.label || tp.name);
        pill.onclick = () => switchTabPage(tp.name);
        tabPagePills.appendChild(pill);
      });
      activeTabPage = canvas.tabPages[0].name;
    }

    renderCanvasSvg();
  }

  // Object Navigator Tree Rendering
  function renderNavigatorTree() {
    if (!currentPayload) {
      navigatorTree.innerHTML = '<div class="empty-state">No form loaded.</div>';
      return;
    }

    const mod = currentPayload.moduleIndex;
    let html = '';

    // Form-Level Triggers
    const formTriggers = mod.triggers.filter(t => t.level === 'FORM');
    html += renderTreeGroup('Triggers (Form)', '⚡', formTriggers.map(t => ({
      name: t.name,
      type: 'TRIGGER',
      obj: t,
      sublabel: `${t.lineCount} lines`
    })));

    // Blocks
    const blockNodes = mod.blocks.map(b => {
      const items = b.items.map(it => ({
        name: it.name,
        type: 'ITEM',
        obj: it,
        block: b,
        sublabel: it.itemType || 'Item'
      }));

      const blockTriggers = mod.triggers.filter(t => t.level === 'BLOCK' && t.blockName === b.name).map(t => ({
        name: t.name,
        type: 'TRIGGER',
        obj: t,
        sublabel: `${t.lineCount} lines`
      }));

      const relations = b.relations.map(r => ({
        name: r.name,
        type: 'RELATION',
        obj: r,
        sublabel: `-> ${r.detailBlock}`
      }));

      return {
        name: b.name,
        type: 'BLOCK',
        obj: b,
        sublabel: b.queryDataSourceName ? `[${b.queryDataSourceName}]` : '[Control]',
        children: [
          { name: `Items (${items.length})`, type: 'GROUP', children: items },
          { name: `Triggers (${blockTriggers.length})`, type: 'GROUP', children: blockTriggers },
          { name: `Relations (${relations.length})`, type: 'GROUP', children: relations }
        ]
      };
    });
    html += renderTreeGroup('Blocks', '📦', blockNodes);

    // Canvases
    const canvasNodes = mod.canvases.map(c => {
      const graphics = mod.graphics.filter(g => g.canvasName === c.name).map(g => ({
        name: g.name,
        type: 'GRAPHIC',
        obj: g,
        sublabel: g.graphicsType
      }));
      const tabs = c.tabPages.map(tp => ({
        name: tp.name,
        type: 'TAB_PAGE',
        obj: tp,
        sublabel: tp.label || ''
      }));
      return {
        name: c.name,
        type: 'CANVAS',
        obj: c,
        sublabel: c.canvasType || '',
        children: [
          ...(tabs.length > 0 ? [{ name: 'Tab Pages', type: 'GROUP', children: tabs }] : []),
          ...(graphics.length > 0 ? [{ name: 'Graphics', type: 'GROUP', children: graphics }] : [])
        ]
      };
    });
    html += renderTreeGroup('Canvases', '🎨', canvasNodes);

    // Windows
    html += renderTreeGroup('Windows', '🪟', mod.windows.map(w => ({
      name: w.name,
      type: 'WINDOW',
      obj: w,
      sublabel: w.title || ''
    })));

    // Program Units
    html += renderTreeGroup('Program Units', '📘', mod.programUnits.map(pu => ({
      name: pu.name,
      type: 'PROGRAM_UNIT',
      obj: pu,
      sublabel: `${pu.unitType} (${pu.lineCount}L)`
    })));

    // LOVs
    html += renderTreeGroup('LOVs & Record Groups', '🔍', [
      ...mod.lovs.map(l => ({ name: l.name, type: 'LOV', obj: l, sublabel: `LOV -> ${l.recordGroupName || ''}` })),
      ...mod.recordGroups.map(rg => ({ name: rg.name, type: 'RECORD_GROUP', obj: rg, sublabel: 'Record Group' }))
    ]);

    // Alerts & Parameters
    html += renderTreeGroup('Alerts & Parameters', '🔔', [
      ...mod.alerts.map(a => ({ name: a.name, type: 'ALERT', obj: a, sublabel: 'Alert' })),
      ...mod.parameters.map(p => ({ name: p.name, type: 'PARAMETER', obj: p, sublabel: `Param (${p.dataType || ''})` }))
    ]);

    // Property Classes
    if (mod.propertyClasses && mod.propertyClasses.length > 0) {
      html += renderTreeGroup('Property Classes', '🏷️', mod.propertyClasses.map(pc => ({
        name: pc,
        type: 'PROPERTY_CLASS',
        obj: mod.propertyClassDetails.find(p => p.name === pc) || { name: pc }
      })));
    }

    // Menus
    if (mod.menus && mod.menus.length > 0) {
      const menuNodes = mod.menus.map(m => ({
        name: m.name,
        type: 'MENU',
        obj: m,
        sublabel: `${m.items ? m.items.length : 0} items`,
        children: (m.items || []).map(it => ({
          name: it.name,
          type: 'MENU_ITEM',
          obj: it,
          menu: m,
          sublabel: it.label || it.commandType || ''
        }))
      }));
      html += renderTreeGroup('Menus', '📋', menuNodes);
    }

    navigatorTree.innerHTML = html;
    bindTreeEvents();
  }

  function renderTreeGroup(title, icon, items) {
    if (!items || items.length === 0) return '';
    const itemsHtml = items.map(item => renderTreeNode(item)).join('');
    return `
      <div class="tree-node tree-group-node">
        <div class="tree-row tree-group-header">
          <span class="tree-toggle">▼</span>
          <span class="tree-icon">${icon}</span>
          <span class="tree-label"><strong>${title}</strong></span>
          <span class="tree-badge">${items.length}</span>
        </div>
        <div class="tree-children">${itemsHtml}</div>
      </div>
    `;
  }

  function renderTreeNode(node) {
    const hasChildren = node.children && node.children.length > 0;
    const toggle = hasChildren ? '<span class="tree-toggle">▼</span>' : '<span class="tree-toggle"></span>';
    const sub = node.sublabel ? `<span class="tree-badge">${escapeHtml(node.sublabel)}</span>` : '';
    const nodeData = encodeURIComponent(JSON.stringify({ type: node.type, name: node.name }));

    let childrenHtml = '';
    if (hasChildren) {
      childrenHtml = `<div class="tree-children">${node.children.map(renderTreeNode).join('')}</div>`;
    }

    return `
      <div class="tree-node" data-type="${node.type}" data-name="${escapeHtml(node.name)}">
        <div class="tree-row" data-meta="${nodeData}">
          ${toggle}
          <span class="tree-label">${escapeHtml(node.name)}</span>
          ${sub}
        </div>
        ${childrenHtml}
      </div>
    `;
  }

  function bindTreeEvents() {
    navigatorTree.querySelectorAll('.tree-row').forEach(row => {
      row.addEventListener('click', (e) => {
        e.stopPropagation();
        const toggle = e.target.classList.contains('tree-toggle');
        const children = row.parentElement.querySelector('.tree-children');

        if (toggle && children) {
          children.classList.toggle('collapsed');
          row.querySelector('.tree-toggle').textContent = children.classList.contains('collapsed') ? '▶' : '▼';
          return;
        }

        // Selection
        navigatorTree.querySelectorAll('.tree-row').forEach(r => r.classList.remove('selected'));
        row.classList.add('selected');

        const metaStr = row.getAttribute('data-meta');
        if (metaStr) {
          const meta = JSON.parse(decodeURIComponent(metaStr));
          handleTreeSelection(meta.type, meta.name);
        }
      });
    });
  }

  function filterTree(query) {
    navigatorTree.querySelectorAll('.tree-node').forEach(node => {
      if (!query) {
        node.style.display = '';
        return;
      }
      const text = node.textContent.toLowerCase();
      node.style.display = text.includes(query) ? '' : 'none';
    });
  }

  // Handle tree node selection
  function handleTreeSelection(type, name) {
    if (!currentPayload) return;
    const mod = currentPayload.moduleIndex;
    let targetObj = null;

    if (type === 'ITEM') {
      for (const b of mod.blocks) {
        const found = b.items.find(i => i.name === name);
        if (found) {
          targetObj = { type: 'ITEM', item: found, block: b };
          // If item belongs to a canvas, switch canvas if needed
          if (found.canvasName && (!activeCanvas || activeCanvas.name !== found.canvasName)) {
            const c = mod.canvases.find(c => c.name === found.canvasName);
            if (c) {
              canvasSelect.value = c.name;
              setActiveCanvas(c);
            }
          }
          break;
        }
      }
    } else if (type === 'BLOCK') {
      const b = mod.blocks.find(b => b.name === name);
      if (b) {
        targetObj = { type: 'BLOCK', block: b };
        const itemCanvas = b.items.find(it => it.canvasName)?.canvasName;
        if (itemCanvas && (!activeCanvas || activeCanvas.name !== itemCanvas)) {
          const c = mod.canvases.find(c => c.name === itemCanvas);
          if (c) {
            canvasSelect.value = c.name;
            setActiveCanvas(c);
          }
        }
      }
    } else if (type === 'TRIGGER') {
      const t = mod.triggers.find(t => t.name === name);
      if (t) targetObj = { type: 'TRIGGER', trigger: t };
    } else if (type === 'PROGRAM_UNIT') {
      const pu = mod.programUnits.find(p => p.name === name);
      if (pu) targetObj = { type: 'PROGRAM_UNIT', programUnit: pu };
    } else if (type === 'CANVAS') {
      const c = mod.canvases.find(c => c.name === name);
      if (c) {
        targetObj = { type: 'CANVAS', canvas: c };
        canvasSelect.value = c.name;
        setActiveCanvas(c);
      }
    } else if (type === 'WINDOW') {
      const w = mod.windows.find(w => w.name === name);
      if (w) targetObj = { type: 'WINDOW', window: w };
    } else if (type === 'LOV') {
      const l = mod.lovs.find(l => l.name === name);
      if (l) targetObj = { type: 'LOV', lov: l };
    } else if (type === 'RECORD_GROUP') {
      const rg = mod.recordGroups.find(r => r.name === name);
      if (rg) targetObj = { type: 'RECORD_GROUP', recordGroup: rg };
    } else if (type === 'GRAPHIC') {
      const g = mod.graphics.find(g => g.name === name);
      if (g) targetObj = { type: 'GRAPHIC', graphic: g };
    } else if (type === 'PROPERTY_CLASS') {
      const pc = mod.propertyClassDetails.find(p => p.name === name) || { name: name };
      targetObj = { type: 'PROPERTY_CLASS', propertyClass: pc };
    } else if (type === 'MENU_ITEM') {
      if (mod.menus) {
        for (const m of mod.menus) {
          const found = (m.items || []).find(i => i.name === name);
          if (found) {
            targetObj = { type: 'MENU_ITEM', item: found, menu: m };
            break;
          }
        }
      }
    } else if (type === 'MENU') {
      const m = mod.menus ? mod.menus.find(m => m.name === name) : null;
      if (m) targetObj = { type: 'MENU', menu: m };
    }

    selectedObject = targetObj;
    renderPropertyInspector(targetObj);
    highlightCanvasItem(type === 'ITEM' ? name : null);

    // If selected object has code, display code
    if (type === 'TRIGGER' && targetObj.trigger.textRef) {
      loadCode(targetObj.trigger.name, targetObj.trigger.textRef.file, 'TRIGGER');
    } else if (type === 'PROGRAM_UNIT' && targetObj.programUnit.textRef) {
      loadCode(targetObj.programUnit.name, targetObj.programUnit.textRef.file, 'PROGRAM_UNIT');
    } else if (type === 'MENU_ITEM' && targetObj && targetObj.item && targetObj.item.commandRef) {
      loadCode(`Menu: ${targetObj.menu.name}.${targetObj.item.name}`, targetObj.item.commandRef.file, 'MENU_ITEM', menuSourceModule);
    } else if (type === 'ITEM') {
      // If item has trigger, pick first
      const itemTriggers = mod.triggers.filter(t => t.level === 'ITEM' && t.itemName === name);
      if (itemTriggers.length > 0 && itemTriggers[0].textRef) {
        loadCode(itemTriggers[0].name, itemTriggers[0].textRef.file, 'TRIGGER');
      }
    }

    // Load XML slice if objectRef exists
    const objRef = mod.objectRefs.find(r => r.name === name);
    if (objRef && objRef.ref) {
      loadXmlSlice(name, objRef.ref.startLine, objRef.ref.endLine);
    }
  }

  // Database Query Engine & Record Navigation
  function getActiveBlockName() {
    if (!currentPayload) return null;
    if (selectedObject && selectedObject.type === 'ITEM' && selectedObject.block) {
      return selectedObject.block.name;
    }
    const mod = currentPayload.moduleIndex;
    if (activeCanvas && mod.blocks) {
      for (const b of mod.blocks) {
        if (b.items && b.items.some(it => it.canvasName === activeCanvas.name)) {
          return b.name;
        }
      }
    }
    if (mod.blocks && mod.blocks.length > 0) {
      const qb = mod.blocks.find(b => b.queryDataSourceName || b.items.length > 0);
      return qb ? qb.name : mod.blocks[0].name;
    }
    return null;
  }

  function applyRecordsToBlock(blockName) {
    const records = queryRecordsByBlock.get(blockName) || [];
    const activeIdx = activeRecordIndexByBlock.get(blockName) || 0;
    const mod = currentPayload?.moduleIndex;
    const block = mod?.blocks.find(b => b.name === blockName);
    const recsToDisplay = Math.max(1, block?.recordsDisplayed || 1);

    if (records.length === 0) return;

    for (let r = 0; r < recsToDisplay; r++) {
      const recIndex = activeIdx + r;
      if (recIndex < records.length) {
        preview.setRecord(blockName, records[recIndex], r);
      }
    }
  }

  function updateRecordNavigator(blockName) {
    const records = queryRecordsByBlock.get(blockName) || [];
    const curIdx = activeRecordIndexByBlock.get(blockName) || 0;
    const total = records.length;

    if (total > 0) {
      recordNavControls?.classList.remove('hidden');
      if (recordNavBadge) recordNavBadge.textContent = `${curIdx + 1} / ${total}`;
      if (oracleStatusMsg) {
        oracleStatusMsg.textContent = `Record: ${curIdx + 1}/${total}  Block: ${blockName}`;
      }
      if (prevRecordBtn) prevRecordBtn.disabled = (curIdx === 0);
      if (nextRecordBtn) nextRecordBtn.disabled = (curIdx >= total - 1);
      if (runtimePrevRecordBtn) runtimePrevRecordBtn.disabled = (curIdx === 0);
      if (runtimeNextRecordBtn) runtimeNextRecordBtn.disabled = (curIdx >= total - 1);
    } else {
      recordNavControls?.classList.add('hidden');
    }
  }

  async function executeBlockQuery(targetBlockName = null) {
    if (!currentPayload) {
      showBanner('Open a form before querying the database.');
      return;
    }

    const blockName = targetBlockName || getActiveBlockName();
    if (!blockName) {
      showBanner('No queryable block found in this form module.');
      return;
    }

    const backend = dbBackendSelect ? dbBackendSelect.value : 'oracle';
    if (oracleStatusMsg) {
      oracleStatusMsg.textContent = `Querying block ${blockName} via ${backend}...`;
    }
    if (dbQueryBtn) {
      dbQueryBtn.disabled = true;
      dbQueryBtn.textContent = '⏳ Querying...';
    }

    try {
      const moduleName = currentPayload.summary.moduleName || currentPayload.moduleIndex.key.name;
      const res = await fetch('/api/forms/query-block', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          moduleName: moduleName,
          blockName: blockName,
          backend: backend,
          limit: 100,
        }),
      });

      const data = await res.json();
      if (!res.ok) {
        throw new Error(data.error || 'Failed to execute database query.');
      }

      const records = (data.records || []).map(r => (r && typeof r === 'object' && r.values) ? r.values : r);
      queryRecordsByBlock.set(blockName, records);
      activeRecordIndexByBlock.set(blockName, 0);

      applyRecordsToBlock(blockName);
      updateRecordNavigator(blockName);

      const count = data.totalCount ?? records.length;
      const sqlText = data.sqlStatement || data.sqlExecuted || '';
      const sqlSnippet = sqlText ? ` - SQL: ${sqlText.substring(0, 45)}...` : '';
      if (oracleStatusMsg) {
        oracleStatusMsg.textContent = `Fetched ${count} records on ${blockName} (${backend})${sqlSnippet}`;
      }

      renderCanvasSvg();
    } catch (err) {
      console.error('Query error:', err);
      showBanner(`Query failed: ${err.message}`);
      if (oracleStatusMsg) {
        oracleStatusMsg.textContent = `Query failed: ${err.message}`;
      }
    } finally {
      if (dbQueryBtn) {
        dbQueryBtn.disabled = false;
        dbQueryBtn.textContent = '⚡ Query DB';
      }
    }
  }

  function navigateRecord(direction) {
    const blockName = getActiveBlockName();
    if (!blockName) return;
    const records = queryRecordsByBlock.get(blockName) || [];
    if (records.length === 0) return;

    let curIdx = activeRecordIndexByBlock.get(blockName) || 0;
    if (direction === 'next' && curIdx < records.length - 1) {
      curIdx++;
    } else if (direction === 'prev' && curIdx > 0) {
      curIdx--;
    } else {
      return;
    }

    activeRecordIndexByBlock.set(blockName, curIdx);
    applyRecordsToBlock(blockName);
    updateRecordNavigator(blockName);
    renderCanvasSvg();
  }

  // Visual Canvas 2D SVG Rendering
  function renderCanvasSvg() {
    if (!currentPayload || !activeCanvas) {
      formsSvg.innerHTML = '<text x="20" y="30" fill="#666">No canvas selected</text>';
      canvasStats.textContent = 'Dimensions: -';
      return;
    }

    const mod = currentPayload.moduleIndex;
    const canvas = activeCanvas;
    const coord = mod.coordinate || {};

    // Coordinate scaling: translate from Oracle Forms units to standard CSS pixels
    // Oracle Forms "Real" coordinate system with "Centimeter" or "Inch" uses milli-units (1/1000)
    let coordScale = 1.0;
    const unit = (coord.realUnit || '').toLowerCase();
    const coordSys = (coord.coordinateSystem || '').toLowerCase();

    if (coordSys === 'real' || (canvas.width > 2000 || canvas.height > 2000)) {
      if (unit.includes('centimeter') || unit === 'cm') {
        coordScale = 37.79527559 / 1000.0; // 1cm = ~37.8px at 96 DPI
      } else if (unit.includes('inch') || unit === 'in') {
        coordScale = 96.0 / 1000.0;         // 1in = 96px at 96 DPI
      } else if (canvas.width > 3000 || canvas.height > 3000) {
        coordScale = 800.0 / Math.max(canvas.width, 1000);
      }
    }

    const scale = (val) => Math.round((val || 0) * coordScale);

    // Collect visual items on this canvas
    const visualItems = [];
    for (const block of mod.blocks) {
      for (const item of block.items) {
        const onThisCanvas = item.canvasName === canvas.name;
        const matchesTab = !canvas.tabPages.length || !item.tabPageName || item.tabPageName === activeTabPage;

        if (onThisCanvas && matchesTab) {
          visualItems.push({ block, item, isFallbackLayout: false });
        } else if (showNullCanvas && !item.canvasName) {
          visualItems.push({ block, item, isFallbackLayout: true });
        }
      }
    }

    // Auto-layout fallback for unpositioned items (x == 0 && y == 0)
    const zeroCoords = visualItems.filter(v => (v.item.xPosition || 0) === 0 && (v.item.yPosition || 0) === 0);
    const needsAutoFlow = zeroCoords.length > 1;

    const resolvedPositions = new Map();
    let currentBlockName = "";
    let currentBlockY = 40;
    let currentItemY = 40;
    const currentBlockX = 140;

    if (needsAutoFlow) {
      for (const { block, item } of visualItems) {
        let rawX = item.xPosition || 0;
        let rawY = item.yPosition || 0;
        let w = item.width || (item.itemType?.includes('Button') ? 120 : 130);
        let h = item.height || 22;

        if (rawX === 0 && rawY === 0) {
          if (block.name !== currentBlockName) {
            if (currentBlockName !== "") {
              currentBlockY = currentItemY + 35;
            }
            currentBlockName = block.name;
            currentItemY = currentBlockY + 20;
          }
          resolvedPositions.set(`${block.name}.${item.name}`, { x: currentBlockX, y: currentItemY, w, h });
          const recs = Math.max(1, block.recordsDisplayed || 1);
          const dist = block.distanceBetweenRecords || 4;
          const totalH = (recs > 1 && item.itemsDisplay !== 1) ? (recs * (h + dist)) : h;
          currentItemY += totalH + 14;
        }
      }
    }

    // Scaled canvas dimensions & dynamic bounding box expansion
    const baseWidth = Math.max(200, scale(canvas.width) || 800);
    const baseHeight = Math.max(150, scale(canvas.height) || 600);

    // Layer 1: Graphics
    const canvasGraphics = mod.graphics.filter(g => {
      if (g.canvasName !== canvas.name) return false;
      if (canvas.tabPages.length > 0 && activeTabPage && g.tabPageName && g.tabPageName !== activeTabPage) return false;
      return true;
    });

    let maxContentX = baseWidth;
    let maxContentY = baseHeight;

    for (const v of visualItems) {
      const pos = resolvedPositions.get(`${v.block.name}.${v.item.name}`);
      const ix = pos ? pos.x : scale(v.item.xPosition || 0);
      const iy = pos ? pos.y : scale(v.item.yPosition || 0);
      const iw = pos ? pos.w : scale(v.item.width || 120);
      const ih = pos ? pos.h : scale(v.item.height || 22);
      if (ix + iw > maxContentX) maxContentX = ix + iw;
      if (iy + ih > maxContentY) maxContentY = iy + ih;
    }

    for (const g of canvasGraphics) {
      const gx = scale(g.xPosition || 0);
      const gy = scale(g.yPosition || 0);
      const gw = scale(g.width || 0);
      const gh = scale(g.height || 0);
      if (gx + gw > maxContentX) maxContentX = gx + gw;
      if (gy + gh > maxContentY) maxContentY = gy + gh;
    }

    const width = Math.max(baseWidth, maxContentX + 30);
    const height = Math.max(baseHeight, maxContentY + 30, needsAutoFlow ? currentItemY + 60 : 0);

    formsSvg.setAttribute('width', width);
    formsSvg.setAttribute('height', height);
    formsSvg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    canvasStats.textContent = `Canvas: ${canvas.name} | Dimensions: ${width} x ${height} px${coordScale !== 1.0 ? ` (Scaled from ${canvas.width}x${canvas.height})` : ''} | Window: ${canvas.windowName || 'Default'}`;

    if (oracleWindowTitle) {
      const winName = canvas.windowName || mod.title || mod.key.name;
      oracleWindowTitle.textContent = `${winName} - [${canvas.name}]`;
    }
    if (oracleStatusMsg && !selectedObject) {
      oracleStatusMsg.textContent = `Record: 1/1  Canvas: ${canvas.name}`;
    }

    let svgHtml = '';

    // Canvas background
    const bgCol = canvas.backColor || '#d4d0c8';
    svgHtml += `<rect x="0" y="0" width="${width}" height="${height}" fill="${bgCol}"/>`;

    // Layer 0.5: Tab Canvas Notebook Body & Interactive Tab Headers
    if (canvas.tabPages && canvas.tabPages.length > 0) {
      const tabStartX = (canvas.xPosition && canvas.xPosition > 0) ? scale(canvas.xPosition) : 15;
      const tabStartY = (canvas.yPosition && canvas.yPosition > 0) ? scale(canvas.yPosition) : 15;
      const tabWidth = scale(canvas.width) || (width - tabStartX * 2);
      const tabHeight = scale(canvas.height) || (height - tabStartY - 30);
      const tabBodyY = tabStartY + 26;
      const tabBodyH = Math.max(60, tabHeight - 26);

      svgHtml += `
        <g class="svg-tab-notebook">
          <!-- Notebook Body Container Frame -->
          <rect x="${tabStartX}" y="${tabBodyY}" width="${tabWidth}" height="${tabBodyH}" fill="${bgCol}" stroke="#808080" stroke-width="1.5" rx="3"/>
          <rect x="${tabStartX + 1}" y="${tabBodyY + 1}" width="${tabWidth - 2}" height="${tabBodyH - 2}" fill="none" stroke="#ffffff" stroke-width="1" rx="2"/>
      `;

      let curTabX = tabStartX + 4;
      canvas.tabPages.forEach((tp) => {
        const isActive = tp.name === activeTabPage;
        const tabLabel = tp.label || tp.name;
        const tabW = Math.max(90, Math.round(tabLabel.length * 7.5 + 24));

        if (isActive) {
          svgHtml += `
            <g class="svg-tab-header active" style="cursor: pointer;" onclick="window.switchSvgTabPage('${escapeHtml(tp.name)}')">
              <path d="M ${curTabX} ${tabBodyY + 1} L ${curTabX} ${tabStartY + 3} Q ${curTabX} ${tabStartY - 2} ${curTabX + 4} ${tabStartY - 2} L ${curTabX + tabW - 4} ${tabStartY - 2} Q ${curTabX + tabW} ${tabStartY - 2} ${curTabX + tabW} ${tabStartY + 3} L ${curTabX + tabW} ${tabBodyY + 1} Z" fill="#f5f4ef" stroke="#808080" stroke-width="1.5"/>
              <path d="M ${curTabX + 1.5} ${tabBodyY} L ${curTabX + 1.5} ${tabStartY + 4} Q ${curTabX + 1.5} ${tabStartY - 0.5} ${curTabX + 4} ${tabStartY - 0.5} L ${curTabX + tabW - 3} ${tabStartY - 0.5}" fill="none" stroke="#ffffff" stroke-width="1"/>
              <!-- Open bottom line to seamlessly merge into active notebook body -->
              <line x1="${curTabX + 1}" y1="${tabBodyY}" x2="${curTabX + tabW - 1}" y2="${tabBodyY}" stroke="#f5f4ef" stroke-width="2.5"/>
              <text x="${curTabX + tabW / 2}" y="${tabStartY + 16}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="#000">${escapeHtml(tabLabel)}</text>
            </g>
          `;
        } else {
          svgHtml += `
            <g class="svg-tab-header inactive" style="cursor: pointer;" onclick="window.switchSvgTabPage('${escapeHtml(tp.name)}')">
              <path d="M ${curTabX} ${tabBodyY} L ${curTabX} ${tabStartY + 6} Q ${curTabX} ${tabStartY + 3} ${curTabX + 3} ${tabStartY + 3} L ${curTabX + tabW - 3} ${tabStartY + 3} Q ${curTabX + tabW} ${tabStartY + 3} ${curTabX + tabW} ${tabStartY + 6} L ${curTabX + tabW} ${tabBodyY} Z" fill="#d0ccc3" stroke="#808080" stroke-width="1"/>
              <text x="${curTabX + tabW / 2}" y="${tabStartY + 18}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#444">${escapeHtml(tabLabel)}</text>
            </g>
          `;
        }
        curTabX += tabW + 3;
      });

      svgHtml += `</g>`;
    }

    for (const g of canvasGraphics) {
      // If auto-flow is managing unpositioned layout, ignore unpositioned dummy frames
      if (needsAutoFlow && g.graphicsType === 'Frame' && !g.xPosition && !g.yPosition && !g.width && !g.height) {
        continue;
      }
      const gx = scale(g.xPosition) || 10;
      const gy = scale(g.yPosition) || 10;
      const gw = scale(g.width) || (width - 40);
      const gh = scale(g.height) || 100;

      if (g.graphicsType === 'Frame') {
        const title = g.frameTitle || g.name || '';
        svgHtml += `
          <g class="svg-graphic frame">
            <rect x="${gx}" y="${gy}" width="${gw}" height="${gh}" fill="none" stroke="#808080" stroke-width="1.5" rx="2"/>
            <rect x="${gx + 1}" y="${gy + 1}" width="${gw - 2}" height="${gh - 2}" fill="none" stroke="#ffffff" stroke-width="1" rx="2"/>
            ${title ? `
              <rect x="${gx + 10}" y="${gy - 7}" width="${title.length * 7 + 10}" height="14" fill="${bgCol}"/>
              <text x="${gx + 15}" y="${gy + 4}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="#000">${escapeHtml(title)}</text>
            ` : ''}
          </g>
        `;
      } else if (g.graphicsType === 'Text') {
        svgHtml += `<text x="${gx}" y="${gy + 12}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#000">${escapeHtml(g.graphicsText || g.name)}</text>`;
      } else {
        svgHtml += `<rect x="${gx}" y="${gy}" width="${gw}" height="${gh}" fill="${g.backColor || 'none'}" stroke="#808080" stroke-width="1"/>`;
      }
    }

    // Layer 2: Synthesized Frames for Auto-Flow Blocks
    if (needsAutoFlow) {
      const drawnBlocks = new Set();
      visualItems.forEach(({ block, item }) => {
        const pos = resolvedPositions.get(`${block.name}.${item.name}`);
        if (pos && !drawnBlocks.has(block.name)) {
          drawnBlocks.add(block.name);
          const blockItems = visualItems.filter(v => v.block.name === block.name && resolvedPositions.has(`${block.name}.${v.item.name}`));
          if (blockItems.length > 0) {
            const firstPos = resolvedPositions.get(`${block.name}.${blockItems[0].item.name}`);
            const lastPos = resolvedPositions.get(`${block.name}.${blockItems[blockItems.length - 1].item.name}`);
            const recs = Math.max(1, block.recordsDisplayed || 1);
            const dist = block.distanceBetweenRecords || 4;
            const lastH = (recs > 1 && blockItems[blockItems.length - 1].item.itemsDisplay !== 1)
              ? (recs * (lastPos.h + dist)) : lastPos.h;
            const fY = firstPos.y - 20;
            const fH = (lastPos.y + lastH + 16) - fY;
            const fW = 480;

            svgHtml += `
              <g class="svg-graphic frame">
                <rect x="20" y="${fY}" width="${fW}" height="${fH}" fill="none" stroke="#808080" stroke-width="1.5" rx="3"/>
                <rect x="21" y="${fY + 1}" width="${fW - 2}" height="${fH - 2}" fill="none" stroke="#ffffff" stroke-width="1" rx="3"/>
                <rect x="35" y="${fY - 7}" width="${block.name.length * 8 + 60}" height="14" fill="${bgCol}"/>
                <text x="40" y="${fY + 4}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="#000">Block: ${escapeHtml(block.name)}</text>
              </g>
            `;
          }
        }
      });
    }

    for (const { block, item, isFallbackLayout } of visualItems) {
      if (previewEnabled && (item.visible === false || item.canvasVisible === false || isFallbackLayout)) continue;
      // Determine layout position
      const autoPos = resolvedPositions.get(`${block.name}.${item.name}`);
      let x = autoPos ? autoPos.x : scale(item.xPosition);
      let y = autoPos ? autoPos.y : scale(item.yPosition);
      let rawW = scale(item.width);
      let rawH = scale(item.height);

      const recordsToDisplay = Math.max(1, block.recordsDisplayed || 1);
      const isMultiRow = recordsToDisplay > 1 && item.itemsDisplay !== 1;
      const dist = scale(block.distanceBetweenRecords || item.distanceBetweenRecords || 4);

      const itemType = (item.itemType || '').toLowerCase();
      const nameUpper = (item.name || '').toUpperCase();
      const propUpper = (item.propertyClass || '').toUpperCase();
      const isButton = itemType.includes('button') || nameUpper.startsWith('BTN') || nameUpper.includes('BUTTON') || propUpper.includes('BUTTON') || (item.label && !item.prompt);
      const isCheckOrRadio = itemType.includes('check') || itemType.includes('radio') || nameUpper.startsWith('CHK') || nameUpper.startsWith('RAD');
      const isHidden = item.visible === false;
      const isHeaderLabel = item.name === 'MIN_NAME' || (!item.columnName && !item.prompt && (item.bevel === 'None' || (!item.itemType && item.label)));

      let w = autoPos ? autoPos.w : (rawW || (isButton ? 140 : 100));
      let h = autoPos ? autoPos.h : (rawH || (isButton ? 33 : 20));
      if (isButton && h < 26) h = 33;

      // Draw Multi-Record Grid rows
      const rowCount = isMultiRow ? recordsToDisplay : 1;

      for (let r = 0; r < rowCount; r++) {
        const rowY = y + r * (h + dist);
        const isFirstRow = (r === 0);

        // Prompt Rendering (only for first row, and NOT for buttons/checkboxes/headers!)
        if (isFirstRow && item.prompt && !isButton && !isCheckOrRadio && !isHeaderLabel) {
          const edge = (item.promptAttachmentEdge || 'Start').toLowerCase();
          const offset = item.promptAttachmentOffset || 5;

          if (edge === 'top') {
            svgHtml += `<text x="${x}" y="${rowY - offset}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#222" font-weight="600">${escapeHtml(item.prompt)}</text>`;
          } else {
            // Start / left
            svgHtml += `<text x="${x - offset}" y="${rowY + Math.min(14, h - 4)}" text-anchor="end" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#222">${escapeHtml(item.prompt)}</text>`;
          }
        }

        // Widget Box by itemType
        const fullItemName = `${block.name}.${item.name}`;
        const isSelected = selectedObject && selectedObject.type === 'ITEM' && selectedObject.item.name === item.name;

        svgHtml += `
          <g class="svg-interactive-item ${isSelected ? 'svg-item-selected' : ''}" 
             ${isHidden ? 'style="opacity: 0.72;"' : ''}
             data-block="${escapeHtml(block.name)}" 
             data-item="${escapeHtml(item.name)}" 
             ${layoutEditMode ? `onmousedown="window.startItemDrag(event, '${escapeHtml(block.name)}', '${escapeHtml(item.name)}')"` : ''}
             ${previewEnabled ? '' : `onclick="window.selectSvgItem('${escapeHtml(block.name)}', '${escapeHtml(item.name)}')"`}>
            <title>${escapeHtml(fullItemName)} | Type: ${escapeHtml(isButton ? 'Push Button' : (item.itemType || 'Text Item'))}${isHidden ? ' [Conditionally Hidden]' : ''} | Column: ${escapeHtml(item.columnName || 'N/A')}</title>
        `;

        if (previewEnabled && !isHeaderLabel) {
          svgHtml += `<foreignObject x="${x}" y="${rowY}" width="${w}" height="${Math.max(h, 20)}"
            data-preview-block="${escapeHtml(block.name)}" data-preview-item="${escapeHtml(item.name)}" data-preview-row="${r}"></foreignObject>`;
        } else if (isHeaderLabel) {
          // Clean borderless display header (e.g. MIN_NAME)
          svgHtml += `
            <text x="${x + w / 2}" y="${rowY + Math.min(15, h - 2)}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="12" font-weight="bold" fill="#111">${escapeHtml(item.label || item.prompt || item.name)}</text>
          `;
        } else if (isButton) {
          // Push button
          const btnLabel = item.label || item.prompt || item.name;
          svgHtml += `
            <rect x="${x}" y="${rowY}" width="${w}" height="${h}" rx="3" fill="#ece9d8" stroke="#716f64" stroke-width="1" ${isHidden ? 'stroke-dasharray="4,2"' : ''}/>
            <line x1="${x + 1}" y1="${rowY + 1}" x2="${x + w - 1}" y2="${rowY + 1}" stroke="#ffffff" stroke-width="1.5"/>
            <line x1="${x + 1}" y1="${rowY + 1}" x2="${x + 1}" y2="${rowY + h - 1}" stroke="#ffffff" stroke-width="1.5"/>
            <line x1="${x + 1}" y1="${rowY + h - 1}" x2="${x + w - 1}" y2="${rowY + h - 1}" stroke="#716f64" stroke-width="1.5"/>
            <line x1="${x + w - 1}" y1="${rowY + 1}" x2="${x + w - 1}" y2="${rowY + h - 1}" stroke="#716f64" stroke-width="1.5"/>
            <text x="${x + w / 2}" y="${rowY + h / 2 + 4}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="600" fill="#111">${escapeHtml(btnLabel)}</text>
          `;
        } else if (itemType.includes('check')) {
          // Checkbox
          svgHtml += `
            <rect x="${x}" y="${rowY}" width="13" height="13" fill="#ffffff" stroke="#7f9db9" stroke-width="1"/>
            <text x="${x + 2}" y="${rowY + 10}" font-size="10" fill="#0055ea" font-weight="bold">✓</text>
            <text x="${x + 18}" y="${rowY + 10}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#000">${escapeHtml(item.label || item.prompt || item.name)}</text>
          `;
        } else if (itemType.includes('radio')) {
          // Radio
          svgHtml += `
            <circle cx="${x + 6}" cy="${rowY + 6}" r="6" fill="#ffffff" stroke="#7f9db9" stroke-width="1"/>
            <circle cx="${x + 6}" cy="${rowY + 6}" r="3" fill="#000000"/>
            <text x="${x + 18}" y="${rowY + 10}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" fill="#000">${escapeHtml(item.label || item.prompt || item.name)}</text>
          `;
        } else if (itemType.includes('display')) {
          // Display Item (flat gray)
          const bRecords = queryRecordsByBlock.get(block.name);
          const activeIdx = activeRecordIndexByBlock.get(block.name) || 0;
          let displayVal = null;
          if (bRecords && bRecords.length > 0) {
            const recIdx = isMultiRow ? (activeIdx + r) : activeIdx;
            if (recIdx < bRecords.length) {
              const rec = bRecords[recIdx];
              displayVal = rec[item.name] ?? (item.columnName ? rec[item.columnName] : null);
              if (displayVal == null) {
                const iUpper = item.name.toUpperCase();
                const cUpper = (item.columnName || '').toUpperCase();
                for (const [k, v] of Object.entries(rec)) {
                  const kUpper = k.toUpperCase();
                  if (kUpper === iUpper || (cUpper && kUpper === cUpper)) {
                    displayVal = v;
                    break;
                  }
                }
              }
            }
          }
          const textToShow = displayVal != null ? String(displayVal) : item.name;
          const textColor = displayVal != null ? '#111111' : '#555555';
          const textWeight = displayVal != null ? 'font-weight="600"' : '';

          svgHtml += `
            <rect x="${x}" y="${rowY}" width="${w}" height="${h}" fill="#f0ede0" stroke="#b0ac9d" stroke-width="1"/>
            <text x="${x + 4}" y="${rowY + Math.min(14, h - 4)}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" ${textWeight} fill="${textColor}">${escapeHtml(textToShow)}</text>
          `;
        } else {
          // Text Item / Non-enterable item
          const isNonEnterable = (item.propertyClass || '').includes('NONENTERABLE') || item.enabled === false;
          const isList = itemType.includes('list') || item.lovName;
          const bgBox = isNonEnterable ? '#f5f4ee' : '#ffffff';
          const strokeBox = isNonEnterable ? '#a6a397' : '#7f9db9';

          const bRecords = queryRecordsByBlock.get(block.name);
          const activeIdx = activeRecordIndexByBlock.get(block.name) || 0;
          let displayVal = null;
          if (bRecords && bRecords.length > 0) {
            const recIdx = isMultiRow ? (activeIdx + r) : activeIdx;
            if (recIdx < bRecords.length) {
              const rec = bRecords[recIdx];
              displayVal = rec[item.name] ?? (item.columnName ? rec[item.columnName] : null);
              if (displayVal == null) {
                const iUpper = item.name.toUpperCase();
                const cUpper = (item.columnName || '').toUpperCase();
                for (const [k, v] of Object.entries(rec)) {
                  const kUpper = k.toUpperCase();
                  if (kUpper === iUpper || (cUpper && kUpper === cUpper)) {
                    displayVal = v;
                    break;
                  }
                }
              }
            }
          }
          const textToShow = displayVal != null ? String(displayVal) : item.name;
          const textColor = displayVal != null ? '#111111' : '#666666';
          const textWeight = displayVal != null ? 'font-weight="500"' : '';

          svgHtml += `
            <rect x="${x}" y="${rowY}" width="${w}" height="${h}" rx="1" fill="${bgBox}" stroke="${strokeBox}" stroke-width="1"/>
            <text x="${x + 6}" y="${rowY + Math.min(14, h - 4)}" font-family="Segoe UI, Tahoma, sans-serif" font-size="10" ${isNonEnterable ? 'font-style="italic"' : ''} ${textWeight} fill="${textColor}">${escapeHtml(textToShow)}</text>
          `;
          if (isList) {
            svgHtml += `
              <rect x="${x + w - 16}" y="${rowY + 1}" width="15" height="${h - 2}" fill="#ece9d8" stroke="#b0ac9d" stroke-width="0.5"/>
              <text x="${x + w - 12}" y="${rowY + h / 2 + 4}" font-size="8" fill="#333">▼</text>
            `;
            if (item.lovName) {
              svgHtml += `<text x="${x + w - 30}" y="${rowY + h / 2 + 4}" font-size="9" fill="#0066cc" title="LOV: ${escapeHtml(item.lovName)}">🔍</text>`;
            }
          }
        }

        svgHtml += `</g>`;
      }
    }

    // Draw multi-record tabular scrollbars (matching classic Oracle Forms runtime)
    const multiRowBlocks = mod.blocks.filter(b => (b.recordsDisplayed || 1) > 1);
    for (const b of multiRowBlocks) {
      const bItems = visualItems.filter(v => v.block.name === b.name);
      if (bItems.length > 0) {
        let maxX = 0;
        let minY = 99999;
        let maxY = 0;
        for (const { item } of bItems) {
          const autoPos = resolvedPositions.get(`${b.name}.${item.name}`);
          const ix = autoPos ? autoPos.x : scale(item.xPosition);
          const iy = autoPos ? autoPos.y : scale(item.yPosition);
          const iw = autoPos ? autoPos.w : (scale(item.width) || 100);
          const ih = autoPos ? autoPos.h : (scale(item.height) || 20);
          const recs = Math.max(1, b.recordsDisplayed || 1);
          const dist = scale(b.distanceBetweenRecords || item.distanceBetweenRecords || 4);
          const totalH = recs * (ih + dist);

          if (ix + iw > maxX) maxX = ix + iw;
          if (iy < minY) minY = iy;
          if (iy + totalH > maxY) maxY = iy + totalH;
        }

        if (maxX > 0 && maxY > minY) {
          const sbX = maxX + 4;
          const sbY = minY;
          const sbW = 15;
          const sbH = maxY - minY - 4;
          svgHtml += `
            <g class="svg-scrollbar" opacity="0.88">
              <!-- Scrollbar track -->
              <rect x="${sbX}" y="${sbY}" width="${sbW}" height="${sbH}" fill="#e0ddd0" stroke="#908c7d" stroke-width="1"/>
              <!-- Up button -->
              <rect x="${sbX}" y="${sbY}" width="${sbW}" height="14" fill="#ece9d8" stroke="#908c7d" stroke-width="0.5"/>
              <text x="${sbX + 3.5}" y="${sbY + 10}" font-size="8" fill="#333">▲</text>
              <!-- Down button -->
              <rect x="${sbX}" y="${sbY + sbH - 14}" width="${sbW}" height="14" fill="#ece9d8" stroke="#908c7d" stroke-width="0.5"/>
              <text x="${sbX + 3.5}" y="${sbY + sbH - 4}" font-size="8" fill="#333">▼</text>
              <!-- Thumb -->
              <rect x="${sbX + 1}" y="${sbY + 16}" width="${sbW - 2}" height="${Math.max(18, sbH / 3)}" fill="#c0bcad" rx="1" stroke="#807c6d" stroke-width="0.5"/>
            </g>
          `;
        }
      }
    }

    formsSvg.innerHTML = svgHtml;
    if (previewEnabled) {
      formsSvg.querySelectorAll('foreignObject[data-preview-block]').forEach(object => {
        const block = mod.blocks.find(b => b.name === object.dataset.previewBlock);
        const item = block.items.find(i => i.name === object.dataset.previewItem);
        object.append(preview.control(block, item, Number(object.dataset.previewRow)));
      });
      canvasStats.textContent += ' | LOCAL PREVIEW: no database or PL/SQL execution';
      oracleStatusMsg.textContent = 'Local preview only - no database or PL/SQL execution';
    }
    applyZoom();
  }

  // Window helper for drag-and-drop visual layout editing
  window.startItemDrag = function (e, blockName, itemName) {
    if (!layoutEditMode || !currentPayload) return;
    e.preventDefault();
    e.stopPropagation();
    window.selectSvgItem(blockName, itemName);

    const block = currentPayload.moduleIndex.blocks.find(b => b.name === blockName);
    const item = block?.items?.find(i => i.name === itemName);
    if (!item) return;

    const startClientX = e.clientX;
    const startClientY = e.clientY;
    const origX = item.xPosition || 0;
    const origY = item.yPosition || 0;

    let badge = document.getElementById('dragCoordsBadge');
    if (!badge) {
      badge = document.createElement('div');
      badge.id = 'dragCoordsBadge';
      badge.className = 'drag-coords-badge';
      document.body.appendChild(badge);
    }
    badge.style.display = 'block';

    const onMouseMove = (moveEvent) => {
      const zoom = zoomLevel || 1.0;
      const dx = (moveEvent.clientX - startClientX) / zoom;
      const dy = (moveEvent.clientY - startClientY) / zoom;
      const newX = Math.max(0, Math.round((origX + dx) / 5) * 5);
      const newY = Math.max(0, Math.round((origY + dy) / 5) * 5);

      item.xPosition = newX;
      item.yPosition = newY;

      badge.style.left = `${moveEvent.clientX + 16}px`;
      badge.style.top = `${moveEvent.clientY + 16}px`;
      badge.textContent = `${blockName}.${itemName}: X=${newX}, Y=${newY}`;

      const propEditX = document.getElementById('propEditX');
      const propEditY = document.getElementById('propEditY');
      if (propEditX) propEditX.value = newX;
      if (propEditY) propEditY.value = newY;

      renderCanvasSvg();
    };

    const onMouseUp = async () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      if (badge) badge.style.display = 'none';

      try {
        await fetch(`/api/forms/module/${encodeURIComponent(currentPayload.moduleIndex.key.name)}/item`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            blockName: block.name,
            itemName: item.name,
            x: item.xPosition,
            y: item.yPosition
          })
        });
      } catch (err) {
        console.warn("Failed to persist dragged position:", err);
      }
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  };

  // Window helper for inline SVG clicks
  window.selectSvgItem = function (blockName, itemName) {
    if (!currentPayload) return;
    const block = currentPayload.moduleIndex.blocks.find(b => b.name === blockName);
    const item = block?.items?.find(i => i.name === itemName);
    if (item && block) {
      selectedObject = { type: 'ITEM', item, block };
      selectedItemStats.textContent = `Selected: ${block.name}.${item.name} (${item.itemType || 'Text Item'})`;
      if (oracleStatusMsg) {
        oracleStatusMsg.textContent = `Block: ${block.name}  Field: ${item.name}  Record: 1/1`;
      }
      renderPropertyInspector(selectedObject);
      renderCanvasSvg();

      // Highlight in left navigator tree
      navigatorTree.querySelectorAll('.tree-node').forEach(node => {
        if (node.getAttribute('data-type') === 'ITEM' && node.getAttribute('data-name') === itemName) {
          node.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
          node.querySelector('.tree-row')?.classList.add('selected');
        } else {
          node.querySelector('.tree-row')?.classList.remove('selected');
        }
      });

      // Load item trigger if exists
      const itemTriggers = currentPayload.moduleIndex.triggers.filter(t => t.level === 'ITEM' && t.itemName === itemName);
      if (itemTriggers.length > 0 && itemTriggers[0].textRef) {
        loadCode(itemTriggers[0].name, itemTriggers[0].textRef.file, 'TRIGGER');
      }
    }
  };

  function highlightCanvasItem(itemName) {
    renderCanvasSvg();
  }

  // Runtime Frame Menu Bar Dynamic Rendering
  function renderRuntimeMenuBar(mod) {
    const container = document.getElementById('oracleMenuItems');
    if (!container) return;

    if (!mod || !mod.menus || mod.menus.length === 0) {
      // Default standard Oracle Forms runtime menus
      container.innerHTML = `
        <div class="menu-item dropdown">
          <span>Action</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Save</span><kbd>Ctrl+S</kbd></div>
            <div class="dropdown-item"><span>Print...</span><kbd>Ctrl+P</kbd></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item"><span>Exit</span><kbd>Ctrl+Q</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Edit</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Cut</span><kbd>Ctrl+X</kbd></div>
            <div class="dropdown-item"><span>Copy</span><kbd>Ctrl+C</kbd></div>
            <div class="dropdown-item"><span>Paste</span><kbd>Ctrl+V</kbd></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item"><span>Clear All</span></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Query</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Enter</span><kbd>F7</kbd></div>
            <div class="dropdown-item"><span>Execute</span><kbd>Ctrl+F11</kbd></div>
            <div class="dropdown-item"><span>Cancel</span><kbd>Ctrl+Q</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Block</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Previous</span><kbd>Shift+F5</kbd></div>
            <div class="dropdown-item"><span>Next</span><kbd>Shift+F6</kbd></div>
            <div class="dropdown-item"><span>Clear</span></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Record</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Previous</span><kbd>Up</kbd></div>
            <div class="dropdown-item"><span>Next</span><kbd>Down</kbd></div>
            <div class="dropdown-item"><span>Insert</span><kbd>F6</kbd></div>
            <div class="dropdown-item"><span>Remove</span><kbd>Shift+F6</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Field</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Clear</span></div>
            <div class="dropdown-item"><span>Duplicate</span><kbd>F3</kbd></div>
            <div class="dropdown-item"><span>List of Values...</span><kbd>F9</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Help</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Keys...</span><kbd>Ctrl+K</kbd></div>
            <div class="dropdown-item"><span>About Forms...</span></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Window</span>
          <div class="dropdown-menu">
            <div class="dropdown-item"><span>Cascade</span></div>
            <div class="dropdown-item"><span>Tile</span></div>
          </div>
        </div>
      `;
      container.querySelectorAll('.dropdown-item').forEach(element => {
        element.addEventListener('click', () => {
          if (previewEnabled) showPreviewDialog(element.textContent.trim(), 'Standard menu action not executed in this local preview.');
        });
      });
      return;
    }

    // Dynamic Menu Module from Form
    const menuMap = new Map();
    for (const m of mod.menus) {
      menuMap.set(m.name.toUpperCase(), m);
    }

    const rootMenu = menuMap.get('MAIN_MENU') || menuMap.get('MAIN') || mod.menus[0];
    let menuHtml = '';

    for (const item of (rootMenu.items || [])) {
      const label = item.label || item.name;
      const subName = item.subMenuName ? item.subMenuName.toUpperCase() : null;
      const subMenu = subName ? menuMap.get(subName) : null;

      if (subMenu && subMenu.items && subMenu.items.length > 0) {
        let subHtml = '';
        for (const sub of subMenu.items) {
          if (sub.menuItemType === 'Separator' || sub.name.toUpperCase().startsWith('SEPARATOR') || (!sub.label && !sub.commandRef)) {
            subHtml += `<div class="dropdown-divider"></div>`;
          } else {
            const subLabel = sub.label || sub.name;
            const hasCode = !!sub.commandRef;
            subHtml += `
              <div class="dropdown-item" data-menu="${escapeHtml(subMenu.name)}" data-item="${escapeHtml(sub.name)}" title="${hasCode ? 'Click to inspect PL/SQL logic' : ''}">
                <span>${escapeHtml(subLabel.replace(/&/g, ''))}</span>
                ${hasCode ? '<span class="item-tag" style="font-size:9px;color:#0066cc;">PL/SQL</span>' : ''}
              </div>
            `;
          }
        }
        menuHtml += `
          <div class="menu-item dropdown">
            <span>${escapeHtml(label.replace(/&/g, ''))}</span>
            <div class="dropdown-menu">
              ${subHtml}
            </div>
          </div>
        `;
      } else {
        const hasCode = !!item.commandRef;
        menuHtml += `
          <div class="menu-item" data-menu="${escapeHtml(rootMenu.name)}" data-item="${escapeHtml(item.name)}">
            <span>${escapeHtml(label.replace(/&/g, ''))}</span>
          </div>
        `;
      }
    }

    container.innerHTML = menuHtml;

    // Attach click listeners to menu items to inspect PL/SQL code
    container.querySelectorAll('.dropdown-item, .menu-item').forEach(el => {
      el.addEventListener('click', () => {
        const menuName = el.getAttribute('data-menu');
        const itemName = el.getAttribute('data-item');
        if (!menuName || !itemName) return;
        const targetMenu = menuMap.get(menuName.toUpperCase());
        const targetItem = targetMenu?.items?.find(i => i.name === itemName);
        if (targetItem) {
          selectedObject = { type: 'MENU_ITEM', item: targetItem, menu: targetMenu };
          renderPropertyInspector(selectedObject);
          if (previewEnabled) {
            showPreviewDialog(targetItem.label || targetItem.name, 'Menu command not executed. No application navigation is inferred.',
              targetItem.commandRef ? { name: `Menu: ${targetMenu.name}.${targetItem.name}`, file: targetItem.commandRef.file, module: menuSourceModule } : null);
          } else if (targetItem.commandRef) {
            loadCode(`Menu: ${targetMenu.name}.${targetItem.name}`, targetItem.commandRef.file, 'MENU_ITEM', menuSourceModule);
          }
        }
      });
    });
  }

  // Property Inspector Rendering
  function renderPropertyInspector(target) {
    if (!target) {
      propertyInspector.innerHTML = '<div class="empty-state">Select an object to inspect properties.</div>';
      inspectorTargetName.textContent = 'None';
      return;
    }

    let title = '';
    let html = '';

    if (target.type === 'ITEM') {
      const item = target.item;
      const block = target.block;
      title = `${block.name}.${item.name}`;

      // General
      html += renderPropGroup('General', [
        ['Name', item.name],
        ['Block', block.name],
        ['Item Type', item.itemType || 'Text Item'],
        ['Data Type', item.dataType || 'Char'],
        ['Property Class', item.propertyClass || '(None)'],
        ['Inherited From', item.inherited ? formatInheritance(item.inherited) : '(Local)']
      ]);

      // Layout
      if (layoutEditMode) {
        html += `
          <div class="prop-group">
            <div class="prop-group-header">Layout Editor</div>
            <table class="prop-table">
              <tr><td class="prop-name">X Position</td><td class="prop-val"><input type="number" class="prop-input" id="propEditX" value="${item.xPosition || 0}"></td></tr>
              <tr><td class="prop-name">Y Position</td><td class="prop-val"><input type="number" class="prop-input" id="propEditY" value="${item.yPosition || 0}"></td></tr>
              <tr><td class="prop-name">Width</td><td class="prop-val"><input type="number" class="prop-input" id="propEditW" value="${item.width || ''}" placeholder="Default"></td></tr>
              <tr><td class="prop-name">Height</td><td class="prop-val"><input type="number" class="prop-input" id="propEditH" value="${item.height || ''}" placeholder="Default"></td></tr>
              <tr><td class="prop-name">Prompt</td><td class="prop-val"><input type="text" class="prop-input" id="propEditPrompt" value="${escapeHtml(item.prompt || '')}"></td></tr>
              <tr><td class="prop-name">Label</td><td class="prop-val"><input type="text" class="prop-input" id="propEditLabel" value="${escapeHtml(item.label || '')}"></td></tr>
            </table>
            <div style="padding: 6px 8px;">
              <button class="btn btn-sm btn-primary" id="propApplyBtn" style="width: 100%;">Apply & Save</button>
            </div>
          </div>
        `;
      } else {
        html += renderPropGroup('Layout', [
          ['Canvas', item.canvasName || '(None)'],
          ['Tab Page', item.tabPageName || '(None)'],
          ['X Position', item.xPosition],
          ['Y Position', item.yPosition],
          ['Width', item.width || '(Default)'],
          ['Height', item.height || '(Default)'],
          ['Visible', item.visible !== false ? 'True' : 'False'],
          ['Prompt', item.prompt || '(None)'],
          ['Prompt Attachment', `${item.promptAttachmentEdge} (${item.promptAttachmentOffset}px)`]
        ]);
      }

      // Data / DML
      html += renderPropGroup('Data & DML', [
        ['Database Item', item.dml?.databaseItem !== false ? 'True' : 'False'],
        ['Column Name', item.columnName || item.name],
        ['Primary Key', item.dml?.primaryKey === true ? 'True' : 'False'],
        ['Required', item.required === true ? 'True' : 'False'],
        ['Max Length', item.dml?.maximumLength || 'Default'],
        ['LOV Name', item.lovName || '(None)'],
        ['Initial Value', item.dml?.initialValue || '(None)'],
        ['Insert / Update', `${item.dml?.insertAllowed !== false ? 'Insert ' : ''}${item.dml?.updateAllowed !== false ? 'Update' : ''}`]
      ]);

      // Triggers
      const itemTriggers = currentPayload.moduleIndex.triggers.filter(t => t.level === 'ITEM' && t.itemName === item.name);
      if (itemTriggers.length > 0) {
        html += `
          <div class="prop-group">
            <div class="prop-group-header">Attached Triggers (${itemTriggers.length})</div>
            <div class="trigger-pills">
              ${itemTriggers.map(t => `<span class="trigger-pill" onclick="window.loadCodeFromPill('${escapeHtml(t.name)}', '${escapeHtml(t.textRef?.file || '')}')">⚡ ${escapeHtml(t.name)}</span>`).join('')}
            </div>
          </div>
        `;
      }

    } else if (target.type === 'BLOCK') {
      const b = target.block;
      title = `Block: ${b.name}`;

      html += renderPropGroup('General', [
        ['Name', b.name],
        ['Query Data Source', b.queryDataSourceName || '(Control Block)'],
        ['Property Class', b.propertyClass || '(None)'],
        ['Inherited From', b.inherited ? formatInheritance(b.inherited) : '(Local)']
      ]);

      html += renderPropGroup('Multi-Record Grid', [
        ['Records Displayed', b.recordsDisplayed],
        ['Distance Between Records', b.distanceBetweenRecords],
        ['Orientation', b.recordOrientation]
      ]);

      html += renderPropGroup('DML & Query', [
        ['Database Block', b.dml?.databaseBlock !== false ? 'True' : 'False'],
        ['Where Clause', b.dml?.whereClause || '(None)'],
        ['Order By Clause', b.dml?.orderByClause || '(None)']
      ]);

      const blockTriggers = currentPayload.moduleIndex.triggers.filter(t => t.level === 'BLOCK' && t.blockName === b.name);
      if (blockTriggers.length > 0) {
        html += `
          <div class="prop-group">
            <div class="prop-group-header">Block Triggers (${blockTriggers.length})</div>
            <div class="trigger-pills">
              ${blockTriggers.map(t => `<span class="trigger-pill" onclick="window.loadCodeFromPill('${escapeHtml(t.name)}', '${escapeHtml(t.textRef?.file || '')}')">⚡ ${escapeHtml(t.name)}</span>`).join('')}
            </div>
          </div>
        `;
      }

    } else if (target.type === 'TRIGGER') {
      const t = target.trigger;
      title = `Trigger: ${t.name}`;
      html += renderPropGroup('Trigger Info', [
        ['Name', t.name],
        ['Level', t.level],
        ['Scope', t.itemName ? `${t.blockName}.${t.itemName}` : (t.blockName || 'Form')],
        ['Line Count', t.lineCount],
        ['Encoding', t.textEncoding],
        ['Inherited From', t.inherited ? formatInheritance(t.inherited) : '(Local)']
      ]);
    } else if (target.type === 'PROGRAM_UNIT') {
      const pu = target.programUnit;
      title = `Program Unit: ${pu.name}`;
      html += renderPropGroup('Program Unit Info', [
        ['Name', pu.name],
        ['Type', pu.unitType],
        ['Line Count', pu.lineCount],
        ['Inherited From', pu.inherited ? formatInheritance(pu.inherited) : '(Local)']
      ]);
    } else if (target.type === 'CANVAS') {
      const c = target.canvas;
      title = `Canvas: ${c.name}`;
      html += renderPropGroup('Canvas Info', [
        ['Name', c.name],
        ['Canvas Type', c.canvasType || 'Content'],
        ['Window', c.windowName || 'Default'],
        ['Width', c.width],
        ['Height', c.height],
        ['Tab Pages', c.tabPages.map(t => t.name).join(', ') || '(None)']
      ]);
    } else if (target.type === 'WINDOW') {
      const w = target.window;
      title = `Window: ${w.name}`;
      html += renderPropGroup('Window Info', [
        ['Name', w.name],
        ['Title', w.title || '(None)'],
        ['Modal', w.modal === true ? 'True' : 'False'],
        ['Width', w.width || 'Default'],
        ['Height', w.height || 'Default']
      ]);
    } else {
      title = target.type;
      html = `<div class="empty-state">${target.type} properties inspected.</div>`;
    }

    inspectorTargetName.textContent = title;
    propertyInspector.innerHTML = html;

    if (layoutEditMode && target.type === 'ITEM') {
      const applyBtn = document.getElementById('propApplyBtn');
      if (applyBtn) {
        applyBtn.addEventListener('click', async () => {
          const item = target.item;
          const block = target.block;
          const x = parseInt(document.getElementById('propEditX')?.value, 10) || 0;
          const y = parseInt(document.getElementById('propEditY')?.value, 10) || 0;
          const wVal = document.getElementById('propEditW')?.value;
          const w = wVal ? parseInt(wVal, 10) : null;
          const hVal = document.getElementById('propEditH')?.value;
          const h = hVal ? parseInt(hVal, 10) : null;
          const prompt = document.getElementById('propEditPrompt')?.value || null;
          const label = document.getElementById('propEditLabel')?.value || null;

          item.xPosition = x;
          item.yPosition = y;
          item.width = w;
          item.height = h;
          item.prompt = prompt;
          item.label = label;

          renderCanvasSvg();

          try {
            await fetch(`/api/forms/module/${encodeURIComponent(currentPayload.moduleIndex.key.name)}/item`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                blockName: block.name,
                itemName: item.name,
                x, y, width: w, height: h,
                prompt, label
              })
            });
            showBanner(`Saved properties for ${block.name}.${item.name}`);
            setTimeout(hideBanner, 2000);
          } catch (err) {
            showBanner('Failed to save properties: ' + err.message);
          }
        });
      }
    }
  }

  function renderDiffReport(diff) {
    if (!diffContainer) return;
    if (!diff || !diff.baseName) {
      diffContainer.innerHTML = '<div class="empty-state">No diff results available.</div>';
      return;
    }

    const hasDiff = (diff.blocksAdded?.length || 0) > 0 ||
      (diff.blocksRemoved?.length || 0) > 0 ||
      (diff.blocksModified?.length || 0) > 0 ||
      (diff.canvasesModified?.length || 0) > 0 ||
      (diff.windowsModified?.length || 0) > 0 ||
      (diff.formTriggersAdded?.length || 0) > 0 ||
      (diff.formTriggersRemoved?.length || 0) > 0 ||
      (diff.formTriggersModified?.length || 0) > 0;

    let html = `
      <div class="diff-header-card">
        <div style="font-weight: bold; font-size: 14px;">Form Diff: ${escapeHtml(diff.baseName)} vs ${escapeHtml(diff.modifiedName)}</div>
        <div class="diff-stat-pills">
          <span class="diff-pill added">+ ${diff.blocksAdded?.length || 0} Blocks Added</span>
          <span class="diff-pill removed">- ${diff.blocksRemoved?.length || 0} Blocks Removed</span>
          <span class="diff-pill modified">~ ${diff.blocksModified?.length || 0} Blocks Modified</span>
          <span class="diff-pill modified">~ ${diff.canvasesModified?.length || 0} Canvases</span>
          <span class="diff-pill modified">~ ${diff.windowsModified?.length || 0} Windows</span>
        </div>
      </div>
    `;

    if (!hasDiff) {
      html += '<div style="padding: 16px; color: #3fb950; font-weight: bold;">✔️ No differences detected. Modules have matching structure and layout.</div>';
      diffContainer.innerHTML = html;
      return;
    }

    if (diff.blocksAdded?.length) {
      html += `<div class="diff-block-section"><div class="diff-block-title" style="color: #3fb950;">+ Blocks Added</div>`;
      diff.blocksAdded.forEach(b => {
        html += `<div class="diff-change-item item-add">+ Block: <strong>${escapeHtml(b)}</strong></div>`;
      });
      html += `</div>`;
    }

    if (diff.blocksRemoved?.length) {
      html += `<div class="diff-block-section"><div class="diff-block-title" style="color: #f85149;">- Blocks Removed</div>`;
      diff.blocksRemoved.forEach(b => {
        html += `<div class="diff-change-item item-rem">- Block: <strong>${escapeHtml(b)}</strong></div>`;
      });
      html += `</div>`;
    }

    for (const b of (diff.blocksModified || [])) {
      html += `
        <div class="diff-block-section">
          <div class="diff-block-title">Block: ${escapeHtml(b.name)}</div>
      `;
      for (const pc of (b.propertyChanges || [])) {
        html += `<div class="diff-change-item item-mod">* Property '${escapeHtml(pc.property)}': ${escapeHtml(pc.oldValue || 'null')} &rarr; <strong>${escapeHtml(pc.newValue || 'null')}</strong></div>`;
      }
      for (const item of (b.itemsAdded || [])) {
        html += `<div class="diff-change-item item-add">+ Item Added: <strong>${escapeHtml(item.name)}</strong> (${escapeHtml(item.itemType || 'Text Item')}) at (${item.xPosition}, ${item.yPosition})</div>`;
      }
      for (const item of (b.itemsRemoved || [])) {
        html += `<div class="diff-change-item item-rem">- Item Removed: <strong>${escapeHtml(item.name)}</strong></div>`;
      }
      for (const item of (b.itemsModified || [])) {
        html += `<div class="diff-change-item item-mod">~ Item <strong>${escapeHtml(item.name)}</strong>:</div>`;
        if (item.geometryDiff) {
          const g = item.geometryDiff;
          html += `<div class="diff-change-item" style="padding-left: 28px; color: #79c0ff;">Position: (${g.oldX}, ${g.oldY}) &rarr; <strong>(${g.newX}, ${g.newY})</strong>`;
          if (g.oldWidth !== g.newWidth || g.oldHeight !== g.newHeight) {
            html += ` | Size: (${g.oldWidth}x${g.oldHeight}) &rarr; <strong>(${g.newWidth}x${g.newHeight})</strong>`;
          }
          html += `</div>`;
        }
        for (const pc of (item.propertyChanges || [])) {
          html += `<div class="diff-change-item" style="padding-left: 28px;">Property '${escapeHtml(pc.property)}': ${escapeHtml(pc.oldValue || 'null')} &rarr; <strong>${escapeHtml(pc.newValue || 'null')}</strong></div>`;
        }
      }
      for (const t of (b.triggersAdded || [])) {
        html += `<div class="diff-change-item item-add">+ Trigger Added: <strong>${escapeHtml(t)}</strong></div>`;
      }
      for (const t of (b.triggersRemoved || [])) {
        html += `<div class="diff-change-item item-rem">- Trigger Removed: <strong>${escapeHtml(t)}</strong></div>`;
      }
      for (const t of (b.triggersModified || [])) {
        html += `<div class="diff-change-item item-mod">~ Trigger Modified: <strong>${escapeHtml(t.name)}</strong></div>`;
      }
      html += `</div>`;
    }

    diffContainer.innerHTML = html;
  }

  function renderPropGroup(groupTitle, rows) {
    return `
      <div class="prop-group">
        <div class="prop-group-header">${groupTitle}</div>
        <table class="prop-table">
          ${rows.map(([k, v]) => `<tr><td class="prop-name">${escapeHtml(k)}</td><td class="prop-val">${escapeHtml(String(v ?? ''))}</td></tr>`).join('')}
        </table>
      </div>
    `;
  }

  function formatInheritance(inh) {
    if (!inh) return '(None)';
    const parts = [];
    if (inh.module) parts.push(`Mod: ${inh.module}`);
    if (inh.file) parts.push(`File: ${inh.file}`);
    if (inh.name) parts.push(`Name: ${inh.name}`);
    if (inh.objectGroup) parts.push(`Group: ${inh.objectGroup}`);
    return parts.join(' | ') || 'Subclass';
  }

  // PL/SQL Code Loading & Highlighting
  window.loadCodeFromPill = function (name, filePath) {
    loadCode(name, filePath, 'TRIGGER');
  };

  async function loadCode(name, relativePath, scope, sourceModule = null) {
    if (!currentPayload || !relativePath) return;
    const moduleName = sourceModule || currentPayload.summary.moduleName;
    codeTitle.textContent = name;
    codeScopeBadge.textContent = scope;

    try {
      const res = await fetch(`/api/module/${encodeURIComponent(moduleName)}/source?path=${encodeURIComponent(relativePath)}`);
      if (res.ok) {
        const data = await res.json();
        plsqlCodeDisplay.innerHTML = highlightPlsql(data.code);
        switchTab('codeTab');
      } else {
        plsqlCodeDisplay.textContent = `Could not load PL/SQL source: ${await res.text()}`;
      }
    } catch (e) {
      plsqlCodeDisplay.textContent = `Error fetching source: ${e.message}`;
    }
  }

  // Raw XML Slice Loading
  async function loadXmlSlice(name, startLine, endLine) {
    if (!currentPayload || !startLine) return;
    const moduleName = currentPayload.summary.moduleName;
    xmlTitle.textContent = `${name} (Lines ${startLine} - ${endLine})`;
    xmlLinesBadge.textContent = `${endLine - startLine + 1} lines`;

    try {
      const res = await fetch(`/api/module/${encodeURIComponent(moduleName)}/xml?startLine=${startLine}&endLine=${endLine}`);
      if (res.ok) {
        const data = await res.json();
        xmlSliceDisplay.textContent = data.xml;
      }
    } catch (e) {
      console.warn("Could not load XML slice:", e);
    }
  }

  // Lightweight PL/SQL Syntax Highlighter
  function highlightPlsql(code) {
    if (!code) return '<em>-- (Empty PL/SQL body)</em>';

    const lines = code.split('\n');
    const highlightedLines = lines.map((line, idx) => {
      const lineNum = `<span style="color:#555;user-select:none;display:inline-block;width:35px;text-align:right;margin-right:12px;">${idx + 1}</span>`;

      // Check comments
      if (line.trim().startsWith('--')) {
        return lineNum + `<span class="hl-com">${escapeHtml(line)}</span>`;
      }

      let escaped = escapeHtml(line);

      // Strings
      escaped = escaped.replace(/(&#39;.*?&#39;|'.*?')/g, '<span class="hl-str">$1</span>');

      // Bind variables (:BLOCK.ITEM)
      escaped = escaped.replace(/(:\b[A-Za-z0-9_$.#]+)/g, '<span class="hl-bind">$1</span>');

      // Keywords
      const keywords = /\b(BEGIN|END|IF|THEN|ELSE|ELSIF|CASE|WHEN|LOOP|FOR|WHILE|DECLARE|PROCEDURE|FUNCTION|PACKAGE|BODY|EXCEPTION|RETURN|NULL|EXIT|RAISE|FORM_TRIGGER_FAILURE|COMMIT|ROLLBACK|COMMIT_FORM|CLEAR_FORM|EXECUTE_QUERY|ENTER_QUERY|SELECT|INTO|FROM|WHERE|AND|OR|NOT|IN|IS|LIKE|ORDER BY|GROUP BY|INSERT|UPDATE|DELETE|VALUES|SET|CREATE|TABLE|VIEW|CURSOR|OPEN|FETCH|CLOSE)\b/gi;
      escaped = escaped.replace(keywords, '<span class="hl-kw">$1</span>');

      // Numbers
      escaped = escaped.replace(/\b(\d+)\b/g, '<span class="hl-num">$1</span>');

      return lineNum + escaped;
    });

    return highlightedLines.join('\n');
  }

  function unescapeXml(str) {
    if (!str) return '';
    return str
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&apos;/g, "'");
  }

  function escapeHtml(str) {
    if (!str) return '';
    return unescapeXml(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // Reports Engine Functions
  async function loadReportsList() {
    if (!reportSelect) return;
    try {
      const res = await fetch('/api/reports');
      if (res.ok) {
        const reports = await res.json();
        reportSelect.innerHTML = reports.map(r => `<option value="${r.file_name}">${r.name} - ${r.title}</option>`).join('');
      }
    } catch (e) {
      console.warn("Could not load reports list:", e);
    }
  }

  async function executeReport() {
    if (!reportSelect) return;
    const reportName = reportSelect.value;
    const backend = reportBackendSelect?.value || 'mock';
    if (!reportName) return;

    if (reportDataTree) {
      reportDataTree.innerHTML = '<div class="empty-state">Running report...</div>';
    }

    try {
      const res = await fetch('/api/reports/run', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          report_name: reportName,
          backend: backend,
          parameters: {}
        })
      });

      if (!res.ok) {
        const err = await res.json();
        throw new Error(err.error || 'Report execution failed');
      }

      const data = await res.json();

      if (reportPreviewFrame) {
        reportPreviewFrame.srcdoc = data.html;
      }

      renderReportDataTree(data.execution);
    } catch (err) {
      if (reportDataTree) {
        reportDataTree.innerHTML = `<div class="empty-state" style="color:#f85149;">Error: ${escapeHtml(err.message)}</div>`;
      }
    }
  }

  function renderReportDataTree(execution) {
    if (!reportDataTree) return;
    let html = '';

    html += '<div class="report-summary-box">';
    html += `<h5>📊 ${escapeHtml(execution.report_name)} (${escapeHtml(execution.unit)})</h5>`;
    const sumKeys = Object.keys(execution.report_summaries || {});
    if (sumKeys.length === 0) {
      html += '<div class="report-sum-row"><span class="report-sum-label">No grand summaries</span></div>';
    } else {
      for (const k of sumKeys) {
        const val = execution.report_summaries[k];
        html += `<div class="report-sum-row"><span class="report-sum-label">${escapeHtml(k)}:</span> <span class="report-sum-value">${typeof val === 'number' ? val.toLocaleString(undefined, {minimumFractionDigits: 2}) : val}</span></div>`;
      }
    }
    html += '</div>';

    const groupKeys = Object.keys(execution.group_records || {});
    for (const g of groupKeys) {
      const recs = execution.group_records[g] || [];
      html += '<div class="report-group-box">';
      html += `<h5>Group: ${escapeHtml(g)} (${recs.length} records)</h5>`;
      if (recs.length > 0) {
        const sampleRec = recs[0];
        const cols = Object.keys(sampleRec.values || {}).slice(0, 4);
        html += `<div style="color:#8b949e;font-size:10px;margin-bottom:6px;">Columns: ${cols.join(', ')}</div>`;
        for (let i = 0; i < Math.min(recs.length, 3); i++) {
          const r = recs[i];
          const summaryEntries = Object.entries(r.group_summaries || {});
          let sumText = summaryEntries.length > 0 ? ` [${summaryEntries.map(([k, v]) => `${k}=${v}`).join(', ')}]` : '';
          html += `<div style="font-family:var(--font-mono);font-size:10px;margin-bottom:3px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">Row ${i+1}: ${JSON.stringify(r.values)}${sumText}</div>`;
        }
        if (recs.length > 3) {
          html += `<div style="color:#6e7681;font-size:10px;">... and ${recs.length - 3} more records</div>`;
        }
      }
      html += '</div>';
    }

    reportDataTree.innerHTML = html;
  }
})();
