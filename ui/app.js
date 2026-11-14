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
  const runtime = new FormsRuntime();
  const preview = new FormsPreview(
    async (block, item, row) => {
      selectedObject = { type: 'ITEM', block, item };
      renderPropertyInspector(selectedObject);
      if (layoutEditMode) {
        renderCanvasSvg();
        return;
      }

      const nameUp = (item.name || '').toUpperCase();
      const labelUp = unescapeXml(item.label || item.prompt || '').toUpperCase();
      const builtin = (item.builtinAction || '').toUpperCase();

      const hasTrigger = currentPayload?.moduleIndex?.triggers?.some(t =>
        t.level === 'ITEM' && t.blockName === block.name && t.itemName === item.name &&
        t.name.toUpperCase() === 'WHEN-BUTTON-PRESSED');

      // Handle explicit built-in or wizard-added in-form navigation buttons when no item trigger overrides them
      if (!hasTrigger || builtin) {
        if (builtin === 'PREVIOUS_RECORD' || nameUp.includes('BTN_PREV') || labelUp.includes('PREVIOUS') || labelUp === '< PREV') {
          if (!runtime.active && (queryRecordsByBlock.get(getActiveBlockName()) || []).length === 0) {
            await executeBlockQuery();
          }
          navigateRecord('prev');
          return;
        }
        if (builtin === 'NEXT_RECORD' || nameUp.includes('BTN_NEXT') || labelUp.includes('NEXT >') || labelUp === 'NEXT') {
          if (!runtime.active && (queryRecordsByBlock.get(getActiveBlockName()) || []).length === 0) {
            await executeBlockQuery();
          }
          navigateRecord('next');
          return;
        }
        if (builtin === 'FIRST_RECORD' || nameUp.includes('BTN_FIRST') || labelUp.includes('FIRST')) {
          navigateRecord('first');
          return;
        }
        if (builtin === 'LAST_RECORD' || nameUp.includes('BTN_LAST') || labelUp.includes('LAST')) {
          navigateRecord('last');
          return;
        }
        if (builtin === 'EXECUTE_QUERY' || nameUp.includes('BTN_QUERY') || (nameUp.includes('BTN_SEARCH') && !hasTrigger)) {
          await executeBlockQuery();
          return;
        }
        if (builtin === 'COMMIT_FORM' || nameUp.includes('BTN_SAVE') || labelUp === 'SAVE' || labelUp.includes('SAVE')) {
          if (runtime.active) await runtimeSend({ kind: 'commit' });
          else showBanner('Changes saved in session.');
          return;
        }
        if (builtin === 'CLEAR_BLOCK' || nameUp.includes('BTN_CLEAR') || labelUp === 'CLEAR') {
          if (runtime.active) await runtimeSend({ kind: 'key', name: 'CLEAR_BLOCK' });
          else { preview.reset(); renderCanvasSvg(); }
          return;
        }
      }

      // If form session is not yet active, auto-start it so in-form buttons work immediately!
      if (!runtime.active) {
        const pendingEdits = [];
        for (const [k, v] of preview.values.entries()) {
          if (v !== '' && v != null) {
            try {
              const [b, it, r] = JSON.parse(k);
              pendingEdits.push({ block: b, item: it, record: r || 0, value: String(v) });
            } catch (_) {}
          }
        }
        await startRuntimeSession();
        if (runtime.active) {
          for (const edit of pendingEdits) {
            const bObj = currentPayload?.moduleIndex?.blocks?.find(b => b.name.toUpperCase() === edit.block);
            if (bObj && !bObj.queryDataSourceName) {
              await runtimeSend({ kind: 'setItem', block: edit.block, item: edit.item, value: edit.value, record: edit.record });
            }
          }
          await runtimeSend({ kind: 'button', block: block.name, item: item.name, record: recordForRow(block.name, row) });
          return;
        }
      }

      if (runtime.active) {
        await runtimeSend({ kind: 'button', block: block.name, item: item.name, record: recordForRow(block.name, row) });
        return;
      }
    },
    (block, item, row) => {
      if (runtime.active && lastSessionState) {
        const record = recordForRow(block.name, row);
        const sameBlock = (lastSessionState.cursorBlock || '').toUpperCase() === block.name.toUpperCase();
        const current = activeRecordIndexByBlock.get(block.name) || 0;
        const hasRecord = record < (queryRecordsByBlock.get(block.name) || []).length;
        if (hasRecord && (!sameBlock || record !== current) && !(item.itemType || '').toLowerCase().includes('button')) {
          runtimeSend({ kind: 'navigate', block: block.name, item: item.name, record });
        }
      }
      selectedObject = { type: 'ITEM', block, item };
      renderPropertyInspector(selectedObject);
      selectedItemStats.textContent = `Item: ${block.name}.${item.name} (Record ${row + 1})`;
    },
    (block, item, row, value) => {
      if (!runtime.active) return;
      runtimeSend({ kind: 'setItem', block: block.name, item: item.name, value, record: recordForRow(block.name, row) });
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

  // Live form session controls
  const runFormBtn = document.getElementById('runFormBtn');
  const stopFormBtn = document.getElementById('stopFormBtn');
  const runtimeCommitBtn = document.getElementById('runtimeCommitBtn');
  const allowWritesToggle = document.getElementById('allowWritesToggle');
  const dbUserInput = document.getElementById('dbUserInput');
  const dbPassInput = document.getElementById('dbPassInput');
  const sessionBadge = document.getElementById('sessionBadge');
  const sessionTracePanel = document.getElementById('sessionTracePanel');
  const sessionTrace = document.getElementById('sessionTrace');
  const runtimeAlertDialog = document.getElementById('runtimeAlertDialog');

  // Query engine state
  const queryRecordsByBlock = new Map();
  const activeRecordIndexByBlock = new Map();
  // Selected tab page of tab canvases drawn on top of the content canvas.
  const overlayTabPage = new Map();

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

  function normalizeOracleColor(c) {
    if (!c) return null;
    const s = String(c).trim();
    const m = /^r(\d+)g(\d+)b(\d+)$/i.exec(s);
    if (m) {
      const scale = (Number(m[1]) <= 100 && Number(m[2]) <= 100 && Number(m[3]) <= 100 && s === s.toLowerCase()) ? 2.55 : 1;
      const r = Math.min(255, Math.round(Number(m[1]) * scale));
      const g = Math.min(255, Math.round(Number(m[2]) * scale));
      const b = Math.min(255, Math.round(Number(m[3]) * scale));
      return '#' + [r, g, b].map(x => x.toString(16).padStart(2, '0')).join('');
    }
    return s;
  }

  // Initialize
  document.addEventListener('DOMContentLoaded', async () => {
    setupTabSwitching();
    setupControls();
    setupWorkspaceEnhancements();
    setupBuilderStudio();
    loadSampleList();
    await loadReportsList();
    // Pre-load default or URL-requested sample & mode
    const urlParams = new URLSearchParams(window.location.search);
    const initialSample = urlParams.get('sample') || 'ofw_demo_employees_fmb.xml';
    const initialMode = urlParams.get('mode');
    loadSample(initialSample).then(async () => {
      const initialTab = urlParams.get('tab');
      if (initialTab === 'reports') {
        switchTab('reportsTab');
        const repMode = urlParams.get('reportMode');
        if (repMode === 'split') document.getElementById('repModeSplitBtn')?.click();
        else if (repMode === 'plsql') document.getElementById('repModePlsqlBtn')?.click();
      } else if (urlParams.get('modal') === 'browse') {
        document.getElementById('openFileModalBtn')?.click();
      } else if (initialMode === 'edit') {
        document.getElementById('editLayoutToggleBtn')?.click();
        window.selectSvgItem?.('CONTROL', 'BTN_SEARCH');
      } else if (initialMode === 'wizard') {
        document.getElementById('openWizardBtn')?.click();
      } else if (initialMode === 'run') {
        await startRuntimeSession();
      }
    });
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
        const leftPane = document.getElementById('leftPane');
        const toggleLeftBtn = document.getElementById('toggleLeftPaneBtn');
        if (viewId === 'reportsTab') {
          leftPane?.classList.add('collapsed');
          toggleLeftBtn?.classList.remove('active');
          executeReport();
        } else if (viewId === 'canvasTab') {
          leftPane?.classList.remove('collapsed');
          toggleLeftBtn?.classList.add('active');
          setTimeout(zoomFit, 50);
        }
      });
    });
  }

  function switchTab(tabId) {
    const btn = document.querySelector(`.tab-btn[data-tab="${tabId}"]`);
    if (btn) btn.click();
  }

  function openBottomDrawer(tabPaneId = 'drawerPlsql') {
    const drawer = document.getElementById('bottomCodeDrawer');
    const toggleBtn = document.getElementById('toggleCodeDrawerBtn');
    if (drawer) drawer.classList.remove('hidden');
    if (toggleBtn) toggleBtn.classList.add('active');
    if (tabPaneId) {
      document.querySelectorAll('.drawer-tab').forEach(b => {
        b.classList.toggle('active', b.getAttribute('data-drawer') === tabPaneId);
      });
      document.querySelectorAll('.drawer-pane').forEach(p => {
        p.classList.toggle('active', p.id === tabPaneId);
      });
    }
  }

  function setupWorkspaceEnhancements() {
    const leftPane = document.getElementById('leftPane');
    const rightPane = document.getElementById('rightPane');
    const bottomDrawer = document.getElementById('bottomCodeDrawer');
    const toggleLeftBtn = document.getElementById('toggleLeftPaneBtn');
    const closeLeftBtn = document.getElementById('closeLeftPaneBtn');
    const toggleRightBtn = document.getElementById('toggleRightPaneBtn');
    const closeRightBtn = document.getElementById('closeRightPaneBtn');
    const toggleDrawerBtn = document.getElementById('toggleCodeDrawerBtn');
    const closeDrawerBtn = document.getElementById('closeDrawerBtn');
    const focusModeBtn = document.getElementById('focusModeBtn');

    const toggleLeft = () => {
      const collapsed = leftPane.classList.toggle('collapsed');
      toggleLeftBtn?.classList.toggle('active', !collapsed);
      setTimeout(zoomFit, 220);
    };
    const toggleRight = () => {
      const collapsed = rightPane.classList.toggle('collapsed');
      toggleRightBtn?.classList.toggle('active', !collapsed);
      setTimeout(zoomFit, 220);
    };
    const setReportViewMode = (mode) => {
      const repLayout = document.getElementById('reportsMainLayout');
      const repPane = document.getElementById('reportPlsqlStudioPane');
      if (!repLayout) return;
      repLayout.classList.remove('mode-preview', 'mode-split', 'mode-plsql');
      repLayout.classList.add(`mode-${mode}`);
      if (repPane) repPane.classList.toggle('hidden', mode === 'preview');
      document.getElementById('repModePreviewBtn')?.classList.toggle('active', mode === 'preview');
      document.getElementById('repModeSplitBtn')?.classList.toggle('active', mode === 'split');
      document.getElementById('repModePlsqlBtn')?.classList.toggle('active', mode === 'plsql');
      if (document.getElementById('reportsTab')?.classList.contains('active')) {
        toggleDrawerBtn?.classList.toggle('active', mode !== 'preview');
      }
    };

    document.getElementById('repModePreviewBtn')?.addEventListener('click', () => setReportViewMode('preview'));
    document.getElementById('repModeSplitBtn')?.addEventListener('click', () => setReportViewMode('split'));
    document.getElementById('repModePlsqlBtn')?.addEventListener('click', () => setReportViewMode('plsql'));
    document.getElementById('copyReportPlsqlBtn')?.addEventListener('click', () => {
      const text = document.getElementById('reportPlsqlDisplay')?.innerText || '';
      navigator.clipboard?.writeText(text);
    });

    const toggleDrawer = () => {
      if (document.getElementById('reportsTab')?.classList.contains('active')) {
        const repLayout = document.getElementById('reportsMainLayout');
        const isSplit = repLayout?.classList.contains('mode-split') || repLayout?.classList.contains('mode-plsql');
        setReportViewMode(isSplit ? 'preview' : 'split');
        return;
      }
      const hidden = bottomDrawer.classList.toggle('hidden');
      toggleDrawerBtn?.classList.toggle('active', !hidden);
      setTimeout(zoomFit, 100);
    };
    const toggleFocus = () => {
      const enterFocus = !leftPane.classList.contains('collapsed') || !rightPane.classList.contains('collapsed');
      leftPane.classList.toggle('collapsed', enterFocus);
      rightPane.classList.toggle('collapsed', enterFocus);
      toggleLeftBtn?.classList.toggle('active', !enterFocus);
      toggleRightBtn?.classList.toggle('active', !enterFocus);
      focusModeBtn?.classList.toggle('active', enterFocus);
      if (enterFocus) {
        switchTab('canvasTab');
      }
      setTimeout(zoomFit, 220);
    };

    toggleLeftBtn?.addEventListener('click', toggleLeft);
    closeLeftBtn?.addEventListener('click', toggleLeft);
    toggleRightBtn?.addEventListener('click', toggleRight);
    closeRightBtn?.addEventListener('click', toggleRight);
    toggleDrawerBtn?.addEventListener('click', toggleDrawer);
    closeDrawerBtn?.addEventListener('click', toggleDrawer);
    focusModeBtn?.addEventListener('click', toggleFocus);

    // Bottom Drawer Tabs
    document.querySelectorAll('.drawer-tab').forEach(btn => {
      btn.addEventListener('click', () => {
        const targetId = btn.getAttribute('data-drawer');
        openBottomDrawer(targetId);
      });
    });

    document.getElementById('openFullCodeTabBtn')?.addEventListener('click', () => {
      switchTab('codeTab');
    });

    // Modals: Open File, DB Config, Floating Props, Floating Code
    const openModal = (id) => document.getElementById(id)?.showModal();
    document.getElementById('openFileModalBtn')?.addEventListener('click', () => {
      openModal('openFileModal');
      loadWorkspaceFilesBrowser();
    });
    document.getElementById('browseReportBtn')?.addEventListener('click', () => {
      openModal('openFileModal');
      loadWorkspaceFilesBrowser('REPORT');
    });
    document.getElementById('dbConfigBtn')?.addEventListener('click', () => openModal('dbConfigModal'));

    document.querySelectorAll('#browseCategoryPills .cat-pill').forEach(pill => {
      pill.addEventListener('click', () => {
        document.querySelectorAll('#browseCategoryPills .cat-pill').forEach(p => p.classList.remove('active'));
        pill.classList.add('active');
        filterWorkspaceFilesBrowser();
      });
    });

    document.getElementById('browseFilterInput')?.addEventListener('input', () => {
      filterWorkspaceFilesBrowser();
    });

    document.querySelectorAll('[data-attach-menu]').forEach(btn => {
      btn.addEventListener('click', () => {
        const menuMod = btn.getAttribute('data-attach-menu');
        document.getElementById('openFileModal')?.close();
        attachMenuToCurrentForm(menuMod);
      });
    });

    const menuModuleSelect = document.getElementById('menuModuleSelect');
    menuModuleSelect?.addEventListener('change', () => {
      const val = menuModuleSelect.value;
      if (val === 'BROWSE_MMB') {
        document.getElementById('menuFileInput')?.click();
        menuModuleSelect.value = currentPayload?.moduleIndex?.menuModule || 'DEFAULT';
        return;
      }
      attachMenuToCurrentForm(val);
    });

    const openFloatingProps = () => {
      const modalContent = document.getElementById('modalPropertyInspector');
      const modalTarget = document.getElementById('modalInspectorTarget');
      if (modalContent && propertyInspector) modalContent.innerHTML = propertyInspector.innerHTML;
      if (modalTarget && inspectorTargetName) modalTarget.textContent = inspectorTargetName.textContent;
      openModal('floatingPropsModal');
    };
    document.getElementById('popoutPropsBtn')?.addEventListener('click', openFloatingProps);
    document.getElementById('detachPropsBtn')?.addEventListener('click', openFloatingProps);

    const openFloatingCode = () => {
      const modalCode = document.getElementById('modalPlsqlDisplay');
      const modalTitle = document.getElementById('modalCodeTitle');
      if (modalCode && plsqlCodeDisplay) modalCode.innerHTML = plsqlCodeDisplay.innerHTML;
      if (modalTitle && codeTitle) modalTitle.textContent = codeTitle.textContent;
      openModal('floatingCodeModal');
    };
    document.getElementById('popoutCodeTopBtn')?.addEventListener('click', openFloatingCode);
    document.getElementById('popoutCodeModalBtn')?.addEventListener('click', openFloatingCode);
    document.getElementById('modalFromStudioBtn')?.addEventListener('click', openFloatingCode);

    document.getElementById('copyPlsqlBtn')?.addEventListener('click', () => {
      const text = plsqlCodeDisplay?.innerText || '';
      navigator.clipboard?.writeText(text);
    });

    // Close modal buttons & backdrop click
    document.querySelectorAll('[data-close-modal]').forEach(btn => {
      btn.addEventListener('click', () => {
        const id = btn.getAttribute('data-close-modal');
        document.getElementById(id)?.close();
      });
    });
    document.querySelectorAll('dialog.workbench-modal').forEach(dlg => {
      dlg.addEventListener('click', (e) => {
        if (e.target === dlg) dlg.close();
      });
    });

    // Update DB status label when backend or username changes
    const updateDbPill = () => {
      const label = document.getElementById('dbStatusLabel');
      if (!label || !dbBackendSelect) return;
      const user = dbUserInput?.value.trim() || 'Logon required';
      const map = {
        'oracle-thin': `${user}@FREEPDB1`,
        'oracle': `${user}@23ai`,
        'sqlite': 'SQLite Local',
        'mock': 'Mock Engine'
      };
      label.textContent = map[dbBackendSelect.value] || dbBackendSelect.value;
    };
    dbBackendSelect?.addEventListener('change', updateDbPill);
    dbUserInput?.addEventListener('input', updateDbPill);
    updateDbPill();

    document.getElementById('logonConnectRunBtn')?.addEventListener('click', () => {
      updateDbPill();
      startRuntimeSession();
    });

    [dbUserInput, dbPassInput].forEach(input => input?.addEventListener('keydown', event => {
      if (event.key === 'Enter') {
        event.preventDefault();
        document.getElementById('logonConnectRunBtn').click();
      }
    }));

    // Delegated handler for all in-window Oracle Forms Menu & Toolbar commands
    oracleWindowShell?.addEventListener('click', (e) => {
      const cmdEl = e.target.closest('[data-ora-cmd]');
      if (!cmdEl) return;
      const cmd = cmdEl.getAttribute('data-ora-cmd');
      if (!cmd) return;
      e.stopPropagation();
      switch (cmd) {
        case 'logon':
          openModal('dbConfigModal');
          break;
        case 'run':
          startRuntimeSession();
          break;
        case 'commit':
          if (runtime.active) runtimeSend({ kind: 'commit' });
          else showBanner('Start a live form session (▶ Run) before committing changes.');
          break;
        case 'clearForm':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'CLEAR_FORM' });
          else { preview.reset(); renderCanvasSvg(); }
          break;
        case 'print':
          window.print();
          break;
        case 'exit':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'EXIT_FORM' });
          else stopRuntimeSession();
          break;
        case 'clearRecord':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'CLEAR_RECORD' });
          else { preview.reset(); renderCanvasSvg(); }
          break;
        case 'toggleEditLayout':
          editLayoutToggleBtn?.click();
          break;
        case 'enterQuery':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'ENTER_QUERY' });
          else if (oracleStatusMsg) oracleStatusMsg.textContent = 'Enter a query criteria in the form fields, then press Execute Query (Ctrl+F11).';
          break;
        case 'executeQuery':
          executeBlockQuery();
          break;
        case 'cancelQuery':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'EXIT_FORM' });
          break;
        case 'prevBlock':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'PREVIOUS_BLOCK' });
          break;
        case 'nextBlock':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'NEXT_BLOCK' });
          break;
        case 'clearBlock':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'CLEAR_BLOCK' });
          else { preview.reset(); renderCanvasSvg(); }
          break;
        case 'firstRecord':
          navigateRecord('first');
          break;
        case 'prevRecord':
          navigateRecord('prev');
          break;
        case 'nextRecord':
          navigateRecord('next');
          break;
        case 'lastRecord':
          navigateRecord('last');
          break;
        case 'createRecord':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'CREATE_RECORD' });
          break;
        case 'deleteRecord':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'DELETE_RECORD' });
          break;
        case 'prevItem':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'PREVIOUS_ITEM' });
          break;
        case 'nextItem':
          if (runtime.active) runtimeSend({ kind: 'key', name: 'NEXT_ITEM' });
          break;
        case 'toggleNav':
          toggleLeft();
          break;
        case 'toggleProps':
          toggleRight();
          break;
        case 'floatProps':
          openFloatingProps();
          break;
        case 'toggleDrawer':
          toggleDrawer();
          break;
        case 'floatCode':
          openFloatingCode();
          break;
        case 'showTrace':
          openBottomDrawer('drawerTrace');
          break;
        case 'toggleFocus':
          toggleFocus();
          break;
        case 'about':
          showPreviewDialog(
            'Oracle Forms Workbench (Rust Native)',
            'Pure-Rust Oracle Forms 12c/14c Designer, PL/SQL Interpreter & Live Database Runtime connected to Oracle 23ai Free (:1521).'
          );
          break;
      }
    });

    // Tree Expand / Collapse All & Category Pills
    document.getElementById('expandAllTreeBtn')?.addEventListener('click', () => {
      navigatorTree?.querySelectorAll('.tree-children').forEach(c => c.classList.remove('collapsed'));
      navigatorTree?.querySelectorAll('.tree-toggle').forEach(t => { if (t.textContent) t.textContent = '▼'; });
    });
    document.getElementById('collapseAllTreeBtn')?.addEventListener('click', () => {
      navigatorTree?.querySelectorAll('.tree-children').forEach(c => c.classList.add('collapsed'));
      navigatorTree?.querySelectorAll('.tree-toggle').forEach(t => { if (t.textContent) t.textContent = '▶'; });
    });

    document.querySelectorAll('#treeCategoryPills .cat-pill').forEach(pill => {
      pill.addEventListener('click', () => {
        document.querySelectorAll('#treeCategoryPills .cat-pill').forEach(p => p.classList.remove('active'));
        pill.classList.add('active');
        const cat = pill.getAttribute('data-cat');
        navigatorTree?.querySelectorAll(':scope > .tree-node').forEach(groupNode => {
          const title = (groupNode.querySelector('.tree-label')?.textContent || '').toUpperCase();
          if (cat === 'ALL') {
            groupNode.style.display = '';
          } else if (cat === 'BLOCKS') {
            groupNode.style.display = title.includes('BLOCK') ? '' : 'none';
          } else if (cat === 'TRIGGERS') {
            groupNode.style.display = title.includes('TRIGGER') ? '' : 'none';
          } else if (cat === 'CANVASES') {
            groupNode.style.display = (title.includes('CANVAS') || title.includes('WINDOW')) ? '' : 'none';
          } else if (cat === 'UNITS') {
            groupNode.style.display = (title.includes('PROGRAM') || title.includes('LIBRARY') || title.includes('ALERT')) ? '' : 'none';
          }
        });
      });
    });

    // Property Filter Input
    const propFilterInput = document.getElementById('propFilterInput');
    propFilterInput?.addEventListener('input', () => {
      const q = propFilterInput.value.trim().toLowerCase();
      propertyInspector?.querySelectorAll('.prop-table tr').forEach(row => {
        const text = row.textContent.toLowerCase();
        row.style.display = (!q || text.includes(q)) ? '' : 'none';
      });
    });

    // PL/SQL Studio List Filter
    const codeListFilter = document.getElementById('codeListFilterInput');
    codeListFilter?.addEventListener('input', () => {
      const q = codeListFilter.value.trim().toLowerCase();
      document.querySelectorAll('#codeUnitsList .code-unit-item').forEach(item => {
        item.style.display = (!q || item.textContent.toLowerCase().includes(q)) ? '' : 'none';
      });
    });

    // Global IDE Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
      const tag = (e.target.tagName || '').toUpperCase();
      const inInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
      if (e.key === 'F4') {
        e.preventDefault();
        openFloatingProps();
      } else if (e.key === 'F11' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        openFloatingCode();
      } else if (!inInput && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (e.key === '[') { e.preventDefault(); toggleLeft(); }
        else if (e.key === ']') { e.preventDefault(); toggleRight(); }
        else if (e.key === '`') { e.preventDefault(); toggleDrawer(); }
      }
    });
  }

  // Controls setup
  function setupControls() {
    const previewToggle = document.getElementById('previewToggle');
    previewEnabled = !!previewToggle?.checked;
    document.getElementById('previewResetBtn').disabled = !previewEnabled;
    document.getElementById('previewAlertSelect').disabled = !previewEnabled;
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
      if (previewSource) loadCode(previewSource.name, previewSource.file, 'PREVIEW ACTION', previewSource.module, true);
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
    document.querySelector('.oracle-action-toolbar')?.addEventListener('click', (event) => {
      const button = event.target.closest('button');
      if (!button) return;
      if (button.hasAttribute('data-ora-cmd') || button.id) {
        return;
      }
      if (previewEnabled && button) showPreviewDialog(button.title, 'Toolbar action not executed in this local preview.');
    });

    // Live form session
    runFormBtn?.addEventListener('click', startRuntimeSession);
    stopFormBtn?.addEventListener('click', stopRuntimeSession);
    runtimeCommitBtn?.addEventListener('click', () => runtimeSend({ kind: 'commit' }));
    runtimeAlertDialog?.querySelectorAll('button[data-button]').forEach(btn => {
      btn.addEventListener('click', async () => {
        runtimeAlertDialog.close();
        try {
          applySessionState(await runtime.answerAlert(Number(btn.dataset.button)));
        } catch (err) {
          showBanner(`Alert answer failed: ${err.message}`);
        }
      });
    });
    // Escape must not leave the server trigger waiting forever: treat it as the default button.
    runtimeAlertDialog?.addEventListener('cancel', (e) => {
      e.preventDefault();
      runtimeAlertDialog.querySelector(`button[data-button="${runtimeAlertDialog.dataset.defaultButton || 1}"]`)?.click();
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
        editLayoutToggleBtn.textContent = layoutEditMode ? '✔️ Done' : '✏️ Layout';
        editLayoutToggleBtn.classList.toggle('btn-primary', layoutEditMode);
        editLayoutToggleBtn.classList.toggle('btn-secondary', !layoutEditMode);
        document.getElementById('layoutFormatRibbon')?.classList.toggle('hidden', !layoutEditMode);
        document.getElementById('canvasToolPalette')?.classList.toggle('hidden', !layoutEditMode);
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
          await syncModuleToServer();
          const res = await fetch('/api/forms/export-xml', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ moduleName: currentPayload.moduleIndex.key.name })
          });
          const data = await res.json();
          if (!res.ok) throw new Error(data.error || 'Export XML failed');
          const blob = new Blob([data.xml], { type: 'application/xml' });
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `${currentPayload.moduleIndex.key.name}_fmb.xml`;
          document.body.appendChild(a);
          a.click();
          document.body.removeChild(a);
          URL.revokeObjectURL(url);
          if (data.mode === 'summary') {
            showBanner(`Exported a SUMMARY of ${currentPayload.moduleIndex.key.name} (regenerated from the parsed model; unmodelled properties are omitted). Open the form from its Forms2XML file for a lossless export.`);
          } else {
            showBanner(`Exported ${currentPayload.moduleIndex.key.name}_fmb.xml: the original XML with only your edits applied.`);
            setTimeout(hideBanner, 4000);
          }
        } catch (err) {
          showBanner("Failed to export XML: " + err.message);
        }
      });
    }

    const convertArtifactXmlBtn = document.getElementById('convertArtifactXmlBtn');
    if (convertArtifactXmlBtn) {
      convertArtifactXmlBtn.addEventListener('click', async () => {
        if (!currentPayload || !currentPayload.moduleIndex || !currentPayload.moduleIndex.key) {
          showBanner('Open a form, menu, library, or report first.');
          return;
        }
        const moduleName = currentPayload.moduleIndex.key.name;
        const originalText = convertArtifactXmlBtn.textContent;
        convertArtifactXmlBtn.disabled = true;
        convertArtifactXmlBtn.textContent = 'Converting via Oracle...';
        showBanner(`Converting ${moduleName} via Oracle 14c container (saving XML/PLD next to the source file)...`);
        try {
          const res = await fetch('/api/artifacts/convert-to-xml', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ moduleName })
          });
          const data = await res.json();
          if (!res.ok) {
            showBanner(`Oracle conversion failed: ${data.error || res.statusText}`);
            return;
          }
          await loadSampleList();
          if (data.payload) {
            setModulePayload(data.payload);
            switchTab('xmlTab');
            await loadFullXml(data.payload.summary?.moduleName || moduleName);
          } else if (data.outputFile) {
            await loadFullXml(moduleName);
            switchTab('xmlTab');
          } else {
            await loadFullXml(moduleName);
          }
          showBanner(`Converted ${moduleName} — saved to: ${data.outputPath || data.outputFile || 'XML'}`);
          setTimeout(hideBanner, 12000);
        } catch (err) {
          showBanner('Oracle conversion failed: ' + err.message);
        } finally {
          convertArtifactXmlBtn.disabled = false;
          convertArtifactXmlBtn.textContent = originalText;
        }
      });
    }

    const comparisonFiles = new Map();
    document.getElementById('diffFilesInput')?.addEventListener('change', async event => {
      for (const file of event.target.files) {
        try {
          const body = new FormData();
          body.append('file', file);
          const response = await fetch('/api/open-file', { method: 'POST', body });
          const payload = await response.json();
          if (!response.ok || !payload.moduleIndex) throw new Error(payload.error || 'Not a form module');
          const key = `upload:${comparisonFiles.size}`;
          comparisonFiles.set(key, payload.moduleIndex);
          const option = new Option(file.name, key, false, true);
          diffCompareSelect.add(option);
          document.getElementById('diffBaseSelect').add(new Option(file.name, key));
        } catch (error) { showBanner(`${file.name}: ${error.message}`); }
      }
      event.target.value = '';
    });
    if (runDiffBtn) {
      runDiffBtn.addEventListener('click', async () => {
        const baseline = document.getElementById('diffBaseSelect').value;
        if (!baseline && !currentPayload) return showBanner('Open a baseline form first.');
        const targets = Array.from(diffCompareSelect.selectedOptions, option => option.value);
        if (!targets.length) return showBanner('Select at least one comparison form.');
        const base = baseline ? comparisonFiles.get(baseline) : structuredClone(currentPayload.moduleIndex);
        diffContainer.replaceChildren();
        runDiffBtn.disabled = true;
        try {
          for (const target of targets) {
            const section = document.createElement('section');
            section.className = 'diff-comparison';
            diffContainer.append(section);
            try {
              const response = await fetch('/api/forms/diff', {
                method: 'POST', headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ base, baseModule: base ? undefined : baseline,
                  modified: comparisonFiles.get(target), modifiedModule: target, includeModels: true })
              });
              const result = await response.json();
              if (!response.ok) throw new Error(result.error || 'Diff failed');
              renderDiffReport(result.diff, section);
              section.insertAdjacentHTML('afterbegin', renderDiffVisuals(result.base, result.modified));
            } catch (error) { section.textContent = `${target}: ${error.message}`; }
          }
        } finally { runDiffBtn.disabled = false; }
      });
    }

    document.getElementById('saveFormBtn').addEventListener('click', async event => {
      if (!currentPayload) return showBanner('Open or create a form first.');
      event.target.disabled = true;
      try {
        await persistDesignerAutoLayout();
        await syncModuleToServer();
        const response = await checkedSave('/api/forms/save', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ moduleName: currentPayload.moduleIndex.key.name })
        });
        const result = await response.json();
        showBanner(`Saved form to ${result.path}.${result.mode === 'summary' ? ' Model summary: unparsed Oracle properties are omitted.' : ' Original XML with item edits preserved.'} Existing files were kept.`);
      } catch (error) { showBanner(`Save Form failed: ${error.message}`); }
      finally { event.target.disabled = false; }
    });
    let oraclePreviewGeneration = 0;
    let oraclePreviewModule = null;
    let oraclePreviewBusy = false;
    let oraclePreviewReady = false;

    function resetOraclePreview(moduleName) {
      if (oraclePreviewModule !== moduleName) {
        oraclePreviewGeneration++;
        oraclePreviewModule = moduleName;
        oraclePreviewBusy = false;
        oraclePreviewReady = false;
        document.getElementById('oracleCompileMode').value = 'connected';
        document.getElementById('oracleCompileCredentials').hidden = false;
        document.getElementById('oracleCompilePassword').value = '';
        document.getElementById('oraclePreviewLog').textContent = 'No launch output yet.';
        document.getElementById('oraclePreviewSummary').textContent = `No preview launched for ${moduleName}.`;
        document.getElementById('oraclePreviewDiagnostics').open = false;
      }
      document.getElementById('oraclePreviewTitle').textContent = `Preview ${moduleName} in Oracle`;
    }

    async function checkOracleReadiness() {
      const button = document.getElementById('checkOracleReadinessBtn');
      const launch = document.getElementById('launchOraclePreviewBtn');
      const status = document.getElementById('oracleReadinessStatus');
      const list = document.getElementById('oracleReadinessChecks');
      button.disabled = true;
      launch.disabled = true;
      oraclePreviewReady = false;
      const generation = oraclePreviewGeneration;
      status.textContent = 'Checking the server computer and local Oracle testbed…';
      list.replaceChildren();
      try {
        const response = await fetch('/api/forms/oracle-preview/readiness');
        if (!response.ok) throw new Error('Setup checks unavailable. Restart the Rust server with the current build.');
        const result = await response.json();
        if (generation !== oraclePreviewGeneration) return;
        for (const check of result.checks) {
          const item = document.createElement('li');
          item.textContent = `${check.ready ? '✓' : check.required ? 'Missing / unavailable:' : 'Optional / unavailable:'} ${check.label}`;
          list.append(item);
        }
        status.textContent = result.ready ? 'Required dependencies are available.' : 'Complete the missing setup steps below, then check again.';
        document.getElementById('oracleReadinessNote').textContent = result.note;
        document.getElementById('oracleCheckDetails').open = !result.ready;
        oraclePreviewReady = result.ready;
        launch.disabled = !result.ready || oraclePreviewBusy;
      } catch (error) { if (generation === oraclePreviewGeneration) status.textContent = error.message; }
      finally { button.disabled = false; }
    }
    document.getElementById('checkOracleReadinessBtn').addEventListener('click', checkOracleReadiness);

    document.getElementById('oracleCompileMode').addEventListener('change', event => {
      const offline = event.target.value === 'offline';
      document.getElementById('oracleCompileCredentials').hidden = offline;
      document.getElementById('oracleCompilePassword').value = '';
    });
    document.getElementById('oracleReferenceDialog').addEventListener('close', () => {
      document.getElementById('oracleCompilePassword').value = '';
    });

    document.getElementById('launchOraclePreviewBtn').addEventListener('click', async event => {
      if (!currentPayload || oraclePreviewBusy) return;
      const sourceModule = currentPayload.moduleIndex.key.name;
      const compile = {
        offline: document.getElementById('oracleCompileMode').value === 'offline',
        username: (document.getElementById('oracleCompileUser').value || '').trim(),
        password: document.getElementById('oracleCompilePassword').value || '',
      };
      if (!compile.offline && (!compile.username || !compile.password)) {
        const missing = !compile.username ? 'username' : 'password';
        document.getElementById('oraclePreviewSummary').textContent = `Enter the database ${missing} for connected compilation. No preview was started.`;
        document.getElementById(missing === 'username' ? 'oracleCompileUser' : 'oracleCompilePassword').focus();
        return;
      }
      const generation = oraclePreviewGeneration;
      const isCurrent = () => generation === oraclePreviewGeneration && currentPayload?.moduleIndex.key.name === sourceModule;
      const button = event.target;
      const log = document.getElementById('oraclePreviewLog');
      const summary = document.getElementById('oraclePreviewSummary');
      oraclePreviewBusy = true;
      button.disabled = true;
      try {
        summary.textContent = `Saving ${sourceModule}…`;
        log.textContent = 'Saving XML and starting the Oracle reference toolchain…';
        const positioned = await persistDesignerAutoLayout();
        if (!isCurrent()) return;
        if (positioned) log.textContent += `\nPersisted designer geometry and captions for ${positioned} items.`;
        await syncModuleToServer();
        if (!isCurrent()) return;
        const response = await checkedSave('/api/forms/oracle-preview', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ moduleName: sourceModule, offline: compile.offline, username: compile.offline ? null : compile.username, password: compile.offline ? null : compile.password })
        });
        compile.password = '';
        if (isCurrent()) document.getElementById('oracleCompilePassword').value = '';
        const result = await response.json();
        if (!isCurrent()) return;
        const heading = `Source form: ${sourceModule}\nSaved samples/${result.fileName} (${result.mode}).\n`;
        summary.textContent = `Preparing ${sourceModule} for Oracle…`;
        for (let poll = 0; poll < 300; poll++) {
          await new Promise(resolve => setTimeout(resolve, 2000));
          if (!isCurrent()) return;
          const response = await fetch(`/api/forms/oracle-preview/${encodeURIComponent(result.jobId)}`);
          const status = await response.json();
          if (!isCurrent()) return;
          if (!response.ok) throw new Error(status.error || 'Preview status unavailable');
          log.textContent = heading + status.log;
          if (!status.running) {
            summary.textContent = status.success ? `Oracle preview for ${sourceModule} has closed.`
              : status.log.includes('logon=no') && status.log.includes('must be declared')
                ? `Offline compilation cannot resolve database objects in ${sourceModule}. Select database-connected compilation and enter credentials.`
                : `Could not launch ${sourceModule}. Open diagnostics for details.`;
            document.getElementById('oraclePreviewDiagnostics').open = !status.success;
            return;
          }
          summary.textContent = status.log.includes('Forms Session ID') ? `${sourceModule} opened in Oracle. Continue in the desktop window.`
            : status.log.includes('[3/3]') ? `Opening ${sourceModule} in the Oracle desktop client…`
            : status.log.includes('[2/3]') ? `Compiling ${sourceModule}…` : `Converting ${sourceModule} to FMB…`;
          if (status.log.includes('[compatibility]')) summary.textContent += ' A compatibility copy is used; see diagnostics for changes.';
        }
        summary.textContent = `Monitoring ended for ${sourceModule}. The Oracle window can continue running.`;
      } catch (error) {
        if (isCurrent()) {
          summary.textContent = `Could not launch ${sourceModule}.`;
          log.textContent += `\n${error.message}`;
          document.getElementById('oraclePreviewDiagnostics').open = true;
        }
      } finally {
        if (isCurrent()) {
          oraclePreviewBusy = false;
          button.disabled = !oraclePreviewReady;
        }
      }
    });

    document.addEventListener('click', event => {
      document.querySelectorAll('.workspace-menu[open], .canvas-options[open]').forEach(menu => {
        if (!menu.contains(event.target)) menu.open = false;
      });
    });
    document.addEventListener('keydown', event => {
      if (event.key === 'Escape') document.querySelectorAll('.workspace-menu[open], .canvas-options[open]').forEach(menu => { menu.open = false; });
    });
    const referenceForm = document.getElementById('oracleReferenceForm');
    function updateOracleReference() {
      const name = referenceForm.value.trim();
      const valid = /^[a-zA-Z0-9_][a-zA-Z0-9_.-]*\.fmx$/i.test(name);
      const link = document.getElementById('oracleReferenceLink');
      const command = document.getElementById('oracleReferenceCommand');
      if (!valid) {
        link.removeAttribute('href');
        command.textContent = 'Enter a plain compiled form filename ending in .fmx.';
        document.getElementById('copyOracleReferenceBtn').disabled = true;
        return;
      }
      link.href = `http://localhost:7001/forms/frmservlet?config=standaloneapp&form=${encodeURIComponent(name)}`;
      command.textContent = `python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py ${name} --interactive`;
      document.getElementById('copyOracleReferenceBtn').disabled = false;
    }
    document.getElementById('oracleReferenceBtn')?.addEventListener('click', () => {
      if (!currentPayload) return showBanner('Open a form first.');
      resetOraclePreview(currentPayload.moduleIndex.key.name);
      document.getElementById('oracleCompileUser').value = dbUserInput?.value.trim() || '';
      referenceForm.value = `${currentPayload.moduleIndex.key.name.toLowerCase()}.fmx`;
      updateOracleReference();
      document.getElementById('oracleReferenceDialog').showModal();
      document.querySelector('#oracleReferenceDialog .modal-body').scrollTop = 0;
      checkOracleReadiness();
    });
    referenceForm?.addEventListener('input', updateOracleReference);
    document.getElementById('copyOracleReferenceBtn')?.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(document.getElementById('oracleReferenceCommand').textContent);
        showBanner('FSAL command copied.');
      } catch (error) { showBanner(`Select and copy the displayed command: ${error.message}`); }
    });

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
      reportSelect.addEventListener('change', () => inspectReport());
      reportBackendSelect?.addEventListener('change', () => invalidateReport('Backend changed. Run to refresh the preview.'));
    }

    sampleSelect.addEventListener('change', () => {
      const selected = sampleSelect.value;
      if (selected === '__BROWSE__') {
        document.getElementById('openFileModal')?.showModal();
        loadWorkspaceFilesBrowser();
        return;
      }
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

    document.getElementById('retrySaveBtn').addEventListener('click', async event => {
      event.target.disabled = true;
      try { await syncModuleToServer(); hideBanner(); } catch (_) { /* save error remains visible */ }
      finally { event.target.disabled = false; }
    });
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

  // Sample listing & Workspace Artifact Browser
  let cachedWorkspaceFiles = [];

  async function loadSampleList() {
    try {
      const res = await fetch('/api/samples');
      if (res.ok) {
        const samples = await res.json();
        const forms = [];
        const menus = [];
        const reports = [];
        const libs = [];
        for (const s of samples) {
          const low = s.toLowerCase();
          if (low.endsWith('_report.xml') || low.endsWith('_rdf.xml') || low.endsWith('.rdf') || low.endsWith('.rpt')) {
            reports.push(s);
          } else if (low.endsWith('.mmb') || low.endsWith('_mmb.xml')) {
            menus.push(s);
          } else if (low.endsWith('.olb') || low.endsWith('_olb.xml') || low.endsWith('.pld') || low.endsWith('.pll')) {
            libs.push(s);
          } else {
            forms.push(s);
          }
        }
        let html = `<option value="__BROWSE__">📂 Browse Local / Workspace Files...</option>`;
        if (forms.length) {
          html += `<optgroup label="📋 Forms (.fmb / .xml)">${forms.map(s => `<option value="${s}">${s}</option>`).join('')}</optgroup>`;
        }
        if (menus.length) {
          html += `<optgroup label="🍔 Menu Modules (.mmb / .xml)">${menus.map(s => `<option value="${s}">${s}</option>`).join('')}</optgroup>`;
        }
        if (reports.length) {
          html += `<optgroup label="📊 Oracle Reports (.rdf / .xml)">${reports.map(s => `<option value="${s}">${s}</option>`).join('')}</optgroup>`;
        }
        if (libs.length) {
          html += `<optgroup label="📚 Libraries (.olb / .pld)">${libs.map(s => `<option value="${s}">${s}</option>`).join('')}</optgroup>`;
        }
        sampleSelect.innerHTML = html;
        if (diffCompareSelect) {
          diffCompareSelect.replaceChildren(...forms.map(name => new Option(name, name)));
          document.getElementById('diffBaseSelect').replaceChildren(new Option('Current form (including edits)', ''), ...forms.map(name => new Option(name, name)));
        }
        const urlSample = new URLSearchParams(window.location.search).get('sample');
        if (urlSample && samples.includes(urlSample)) {
          sampleSelect.value = urlSample;
        } else if (samples.includes('ofw_demo_employees_fmb.xml')) {
          sampleSelect.value = 'ofw_demo_employees_fmb.xml';
        }
      }
    } catch (e) {
      console.warn("Could not load sample list:", e);
    }
  }

  async function loadWorkspaceFilesBrowser(forceCategory = null) {
    const listEl = document.getElementById('workspaceFilesBrowserList');
    if (!listEl) return;
    if (forceCategory) {
      document.querySelectorAll('#browseCategoryPills .cat-pill').forEach(p => {
        p.classList.toggle('active', p.getAttribute('data-bcat') === forceCategory);
      });
    }
    try {
      const res = await fetch('/api/files/browse');
      if (res.ok) {
        const data = await res.json();
        cachedWorkspaceFiles = Array.isArray(data) ? data : (data.files || []);
        filterWorkspaceFilesBrowser();
      }
    } catch (e) {
      listEl.innerHTML = `<div class="empty-state">Error loading workspace files: ${escapeHtml(e.message)}</div>`;
    }
  }

  function filterWorkspaceFilesBrowser() {
    const listEl = document.getElementById('workspaceFilesBrowserList');
    if (!listEl) return;
    const activeCat = document.querySelector('#browseCategoryPills .cat-pill.active')?.getAttribute('data-bcat') || 'ALL';
    const query = (document.getElementById('browseFilterInput')?.value || '').trim().toLowerCase();

    const filtered = cachedWorkspaceFiles.filter(f => {
      const fname = (f.fileName || f.name || '').toLowerCase();
      const fpath = (f.relativePath || f.path || '').toLowerCase();
      if (activeCat !== 'ALL' && f.category !== activeCat) return false;
      if (query && !fname.includes(query) && !fpath.includes(query)) return false;
      return true;
    });

    if (filtered.length === 0) {
      listEl.innerHTML = `<div class="empty-state">No matching artifacts found in workspace. Use the Local Computer picker on the left to open any external file.</div>`;
      return;
    }

    const catLabels = {
      FORM: 'Form',
      MENU: 'Menu (.mmb)',
      OBJECT_LIB: 'Object Lib (.olb)',
      PLSQL_LIB: 'PL/SQL (.pld)',
      REPORT: 'Report (.rdf/.xml)'
    };

    listEl.innerHTML = filtered.map(f => {
      const fname = f.fileName || f.name || '';
      const fpath = f.relativePath || f.path || '';
      const kb = Math.max(1, Math.round((f.sizeBytes || 0) / 1024));
      const catLabel = catLabels[f.category] || f.category;
      let actionsHtml = '';
      if (f.category === 'MENU') {
        const baseMenu = fname.replace(/_mmb\.xml$/i, '').replace(/\.mmb$/i, '').toUpperCase();
        actionsHtml = `
          <button class="btn btn-xs btn-emerald" data-browser-attach-menu="${escapeHtml(baseMenu)}" title="Attach this Menu Module to the active Form">🍔 Attach to Form</button>
          <button class="btn btn-xs btn-secondary" data-browser-open="${escapeHtml(fname)}" title="Inspect Menu Module structure & PL/SQL">Inspect</button>
        `;
      } else if (f.category === 'REPORT') {
        actionsHtml = `<button class="btn btn-xs btn-primary" data-browser-open="${escapeHtml(fname)}">📊 Open in Reports</button>`;
      } else {
        actionsHtml = `<button class="btn btn-xs btn-primary" data-browser-open="${escapeHtml(fname)}">📂 Open</button>`;
      }
      return `
        <div class="browse-file-row">
          <div class="browse-file-info">
            <div class="browse-file-name">
              <span class="browse-cat-badge ${escapeHtml(f.category)}">${escapeHtml(catLabel)}</span>
              <span>${escapeHtml(fname)}</span>
            </div>
            <div class="browse-file-meta">
              <span>${escapeHtml(fpath)}</span>
              <span>·</span>
              <span>${kb} KB</span>
            </div>
          </div>
          <div style="display:flex; gap:5px; flex-shrink:0;">
            ${actionsHtml}
          </div>
        </div>
      `;
    }).join('');

    listEl.querySelectorAll('[data-browser-open]').forEach(btn => {
      btn.addEventListener('click', () => {
        const fname = btn.getAttribute('data-browser-open');
        document.getElementById('openFileModal')?.close();
        loadSample(fname);
      });
    });

    listEl.querySelectorAll('[data-browser-attach-menu]').forEach(btn => {
      btn.addEventListener('click', () => {
        const menuMod = btn.getAttribute('data-browser-attach-menu');
        document.getElementById('openFileModal')?.close();
        attachMenuToCurrentForm(menuMod);
      });
    });
  }

  async function attachMenuToCurrentForm(menuModuleName) {
    if (!currentPayload?.moduleIndex) {
      showBanner('Open a Form first before attaching a custom Menu Module (.mmb).');
      return;
    }
    const modName = currentPayload.moduleIndex.key?.name || currentPayload.summary?.moduleName;
    try {
      const res = await fetch(`/api/forms/module/${encodeURIComponent(modName)}/attach-menu`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ menuModule: menuModuleName })
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || 'Failed to attach menu module');
      setModulePayload(data);
      if (!menuModuleName || menuModuleName === 'DEFAULT') {
        showBanner(`Restored Standard Oracle Default Menu Bar on ${modName}.`);
      } else {
        showBanner(`🍔 Attached Custom Menu Module '${menuModuleName}.mmb' to ${modName} (${(data.moduleIndex.menus || []).length} menus loaded).`);
      }
      setTimeout(hideBanner, 3600);
    } catch (e) {
      showBanner(`Could not attach menu '${menuModuleName}': ${e.message}`);
    }
  }

  async function loadSample(sampleName) {
    try {
      hideBanner();
      const low = (sampleName || '').toLowerCase();
      if (low.endsWith('_report.xml') || low.endsWith('_rdf.xml') || low.endsWith('.rdf') || low.endsWith('.rpt')) {
        switchTab('reportsTab');
        if (reportSelect) {
          if (!Array.from(reportSelect.options).some(o => o.value === sampleName)) {
            reportSelect.add(new Option(sampleName, sampleName));
          }
          reportSelect.value = sampleName;
          executeReport();
        }
        return;
      }
      const res = await fetch(`/api/open-sample/${encodeURIComponent(sampleName)}`, { method: 'POST' });
      if (!res.ok) {
        const err = await res.json();
        showBanner(err.error || "Failed to load sample");
        return;
      }
      const payload = await res.json();
      if (payload.isReport) {
        switchTab('reportsTab');
        if (reportSelect && payload.reportName) {
          if (!Array.from(reportSelect.options).some(o => o.value === payload.reportName)) {
            reportSelect.add(new Option(`${payload.title || payload.reportName}`, payload.reportName));
          }
          reportSelect.value = payload.reportName;
          executeReport();
        }
        return;
      }
      if (sampleSelect) sampleSelect.value = sampleName;
      setModulePayload(payload);
    } catch (e) {
      showBanner("Network error while loading sample: " + e.message);
    }
  }

  async function uploadFile(file) {
    try {
      hideBanner();
      document.getElementById('openFileModal')?.close();
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
      document.getElementById('openFileModal')?.close();
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
    if (payload.isReport) {
      switchTab('reportsTab');
      if (reportSelect && payload.reportName) {
        if (!Array.from(reportSelect.options).some(o => o.value === payload.reportName)) {
          reportSelect.add(new Option(`${payload.title || payload.reportName}`, payload.reportName));
        }
        reportSelect.value = payload.reportName;
        executeReport();
      }
      return;
    }
    setModulePayload(payload);
  }

  function showBanner(message, isConversion = false) {
    bannerMessage.textContent = message;
    document.getElementById('retrySaveBtn')?.classList.toggle('hidden', !/save failed|Failed to save properties/.test(message));
    conversionBanner.classList.remove('hidden');
    bannerSampleBtn.style.display = isConversion ? 'inline-block' : 'none';
  }

  function hideBanner() {
    conversionBanner.classList.add('hidden');
  }

  // Set active module
  function setModulePayload(payload) {
    if (runtime.active) {
      runtime.stop();
      queryRecordsByBlock.clear();
      activeRecordIndexByBlock.clear();
      setRuntimeUi(null);
    }
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
    alerts.replaceChildren(new Option('🔔 Alert...', ''));
    for (const alert of payload.moduleIndex.alerts || []) alerts.add(new Option(alert.name, alert.name));
    const summary = payload.summary;
    const mod = payload.moduleIndex;

    moduleBadge.textContent = `${summary.moduleName} (${summary.moduleType})`;
    loadFullXml(summary.moduleName);
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
    renderCodeUnitsList(mod);
    renderPropertyInspector({ type: 'MODULE', module: mod, summary });
    renderCanvasSvg();
    setTimeout(zoomFit, 60);
  }

  function renderCodeUnitsList(mod) {
    const listEl = document.getElementById('codeUnitsList');
    if (!listEl) return;
    const triggers = mod.triggers || [];
    const units = mod.programUnits || [];
    if (triggers.length === 0 && units.length === 0) {
      listEl.innerHTML = '<div class="empty-state">No triggers or PL/SQL program units in this module.</div>';
      return;
    }
    let html = '';
    for (const t of triggers) {
      const scope = t.itemName ? `${t.blockName}.${t.itemName}` : (t.blockName || 'Form');
      const file = t.textRef?.file || '';
      html += `
        <div class="code-unit-item" data-code-name="${escapeHtml(t.name)}" data-code-file="${escapeHtml(file)}" data-code-scope="TRIGGER (${escapeHtml(scope)})">
          <div>
            <strong>⚡ ${escapeHtml(t.name)}</strong>
            <span class="code-unit-meta">${escapeHtml(scope)}</span>
          </div>
          <span class="tree-badge">${t.lineCount || 0}L</span>
        </div>
      `;
    }
    for (const pu of units) {
      const file = pu.textRef?.file || '';
      html += `
        <div class="code-unit-item" data-code-name="${escapeHtml(pu.name)}" data-code-file="${escapeHtml(file)}" data-code-scope="PROGRAM UNIT (${escapeHtml(pu.unitType || 'PROC')})">
          <div>
            <strong>📦 ${escapeHtml(pu.name)}</strong>
            <span class="code-unit-meta">${escapeHtml(pu.unitType || 'Program Unit')}</span>
          </div>
          <span class="tree-badge">${pu.lineCount || 0}L</span>
        </div>
      `;
    }
    listEl.innerHTML = html;
    listEl.querySelectorAll('.code-unit-item').forEach(el => {
      el.addEventListener('click', () => {
        listEl.querySelectorAll('.code-unit-item').forEach(i => i.classList.remove('active'));
        el.classList.add('active');
        loadCode(el.dataset.codeName, el.dataset.codeFile, el.dataset.codeScope, null, false);
      });
    });
    // Auto-load the first trigger into the studio display so it's never blank
    const firstEl = listEl.querySelector('.code-unit-item');
    if (firstEl) {
      firstEl.classList.add('active');
      loadCode(firstEl.dataset.codeName, firstEl.dataset.codeFile, firstEl.dataset.codeScope, null, false);
    }
  }

  function showPreviewDialog(title, message, source = null) {
    previewSource = source;
    document.getElementById('previewDialogTitle').textContent = title;
    document.getElementById('previewDialogMessage').textContent = message;
    document.getElementById('previewSourceBtn').hidden = !source;
    document.getElementById('previewDialog').showModal();
  }

  function switchTabPage(tabName, canvasName = null) {
    if (canvasName && activeCanvas && canvasName !== activeCanvas.name) {
      overlayTabPage.set(canvasName, tabName);
      renderCanvasSvg();
      return;
    }
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

      const rawDs = unescapeXml(b.queryDataSourceName || '').trim();
      const blockSublabel = !rawDs
        ? '[Control]'
        : (/^(\(?\s*select\b|\s*select\b)/i.test(rawDs) || rawDs.includes('\n') || rawDs.length > 24
          ? '[Subquery]'
          : `[${rawDs}]`);

      return {
        name: b.name,
        type: 'BLOCK',
        obj: b,
        sublabel: blockSublabel,
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
      openBottomDrawer('drawerPlsql');
      loadCode(targetObj.trigger.name, targetObj.trigger.textRef.file, 'TRIGGER', null, false);
    } else if (type === 'PROGRAM_UNIT' && targetObj.programUnit.textRef) {
      openBottomDrawer('drawerPlsql');
      loadCode(targetObj.programUnit.name, targetObj.programUnit.textRef.file, 'PROGRAM_UNIT', null, false);
    } else if (type === 'MENU_ITEM' && targetObj && targetObj.item && targetObj.item.commandRef) {
      openBottomDrawer('drawerPlsql');
      loadCode(`Menu: ${targetObj.menu.name}.${targetObj.item.name}`, targetObj.item.commandRef.file, 'MENU_ITEM', menuSourceModule, false);
    } else if (type === 'ITEM') {
      // If item has trigger, preload first trigger into code viewer without forcing tab switch
      const itemTriggers = mod.triggers.filter(t => t.level === 'ITEM' && t.itemName === name);
      if (itemTriggers.length > 0 && itemTriggers[0].textRef) {
        loadCode(itemTriggers[0].name, itemTriggers[0].textRef.file, 'TRIGGER', null, false);
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
    const activeIdx = topRecordIndex(blockName);
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
    if (runtime.active) {
      return runtimeSend({ kind: 'executeQuery', block: targetBlockName || getActiveBlockName() });
    }
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

  async function navigateRecord(direction) {
    if (runtime.active) {
      if (direction === 'first') return runtimeSend({ kind: 'key', name: 'FIRST_RECORD' });
      if (direction === 'last') return runtimeSend({ kind: 'key', name: 'LAST_RECORD' });
      return runtimeSend({ kind: direction === 'next' ? 'nextRecord' : 'previousRecord' });
    }
    const blockName = getActiveBlockName();
    if (!blockName) return;
    let records = queryRecordsByBlock.get(blockName) || [];
    if (records.length === 0) {
      await executeBlockQuery(blockName);
      records = queryRecordsByBlock.get(blockName) || [];
      if (records.length === 0) return;
    }

    let curIdx = activeRecordIndexByBlock.get(blockName) || 0;
    if (direction === 'next' && curIdx < records.length - 1) {
      curIdx++;
    } else if (direction === 'prev' && curIdx > 0) {
      curIdx--;
    } else if (direction === 'first' && curIdx > 0) {
      curIdx = 0;
    } else if (direction === 'last' && curIdx < records.length - 1) {
      curIdx = records.length - 1;
    } else {
      return;
    }

    activeRecordIndexByBlock.set(blockName, curIdx);
    applyRecordsToBlock(blockName);
    updateRecordNavigator(blockName);
    renderCanvasSvg();
  }

  // ---------------------------------------------------------------------------
  // Live form session (server-side runtime)
  // ---------------------------------------------------------------------------

  function recordForRow(blockName, row) {
    return topRecordIndex(blockName) + (row || 0);
  }

  function setRuntimeUi(state) {
    const running = runtime.active;
    runFormBtn?.classList.toggle('hidden', running);
    stopFormBtn?.classList.toggle('hidden', !running);
    runtimeCommitBtn?.classList.toggle('hidden', !running);
    sessionBadge?.classList.toggle('hidden', !running);
    sessionTracePanel?.classList.toggle('hidden', !running);
    if (dbBackendSelect) dbBackendSelect.disabled = running;
    if (allowWritesToggle) allowWritesToggle.disabled = running;
    if (running && state && sessionBadge) {
      const ro = state.readOnly;
      sessionBadge.textContent = `RUNNING ${state.moduleName} · ${state.backend}${ro ? ' · read-only' : ' · writes ON'}`;
      sessionBadge.classList.toggle('read-only', !!ro);
    }
    if (runtimeCommitBtn) runtimeCommitBtn.disabled = !!state?.readOnly;
  }

  async function startRuntimeSession() {
    if (!currentPayload) return showBanner('Open a form before running it.');
    if (runtime.active) {
      const message = 'This form is already running. Close Logon to continue, or stop the form before starting a new connection.';
      document.getElementById('dbLogonStatus').textContent = message;
      showBanner(message);
      return;
    }
    const backend = dbBackendSelect ? dbBackendSelect.value : '';
    const logonDialog = document.getElementById('dbConfigModal');
    const logonStatus = document.getElementById('dbLogonStatus');
    const connectButton = document.getElementById('logonConnectRunBtn');
    dbUserInput.removeAttribute('aria-invalid');
    dbPassInput.removeAttribute('aria-invalid');
    const credentialError = FormsRuntime.credentialError({
      backend, username: dbUserInput?.value, password: dbPassInput?.value,
      useServerCredentials: document.getElementById('useServerDbCredentials')?.checked
    });
    if (credentialError) {
      logonStatus.textContent = credentialError.message;
      if (!logonDialog.open) logonDialog.showModal();
      const input = credentialError.field === 'username' ? dbUserInput : dbPassInput;
      input.setAttribute('aria-invalid', 'true');
      input.focus();
      return;
    }
    logonStatus.textContent = 'Connecting and starting the form…';
    connectButton.disabled = true;
    connectButton.textContent = 'Connecting…';
    runFormBtn.disabled = true;
    runFormBtn.textContent = '⏳ Starting...';
    try {
      await syncModuleToServer();
      const state = await runtime.start({
        moduleName: currentPayload.summary.moduleName || currentPayload.moduleIndex.key.name,
        backend: backend === 'mock' ? 'mock' : backend,
        username: dbUserInput?.value.trim(),
        password: dbPassInput?.value,
        allowWrites: allowWritesToggle?.checked,
      });
      if (dbPassInput) dbPassInput.value = '';
      logonStatus.textContent = '';
      if (logonDialog.open) logonDialog.close();
      sessionTrace.textContent = '';
      const previewToggle = document.getElementById('previewToggle');
      if (!previewToggle.checked) {
        previewToggle.checked = true;
        previewToggle.dispatchEvent(new Event('change'));
      }
      hideBanner();
      applySessionState(state);
      if (state.missingLibraries?.length) {
        showBanner(`Attached libraries not found (calls into them will fail): ${state.missingLibraries.join(', ')}`);
      }
    } catch (err) {
      logonStatus.textContent = `Could not connect: ${err.message}`;
      if (['oracle', 'oracle-thin'].includes(backend) && !logonDialog.open) logonDialog.showModal();
      showBanner(`Could not start the form: ${err.message}`);
    } finally {
      connectButton.disabled = false;
      connectButton.textContent = 'Connect & Run';
      runFormBtn.disabled = false;
      runFormBtn.textContent = '▶ Run';
      if (!runtime.active) setRuntimeUi(null);
    }
  }

  async function stopRuntimeSession() {
    await runtime.stop();
    queryRecordsByBlock.clear();
    activeRecordIndexByBlock.clear();
    preview.reset();
    setRuntimeUi(null);
    renderCanvasSvg();
  }

  async function runtimeSend(event) {
    try {
      applySessionState(await runtime.send(event));
    } catch (err) {
      showBanner(err.message);
      if (/not found|expired|ended/i.test(err.message)) stopRuntimeSession();
    }
  }

  let lastSessionState = null;
  function applySessionState(state) {
    if (!state) return;
    if (!state.alert) lastSessionState = state;
    if (state.alert) {
      const a = state.alert;
      document.getElementById('runtimeAlertTitle').textContent = a.alertName;
      document.getElementById('runtimeAlertMessage').textContent = a.message;
      [a.button1, a.button2, a.button3].forEach((label, i) => {
        const btn = document.getElementById(`runtimeAlertBtn${i + 1}`);
        btn.textContent = label || '';
        btn.hidden = !label;
      });
      runtimeAlertDialog.dataset.defaultButton = a.defaultButton || 1;
      runtimeAlertDialog.showModal();
      return;
    }

    for (const b of state.blocks || []) {
      queryRecordsByBlock.set(b.name, (b.records || []).map(r => r.values || {}));
      activeRecordIndexByBlock.set(b.name, b.currentRecord || 0);
    }
    preview.reset();
    for (const b of state.blocks || []) applyRecordsToBlock(b.name);
    if (state.cursorBlock) updateRecordNavigator(state.cursorBlock);

    if (state.trace?.length) {
      sessionTrace.textContent += state.trace.join('\n') + '\n';
      sessionTrace.scrollTop = sessionTrace.scrollHeight;
    }
    const lastMessage = state.messages?.length ? state.messages[state.messages.length - 1] : null;
    if (oracleStatusMsg) {
      oracleStatusMsg.textContent = lastMessage
        || `Record: ${state.cursorRecord} · Block: ${state.cursorBlock || '-'} · ${state.formStatus}`;
    }
    if (state.error) showBanner(state.error);
    setRuntimeUi(state);
    renderCanvasSvg();
    if (state.exitRequested) {
      showBanner('The form called EXIT_FORM. Session ended.');
      stopRuntimeSession();
    }
  }

  function canvasCoordinateScale(coordinate, canvas, override) {
    if (override) return override;
    const unit = (coordinate.realUnit || '').toLowerCase();
    const system = (coordinate.coordinateSystem || '').toLowerCase();
    if (system === 'real' || canvas.width > 2000 || canvas.height > 2000) {
      if (unit.includes('centimeter') || unit === 'cm') return 37.79527559 / 1000;
      if (unit.includes('inch') || unit === 'in') return 96 / 1000;
      if (canvas.width > 3000 || canvas.height > 3000) return 800 / Math.max(canvas.width, 1000);
    }
    return 1;
  }

  function resolveAutoLayout(visualItems) {
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

    return resolvedPositions;
  }

  async function persistDesignerAutoLayout() {
    if (!currentPayload?.moduleIndex) return 0;
    const module = currentPayload.moduleIndex;
    const targets = new Map();
    for (const canvas of module.canvases || []) {
      const pages = canvas.tabPages?.length ? canvas.tabPages.map(page => page.name) : [null];
      for (const page of pages) {
        const visualItems = module.blocks.flatMap(block => block.items
          .filter(item => item.canvasName === canvas.name && (!canvas.tabPages?.length || !item.tabPageName || item.tabPageName === page))
          .map(item => ({ block, item })));
        const resolved = resolveAutoLayout(visualItems);
        for (const entry of visualItems) {
          const key = `${entry.block.name}.${entry.item.name}`;
          const captionMissing = (entry.item.itemType || '').toLowerCase().includes('button') && !entry.item.label;
          if ((resolved.has(key) || captionMissing) && !targets.has(key)) {
            targets.set(key, { ...entry, position: resolved.get(key), unitScale: canvasCoordinateScale(module.coordinate || {}, canvas) });
          }
        }
      }
    }
    for (const { block, item, position, unitScale } of targets.values()) {
      const button = (item.itemType || '').toLowerCase().includes('button');
      const height = position ? Math.max(1, Math.round((button && position.h < 26 ? 33 : position.h) / unitScale)) : (item.height || 33);
      const x = position ? Math.round(position.x / unitScale) : (item.xPosition || 0);
      const y = position ? Math.round(position.y / unitScale) : (item.yPosition || 0);
      const width = position ? Math.max(1, Math.round(position.w / unitScale)) : (item.width || 140);
      const inferredCaption = button && !item.label;
      await checkedSave(`/api/forms/module/${encodeURIComponent(module.key.name)}/item`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ blockName: block.name, itemName: item.name,
          x, y, width, height,
          label: button ? (item.label || item.prompt || item.name) : undefined,
          prompt: inferredCaption ? '' : undefined,
          promptAttachmentEdge: item.promptAttachmentEdge || 'Start',
          promptAttachmentOffset: item.promptAttachmentOffset ?? 5 })
      });
      Object.assign(item, { xPosition: x, yPosition: y, width, height });
      if (inferredCaption) {
        item.label = item.prompt || item.name;
        item.prompt = '';
      }
    }
    if (targets.size && currentPayload.moduleIndex === module) renderCanvasSvg();
    return targets.size;
  }

  // Visual Canvas 2D SVG Rendering
  /**
   * Renders one canvas as SVG markup. `opts.overlay` is set for stacked / tab canvases
   * drawn on top of the window's content canvas (their own origin is 0,0).
   */
  function buildCanvasLayer(canvas, activeTabPage, opts = {}) {
    const mod = currentPayload.moduleIndex;
    const coordScale = canvasCoordinateScale(mod.coordinate || {}, canvas, opts.coordScale);
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

    const resolvedPositions = resolveAutoLayout(visualItems);
    const needsAutoFlow = resolvedPositions.size > 0;

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
    const height = Math.max(baseHeight, maxContentY + 30);


    let svgHtml = '';

    // Canvas background (overlays only paint their own viewport)
    const bgCol = canvas.backColor || '#d4d0c8';
    if (!opts.overlay) {
      svgHtml += `<rect x="0" y="0" width="${width}" height="${height}" fill="${bgCol}"/>`;
    }

    // Layer 0.5: Tab Canvas Notebook Body & Interactive Tab Headers
    if (canvas.tabPages && canvas.tabPages.length > 0) {
      const tabStartX = opts.overlay ? 0 : ((canvas.xPosition && canvas.xPosition > 0) ? scale(canvas.xPosition) : 15);
      const tabStartY = opts.overlay ? 0 : ((canvas.yPosition && canvas.yPosition > 0) ? scale(canvas.yPosition) : 15);
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
            <g class="svg-tab-header active" style="cursor: pointer;" onclick="window.switchSvgTabPage('${escapeHtml(tp.name)}', '${escapeHtml(canvas.name)}')">
              <path d="M ${curTabX} ${tabBodyY + 1} L ${curTabX} ${tabStartY + 3} Q ${curTabX} ${tabStartY - 2} ${curTabX + 4} ${tabStartY - 2} L ${curTabX + tabW - 4} ${tabStartY - 2} Q ${curTabX + tabW} ${tabStartY - 2} ${curTabX + tabW} ${tabStartY + 3} L ${curTabX + tabW} ${tabBodyY + 1} Z" fill="#f5f4ef" stroke="#808080" stroke-width="1.5"/>
              <path d="M ${curTabX + 1.5} ${tabBodyY} L ${curTabX + 1.5} ${tabStartY + 4} Q ${curTabX + 1.5} ${tabStartY - 0.5} ${curTabX + 4} ${tabStartY - 0.5} L ${curTabX + tabW - 3} ${tabStartY - 0.5}" fill="none" stroke="#ffffff" stroke-width="1"/>
              <!-- Open bottom line to seamlessly merge into active notebook body -->
              <line x1="${curTabX + 1}" y1="${tabBodyY}" x2="${curTabX + tabW - 1}" y2="${tabBodyY}" stroke="#f5f4ef" stroke-width="2.5"/>
              <text x="${curTabX + tabW / 2}" y="${tabStartY + 16}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="#000">${escapeHtml(tabLabel)}</text>
            </g>
          `;
        } else {
          svgHtml += `
            <g class="svg-tab-header inactive" style="cursor: pointer;" onclick="window.switchSvgTabPage('${escapeHtml(tp.name)}', '${escapeHtml(canvas.name)}')">
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
        const isSelGraphic = selectedObject && selectedObject.type === 'GRAPHIC' && selectedObject.graphic.name === g.name;
        svgHtml += `
          <g class="svg-graphic frame ${isSelGraphic ? 'svg-item-selected' : ''}" style="cursor:pointer;" onclick="window.selectSvgGraphic('${escapeHtml(g.name)}')">
            <rect x="${gx}" y="${gy}" width="${gw}" height="${gh}" fill="${g.backColor || 'none'}" stroke="${isSelGraphic ? '#38bdf8' : '#808080'}" stroke-width="1.5" rx="2"/>
            <rect x="${gx + 1}" y="${gy + 1}" width="${gw - 2}" height="${gh - 2}" fill="none" stroke="#ffffff" stroke-width="1" rx="2"/>
            ${title ? `
              <rect x="${gx + 10}" y="${gy - 7}" width="${title.length * 7 + 12}" height="14" fill="${bgCol}"/>
              <text x="${gx + 15}" y="${gy + 4}" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="#000">${escapeHtml(title)}</text>
            ` : ''}
          </g>
        `;
      } else if (g.graphicsType === 'Text') {
        svgHtml += `<text x="${gx}" y="${gy + 12}" style="cursor:pointer;" onclick="window.selectSvgGraphic('${escapeHtml(g.name)}')" font-family="Segoe UI, Tahoma, sans-serif" font-size="11" font-weight="bold" fill="${g.foreColor || '#000'}">${escapeHtml(g.graphicsText || g.name)}</text>`;
      } else {
        svgHtml += `<rect x="${gx}" y="${gy}" width="${gw}" height="${gh}" fill="${g.backColor || 'none'}" stroke="#808080" stroke-width="1"/>`;
      }
    }

    // Inferred block outlines are designer guides, not graphics in the Oracle source.
    if (needsAutoFlow && layoutEditMode) {
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

      // Draw Multi-Record Grid rows. A single-record item of a multi-record block
      // (Number of Items Displayed = 1) shows the current record.
      const rowCount = isMultiRow ? recordsToDisplay : 1;
      const rowOffset = (!isMultiRow && recordsToDisplay > 1)
        ? Math.max(0, (activeRecordIndexByBlock.get(block.name) || 0) - topRecordIndex(block.name)) : 0;

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
            data-preview-block="${escapeHtml(block.name)}" data-preview-item="${escapeHtml(item.name)}" data-preview-row="${r + rowOffset}"></foreignObject>`;
        } else if (isHeaderLabel) {
          // Clean borderless display header (e.g. MIN_NAME)
          svgHtml += `
            <text x="${x + w / 2}" y="${rowY + Math.min(15, h - 2)}" text-anchor="middle" font-family="Segoe UI, Tahoma, sans-serif" font-size="12" font-weight="bold" fill="#111">${escapeHtml(item.label || item.prompt || item.name)}</text>
          `;
        } else if (isButton) {
          // Push button
          const btnLabel = item.label || item.prompt || item.name;
          const btnBg = normalizeOracleColor(item.backColor) || '#ece9d8';
          const btnFg = normalizeOracleColor(item.foreColor) || '#111111';
          const btnFont = item.fontName || 'Segoe UI, Tahoma, sans-serif';
          const btnSize = item.fontSize || 11;
          const btnWeight = item.fontWeight || '600';
          svgHtml += `
            <rect x="${x}" y="${rowY}" width="${w}" height="${h}" rx="3" fill="${escapeHtml(btnBg)}" stroke="#716f64" stroke-width="1" ${isHidden ? 'stroke-dasharray="4,2"' : ''}/>
            <line x1="${x + 1}" y1="${rowY + 1}" x2="${x + w - 1}" y2="${rowY + 1}" stroke="#ffffff" stroke-width="1.5"/>
            <line x1="${x + 1}" y1="${rowY + 1}" x2="${x + 1}" y2="${rowY + h - 1}" stroke="#ffffff" stroke-width="1.5"/>
            <line x1="${x + 1}" y1="${rowY + h - 1}" x2="${x + w - 1}" y2="${rowY + h - 1}" stroke="#716f64" stroke-width="1.5"/>
            <line x1="${x + w - 1}" y1="${rowY + 1}" x2="${x + w - 1}" y2="${rowY + h - 1}" stroke="#716f64" stroke-width="1.5"/>
            <text x="${x + w / 2}" y="${rowY + h / 2 + 4}" text-anchor="middle" font-family="${escapeHtml(btnFont)}" font-size="${btnSize}" font-weight="${escapeHtml(btnWeight)}" fill="${escapeHtml(btnFg)}">${escapeHtml(btnLabel)}</text>
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
            const recIdx = isMultiRow ? (topRecordIndex(block.name) + r) : activeIdx;
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
            const recIdx = isMultiRow ? (topRecordIndex(block.name) + r) : activeIdx;
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

    return { svgHtml, width, height, coordScale };
  }

  /** Index of the first record shown in a multi-record block (keeps the current record visible). */
  function topRecordIndex(blockName) {
    const block = currentPayload?.moduleIndex.blocks.find(b => b.name === blockName);
    const shown = Math.max(1, block?.recordsDisplayed || 1);
    const current = activeRecordIndexByBlock.get(blockName) || 0;
    return Math.max(0, current - shown + 1);
  }

  /** Stacked and tab canvases shown in the same window as a content canvas. */
  function overlayCanvasesFor(canvas) {
    const mod = currentPayload.moduleIndex;
    if ((canvas.canvasType || 'Content') !== 'Content' || !canvas.windowName) return [];
    return mod.canvases.filter(c => c !== canvas && c.windowName === canvas.windowName
      && ['Tab', 'Stacked'].includes(c.canvasType || ''));
  }

  function renderCanvasSvg() {
    if (!currentPayload || !activeCanvas) {
      formsSvg.innerHTML = '<text x="20" y="30" fill="#666">No canvas selected</text>';
      canvasStats.textContent = 'Dimensions: -';
      return;
    }
    const mod = currentPayload.moduleIndex;
    const canvas = activeCanvas;

    const base = buildCanvasLayer(canvas, activeTabPage);
    let svgHtml = base.svgHtml;
    let width = base.width;
    let height = base.height;

    // A window shows its content canvas with stacked / tab canvases on top of it.
    for (const ov of overlayCanvasesFor(canvas)) {
      if (!overlayTabPage.has(ov.name) && ov.tabPages?.length) overlayTabPage.set(ov.name, ov.tabPages[0].name);
      const layer = buildCanvasLayer(ov, overlayTabPage.get(ov.name) || null, { overlay: true, coordScale: base.coordScale });
      const ox = Math.round((ov.viewportXPosition || 0) * base.coordScale);
      const oy = Math.round((ov.viewportYPosition || 0) * base.coordScale);
      const vw = Math.round((ov.viewportWidth || ov.width || 0) * base.coordScale) || layer.width;
      const vh = Math.round((ov.viewportHeight || ov.height || 0) * base.coordScale) || layer.height;
      svgHtml += `<g class="svg-overlay-canvas" data-canvas="${escapeHtml(ov.name)}" transform="translate(${ox},${oy})">
        <rect x="0" y="0" width="${vw}" height="${vh}" fill="${ov.backColor || '#d4d0c8'}"/>${layer.svgHtml}</g>`;
      width = Math.max(width, ox + vw + 10);
      height = Math.max(height, oy + vh + 10);
    }

    formsSvg.setAttribute('width', width);
    formsSvg.setAttribute('height', height);
    formsSvg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    const overlays = overlayCanvasesFor(canvas).map(c => c.name);
    canvasStats.textContent = `Canvas: ${canvas.name}${overlays.length ? ' + ' + overlays.join(', ') : ''} | Dimensions: ${width} x ${height} px | Window: ${canvas.windowName || 'Default'}`;
    if (oracleWindowTitle) {
      const winObj = (mod.windows || []).find(w => w.name === canvas.windowName) || (mod.windows || [])[0];
      const realTitle = unescapeXml(winObj?.title || mod.title || '');
      const winName = canvas.windowName || winObj?.name || mod.key.name;
      oracleWindowTitle.textContent = realTitle
        ? `${realTitle}  —  [${winName} · ${canvas.name}]`
        : `${winName} - [${canvas.name}]`;
    }
    if (oracleStatusMsg && !selectedObject && !runtime.active) {
      oracleStatusMsg.textContent = `Record: 1/1  Canvas: ${canvas.name}`;
    }

    formsSvg.innerHTML = svgHtml;
    if (previewEnabled) {
      formsSvg.querySelectorAll('foreignObject[data-preview-block]').forEach(object => {
        const block = mod.blocks.find(b => b.name === object.dataset.previewBlock);
        const item = block.items.find(i => i.name === object.dataset.previewItem);
        const row = Number(object.dataset.previewRow);
        const control = preview.control(block, item, row);
        // Highlight the current record of multi-record blocks (Forms' current-record VA).
        if ((block.recordsDisplayed || 1) > 1
          && topRecordIndex(block.name) + row === (activeRecordIndexByBlock.get(block.name) || 0)
          && (queryRecordsByBlock.get(block.name) || []).length > 0) {
          control.classList.add('current-record');
        }
        object.append(control);
      });
      if (runtime.active) {
        canvasStats.textContent += ' | LIVE: triggers and PL/SQL run on the server';
      } else {
        canvasStats.textContent += ' | READY: click any form button or ▶ Run to execute live';
        const activeBlk = getActiveBlockName();
        const recs = activeBlk ? (queryRecordsByBlock.get(activeBlk) || []) : [];
        if (recs.length === 0 && oracleStatusMsg) {
          oracleStatusMsg.textContent = 'Ready — click Search, Show all, < Previous / Next > on the form, or ▶ Run';
        }
      }
    }
    applyZoom();
  }

  window.selectSvgGraphic = function (graphicName) {
    if (!currentPayload) return;
    const g = (currentPayload.moduleIndex.graphics || []).find(gr => gr.name === graphicName);
    if (g) {
      selectedObject = { type: 'GRAPHIC', graphic: g };
      selectedItemStats.textContent = `Selected Graphic: ${g.name} (${g.graphicsType || 'Frame'})`;
      renderPropertyInspector(selectedObject);
      renderCanvasSvg();
    }
  };

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
        await checkedSave(`/api/forms/module/${encodeURIComponent(currentPayload.moduleIndex.key.name)}/item`, {
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
        showBanner(`Position changed in this browser; server save failed: ${err.message}`);
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
  let fullBlockRecordsBackup = new Map();

  function renderRuntimeMenuBar(mod) {
    const container = document.getElementById('oracleMenuItems');
    if (!container) return;

    const menuSelect = document.getElementById('menuModuleSelect');
    if (menuSelect) {
      const attachedName = (mod?.menuModule || '').toUpperCase().replace(/\.MMB$/i, '').replace(/\.MMX$/i, '');
      if (!mod || !mod.menus || mod.menus.length === 0 || !attachedName || attachedName === 'DEFAULT&SMARTBAR') {
        menuSelect.value = 'DEFAULT';
      } else {
        if (!Array.from(menuSelect.options).some(o => o.value.toUpperCase() === attachedName)) {
          menuSelect.add(new Option(`🍔 Menu: ${attachedName}.mmb`, attachedName), 0);
        }
        menuSelect.value = attachedName;
      }
    }

    if (!mod || !mod.menus || mod.menus.length === 0) {
      // Default standard Oracle Forms runtime menus with interactive commands
      container.innerHTML = `
        <div class="menu-item dropdown">
          <span>Action</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="logon"><span>🔑 Connect / Logon...</span><kbd>Alt+L</kbd></div>
            <div class="dropdown-item" data-ora-cmd="run"><span>▶ Run Form Session</span><kbd>Ctrl+R</kbd></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" data-ora-cmd="commit"><span>💾 Save / Commit</span><kbd>Ctrl+S</kbd></div>
            <div class="dropdown-item" data-ora-cmd="clearForm"><span>🧹 Clear All</span><kbd>F8</kbd></div>
            <div class="dropdown-item" data-ora-cmd="print"><span>🖨️ Print...</span><kbd>Ctrl+P</kbd></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" data-ora-cmd="exit"><span>🚪 Exit Form</span><kbd>Ctrl+Q</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Edit</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="clearRecord"><span>Clear Record</span><kbd>Shift+F4</kbd></div>
            <div class="dropdown-item" data-ora-cmd="toggleEditLayout"><span>✏️ Edit Canvas Layout</span></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Query</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="enterQuery"><span>❓ Enter Query</span><kbd>F7</kbd></div>
            <div class="dropdown-item" data-ora-cmd="executeQuery"><span>⚡ Execute Query</span><kbd>Ctrl+F11</kbd></div>
            <div class="dropdown-item" data-ora-cmd="cancelQuery"><span>🚫 Cancel Query</span><kbd>Ctrl+Q</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Block</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="prevBlock"><span>⏮️ Previous Block</span><kbd>Shift+PgUp</kbd></div>
            <div class="dropdown-item" data-ora-cmd="nextBlock"><span>⏭️ Next Block</span><kbd>Shift+PgDn</kbd></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" data-ora-cmd="clearBlock"><span>🧹 Clear Block</span><kbd>F5</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Record</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="firstRecord"><span>⏫ First Record</span></div>
            <div class="dropdown-item" data-ora-cmd="prevRecord"><span>🔼 Previous Record</span><kbd>Up</kbd></div>
            <div class="dropdown-item" data-ora-cmd="nextRecord"><span>🔽 Next Record</span><kbd>Down</kbd></div>
            <div class="dropdown-item" data-ora-cmd="lastRecord"><span>⏬ Last Record</span></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" data-ora-cmd="createRecord"><span>➕ Insert Record</span><kbd>F6</kbd></div>
            <div class="dropdown-item" data-ora-cmd="deleteRecord"><span>❌ Remove Record</span><kbd>Shift+F6</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Field</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="prevItem"><span>Previous Field</span><kbd>Shift+Tab</kbd></div>
            <div class="dropdown-item" data-ora-cmd="nextItem"><span>Next Field</span><kbd>Tab</kbd></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Window</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="toggleNav"><span>🗂️ Show/Hide Object Navigator</span><kbd>[</kbd></div>
            <div class="dropdown-item" data-ora-cmd="toggleProps"><span>🎛️ Show/Hide Property Palette</span><kbd>]</kbd></div>
            <div class="dropdown-item" data-ora-cmd="floatProps"><span>↗ Floating Property Sheet</span><kbd>F4</kbd></div>
            <div class="dropdown-item" data-ora-cmd="toggleDrawer"><span>📜 Show/Hide PL/SQL Split Drawer</span><kbd>\`</kbd></div>
            <div class="dropdown-item" data-ora-cmd="floatCode"><span>↗ Floating PL/SQL Editor</span><kbd>F11</kbd></div>
            <div class="dropdown-item" data-ora-cmd="showTrace"><span>📡 Show Session Trace Log</span></div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" data-ora-cmd="toggleFocus"><span>⛶ Full Runner View (Hide Sidebars)</span></div>
          </div>
        </div>
        <div class="menu-item dropdown">
          <span>Help</span>
          <div class="dropdown-menu">
            <div class="dropdown-item" data-ora-cmd="floatProps"><span>Display Error / Properties...</span><kbd>F4</kbd></div>
            <div class="dropdown-item" data-ora-cmd="about"><span>About Oracle Forms Workbench...</span></div>
          </div>
        </div>
      `;
      return;
    }

    // Dynamic Menu Module (.mmb) from Form
    const menuMap = new Map();
    for (const m of mod.menus) {
      menuMap.set(m.name.toUpperCase(), m);
    }

    const rootMenu = menuMap.get('MAIN_MENU') || menuMap.get('MAIN') || mod.menus[0];
    const mmbBadgeName = (mod.menuModule || menuSourceModule || rootMenu.name).replace(/\.mmb$/i, '');
    let menuHtml = '';

    for (const item of (rootMenu.items || [])) {
      const label = unescapeXml(item.label || item.name).replace(/&/g, '');
      const subName = item.subMenuName ? item.subMenuName.toUpperCase() : null;
      const subMenu = subName ? menuMap.get(subName) : null;

      if (subMenu && subMenu.items && subMenu.items.length > 0) {
        let subHtml = '';
        for (const sub of subMenu.items) {
          if (sub.menuItemType === 'Separator' || sub.name.toUpperCase().startsWith('SEPARATOR') || (!sub.label && !sub.commandRef)) {
            subHtml += `<div class="dropdown-divider"></div>`;
          } else {
            const subLabel = unescapeXml(sub.label || sub.name).replace(/&/g, '');
            const hasCode = !!sub.commandRef;
            const kbd = sub.keyboardAccelerator ? `<kbd>${escapeHtml(sub.keyboardAccelerator)}</kbd>` : (hasCode ? '<span class="item-tag" style="font-size:9px;color:#0066cc;">PL/SQL</span>' : '');
            subHtml += `
              <div class="dropdown-item" data-menu="${escapeHtml(subMenu.name)}" data-item="${escapeHtml(sub.name)}" title="${hasCode ? 'Click to run custom .mmb PL/SQL command & inspect source' : ''}">
                <span>${escapeHtml(subLabel)}</span>
                ${kbd}
              </div>
            `;
          }
        }
        menuHtml += `
          <div class="menu-item dropdown">
            <span>${escapeHtml(label)}</span>
            <div class="dropdown-menu">
              ${subHtml}
            </div>
          </div>
        `;
      } else {
        menuHtml += `
          <div class="menu-item" data-menu="${escapeHtml(rootMenu.name)}" data-item="${escapeHtml(item.name)}">
            <span>${escapeHtml(label)}</span>
          </div>
        `;
      }
    }

    // Always append Window menu and active .mmb badge so IDE panels & custom menu inspection are 1 click away
    menuHtml += `
      <div class="menu-item dropdown">
        <span>Window</span>
        <div class="dropdown-menu">
          <div class="dropdown-item" data-ora-cmd="toggleNav"><span>🗂️ Show/Hide Object Navigator</span><kbd>[</kbd></div>
          <div class="dropdown-item" data-ora-cmd="toggleProps"><span>🎛️ Show/Hide Property Palette</span><kbd>]</kbd></div>
          <div class="dropdown-item" data-ora-cmd="floatProps"><span>↗ Floating Property Sheet</span><kbd>F4</kbd></div>
          <div class="dropdown-item" data-ora-cmd="toggleDrawer"><span>📜 Show/Hide PL/SQL Split Drawer</span><kbd>\`</kbd></div>
          <div class="dropdown-item" data-ora-cmd="floatCode"><span>↗ Floating PL/SQL Editor</span><kbd>F11</kbd></div>
          <div class="dropdown-item" data-ora-cmd="showTrace"><span>📡 Show Session Trace Log</span></div>
          <div class="dropdown-divider"></div>
          <div class="dropdown-item" data-ora-cmd="toggleFocus"><span>⛶ Full Runner View (Hide Sidebars)</span></div>
        </div>
      </div>
      <span class="ora-db-tag" style="margin-left:6px; background:#eff6ff; color:#1d4ed8; border-color:#93c5fd;" title="Attached Oracle Forms Menu Module (.mmb)">
        🍔 ${escapeHtml(mmbBadgeName)}.mmb
      </span>
    `;

    container.innerHTML = menuHtml;

    // Attach click listeners to custom .mmb menu items to both inspect PL/SQL code AND execute the action live!
    container.querySelectorAll('.dropdown-item[data-menu], .menu-item[data-menu]').forEach(el => {
      el.addEventListener('click', async (e) => {
        e.stopPropagation();
        const menuName = el.getAttribute('data-menu');
        const itemName = el.getAttribute('data-item');
        if (!menuName || !itemName) return;
        const targetMenu = menuMap.get(menuName.toUpperCase());
        const targetItem = targetMenu?.items?.find(i => i.name === itemName);
        if (targetItem) {
          selectedObject = { type: 'MENU_ITEM', item: targetItem, menu: targetMenu };
          renderPropertyInspector(selectedObject);
          if (targetItem.commandRef) {
            openBottomDrawer('drawerPlsql');
            loadCode(`Menu (${mmbBadgeName}.mmb): ${targetMenu.name}.${targetItem.name}`, targetItem.commandRef.file, 'MENU_ITEM (.mmb)', menuSourceModule, false);
          }
          await executeCustomMenuItem(targetMenu, targetItem, mmbBadgeName);
        }
      });
    });
  }

  async function executeCustomMenuItem(targetMenu, targetItem, mmbBadgeName = 'OFW_EMP_MENU') {
    const itemUp = (targetItem.name || '').toUpperCase();
    const labelClean = unescapeXml(targetItem.label || targetItem.name).replace(/&/g, '');
    const activeBlock = getActiveBlockName() || 'EMP';

    const ensureBlockQueried = async (blockName) => {
      let recs = queryRecordsByBlock.get(blockName) || [];
      if (recs.length === 0) {
        await executeBlockQuery();
        recs = queryRecordsByBlock.get(blockName) || [];
      }
      if (recs.length > 0 && !fullBlockRecordsBackup.has(blockName)) {
        fullBlockRecordsBackup.set(blockName, recs.map(r => ({ ...r })));
      }
      return fullBlockRecordsBackup.get(blockName) || recs;
    };

    const filterByDept = async (deptName) => {
      const allRecs = await ensureBlockQueried('EMP');
      if (!deptName || deptName === 'ALL') {
        queryRecordsByBlock.set('EMP', allRecs.map(r => ({ ...r })));
        activeRecordIndexByBlock.set('EMP', 0);
        preview.reset();
        applyRecordsToBlock('EMP');
        updateRecordNavigator('EMP');
        renderCanvasSvg();
        if (oracleStatusMsg) oracleStatusMsg.textContent = `[${mmbBadgeName}.mmb] FILTER_BY_DEPARTMENT('ALL'): Showing all ${allRecs.length} employee records.`;
        return;
      }
      const filtered = allRecs.filter(r => String(r.DEPARTMENT || '').toLowerCase() === deptName.toLowerCase());
      queryRecordsByBlock.set('EMP', filtered.map(r => ({ ...r })));
      activeRecordIndexByBlock.set('EMP', 0);
      preview.reset();
      applyRecordsToBlock('EMP');
      updateRecordNavigator('EMP');
      renderCanvasSvg();
      if (oracleStatusMsg) {
        oracleStatusMsg.textContent = `[${mmbBadgeName}.mmb] FILTER_BY_DEPARTMENT('${deptName}'): Showing ${filtered.length} employee(s) in ${deptName}.`;
      }
    };

    switch (itemUp) {
      case 'SHOW_ALL':
      case 'DEPT_ALL':
        await filterByDept('ALL');
        break;
      case 'EXEC_SEARCH':
        await executeBlockQuery();
        break;
      case 'SAVE_DIR':
        if (runtime.active) await runtimeSend({ kind: 'commit' });
        else showBanner(`[${mmbBadgeName}.mmb] Directory changes committed.`);
        break;
      case 'CLEAR_DIR':
        if (runtime.active) await runtimeSend({ kind: 'key', name: 'CLEAR_FORM' });
        else { preview.reset(); renderCanvasSvg(); }
        break;
      case 'EXIT_APP':
        if (runtime.active) await runtimeSend({ kind: 'key', name: 'EXIT_FORM' });
        else stopRuntimeSession();
        break;
      case 'DEPT_ENG':
        await filterByDept('Engineering');
        break;
      case 'DEPT_FIN':
        await filterByDept('Finance');
        break;
      case 'DEPT_OPS':
        await filterByDept('Operations');
        break;
      case 'DEPT_SALES':
        await filterByDept('Sales');
        break;
      case 'DEPT_HR':
        await filterByDept('HR');
        break;
      case 'FIRST_EMP':
        await ensureBlockQueried(activeBlock);
        navigateRecord('first');
        break;
      case 'PREV_EMP':
        await ensureBlockQueried(activeBlock);
        navigateRecord('prev');
        break;
      case 'NEXT_EMP':
        await ensureBlockQueried(activeBlock);
        navigateRecord('next');
        break;
      case 'LAST_EMP':
        await ensureBlockQueried(activeBlock);
        navigateRecord('last');
        break;
      case 'MERIT_RAISE_5':
      case 'MERIT_RAISE_10': {
        await ensureBlockQueried('EMP');
        const pct = itemUp === 'MERIT_RAISE_10' ? 10 : 5;
        const recs = queryRecordsByBlock.get('EMP') || [];
        const idx = activeRecordIndexByBlock.get('EMP') || 0;
        if (recs[idx]) {
          const curSal = parseFloat(recs[idx].SALARY || '0') || 0;
          const newSal = (curSal * (1 + pct / 100)).toFixed(2);
          recs[idx].SALARY = newSal;
          applyRecordsToBlock('EMP');
          renderCanvasSvg();
          if (runtime.active) {
            await runtimeSend({ kind: 'setItem', block: 'EMP', item: 'SALARY', value: String(newSal), record: idx });
          }
          const msg = `⚡ [${mmbBadgeName}.mmb] APPLY_MERIT_RAISE(${pct}%): ${recs[idx].FULL_NAME || recs[idx].LAST_NAME || 'Employee'} salary updated from $${curSal.toFixed(2)} to $${newSal}`;
          if (oracleStatusMsg) oracleStatusMsg.textContent = msg;
          showBanner(msg);
          setTimeout(hideBanner, 3600);
        }
        break;
      }
      case 'PROMOTE_SENIOR': {
        await ensureBlockQueried('EMP');
        const recs = queryRecordsByBlock.get('EMP') || [];
        const idx = activeRecordIndexByBlock.get('EMP') || 0;
        if (recs[idx]) {
          const curTitle = String(recs[idx].JOB_TITLE || 'Specialist');
          const newTitle = curTitle.toUpperCase().startsWith('SENIOR') ? curTitle : `Senior ${curTitle}`;
          recs[idx].JOB_TITLE = newTitle;
          applyRecordsToBlock('EMP');
          renderCanvasSvg();
          if (runtime.active) {
            await runtimeSend({ kind: 'setItem', block: 'EMP', item: 'JOB_TITLE', value: newTitle, record: idx });
          }
          const msg = `🎖️ [${mmbBadgeName}.mmb] PROMOTE_EMPLOYEE: ${recs[idx].FULL_NAME || recs[idx].LAST_NAME || 'Employee'} promoted to '${newTitle}'`;
          if (oracleStatusMsg) oracleStatusMsg.textContent = msg;
          showBanner(msg);
          setTimeout(hideBanner, 3600);
        }
        break;
      }
      case 'RUN_EMP_DIRECTORY_RPT':
        loadSample('ofw_employee_directory_report.xml');
        break;
      case 'RUN_DEPT_SALARIES_RPT':
        loadSample('department_salaries_report.xml');
        break;
      default:
        if (oracleStatusMsg) {
          oracleStatusMsg.textContent = `[${mmbBadgeName}.mmb] Executed menu item ${targetMenu.name}.${targetItem.name} (${labelClean})`;
        }
        break;
    }
  }

  function normalizeHexColor(val, fallback = '#e8e4d8') {
    if (!val || typeof val !== 'string') return fallback;
    const v = val.trim();
    if (/^#[0-9a-fA-F]{6}$/.test(v)) return v;
    const m = v.match(/^r(\d+)g(\d+)b(\d+)$/i);
    if (m) {
      const r = Math.min(255, Math.round((parseInt(m[1], 10) / 100) * 255));
      const g = Math.min(255, Math.round((parseInt(m[2], 10) / 100) * 255));
      const b = Math.min(255, Math.round((parseInt(m[3], 10) / 100) * 255));
      return '#' + [r, g, b].map(x => x.toString(16).padStart(2, '0')).join('');
    }
    const named = {
      white: '#ffffff', black: '#000000', gray: '#d4d0c8', grey: '#d4d0c8',
      blue: '#2563eb', darkblue: '#1e3a8a', green: '#15803d', red: '#b91c1c',
      yellow: '#fef9c3', cyan: '#e0f2fe'
    };
    return named[v.toLowerCase()] || fallback;
  }

  async function checkedSave(url, options) {
    const response = await fetch(url, options);
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error || `Save failed (${response.status})`);
    }
    return response;
  }

  async function syncModuleToServer() {
    if (!currentPayload?.moduleIndex) return;
    const modName = currentPayload.moduleIndex.key?.name || currentPayload.summary?.moduleName;
    if (!modName) return;
    try {
      await checkedSave(`/api/forms/module/${encodeURIComponent(modName)}/sync`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(currentPayload.moduleIndex)
      });
    } catch (error) {
      showBanner(`Changes remain in this browser; server save failed: ${error.message}`);
      throw error;
    }
  }

  // Property Inspector Rendering (Modern Forms Builder Property Palette)
  function renderPropertyInspector(target) {
    if (!target) {
      propertyInspector.innerHTML = '<div class="empty-state">Select an object on the canvas or tree to inspect & edit properties.</div>';
      inspectorTargetName.textContent = 'None';
      return;
    }

    let title = '';
    let html = '';

    if (!layoutEditMode) {
      html += `
        <div style="padding: 6px 10px; background: rgba(37,99,235,0.12); border-bottom: 1px solid rgba(59,130,246,0.28); display: flex; align-items: center; justify-content: space-between; gap: 8px;">
          <span style="font-size: 11px; color: #93c5fd;">Enable <strong>✏️ Layout</strong> mode to edit colors, fonts, titles & geometry</span>
          <button class="btn btn-xs btn-primary" onclick="document.getElementById('editLayoutToggleBtn')?.click()">✏️ Edit Mode</button>
        </div>
      `;
    }

    if (target.type === 'ITEM') {
      const item = target.item;
      const block = target.block;
      title = `${block.name}.${item.name}`;
      const isBtn = (item.itemType || '').toLowerCase().includes('button');
      const defaultBg = isBtn ? '#e8e4d8' : '#ffffff';
      const hexBg = normalizeHexColor(item.backColor, defaultBg);
      const hexFg = normalizeHexColor(item.foreColor, '#111111');

      // Sync top formatting ribbon inputs if present
      const rBg = document.getElementById('fmtBackColor');
      if (rBg) rBg.value = hexBg;
      const rFg = document.getElementById('fmtForeColor');
      if (rFg) rFg.value = hexFg;
      const rFont = document.getElementById('fmtFontFamily');
      if (rFont && item.fontName) rFont.value = item.fontName;
      const rSize = document.getElementById('fmtFontSize');
      if (rSize && item.fontSize) rSize.value = String(item.fontSize);

      html += `
        <div class="prop-group">
          <div class="prop-group-header">General & Functional</div>
          <table class="prop-table">
            <tr><td class="prop-name">Name</td><td class="prop-val"><strong>${escapeHtml(item.name)}</strong></td></tr>
            <tr><td class="prop-name">Block</td><td class="prop-val">${escapeHtml(block.name)}</td></tr>
            <tr>
              <td class="prop-name">Item Type</td>
              <td class="prop-val">
                <select class="prop-input" id="propEditItemType">
                  ${['Text Item', 'Display Item', 'Push Button', 'Check Box', 'List Item', 'Radio Group'].map(t =>
                    `<option value="${t}" ${(item.itemType || 'Text Item') === t ? 'selected' : ''}>${t}</option>`
                  ).join('')}
                </select>
              </td>
            </tr>
            <tr>
              <td class="prop-name">Label / Caption</td>
              <td class="prop-val"><input type="text" class="prop-input" id="propEditLabel" value="${escapeHtml(item.label || '')}" placeholder="Button label..."></td>
            </tr>
            <tr>
              <td class="prop-name">Built-in Action</td>
              <td class="prop-val">
                <select class="prop-input" id="propEditActionType">
                  <option value="" ${!item.actionType ? 'selected' : ''}>(Trigger / Auto)</option>
                  <option value="PREVIOUS_RECORD" ${item.actionType === 'PREVIOUS_RECORD' ? 'selected' : ''}>◀ Previous Record</option>
                  <option value="NEXT_RECORD" ${item.actionType === 'NEXT_RECORD' ? 'selected' : ''}>▶ Next Record</option>
                  <option value="FIRST_RECORD" ${item.actionType === 'FIRST_RECORD' ? 'selected' : ''}>⏮ First Record</option>
                  <option value="LAST_RECORD" ${item.actionType === 'LAST_RECORD' ? 'selected' : ''}>⏭ Last Record</option>
                  <option value="EXECUTE_QUERY" ${item.actionType === 'EXECUTE_QUERY' ? 'selected' : ''}>⚡ Execute Query (Search)</option>
                  <option value="SHOW_ALL" ${item.actionType === 'SHOW_ALL' ? 'selected' : ''}>📋 Show All Records</option>
                  <option value="ENTER_QUERY" ${item.actionType === 'ENTER_QUERY' ? 'selected' : ''}>🔍 Enter Query Mode</option>
                  <option value="COMMIT_FORM" ${item.actionType === 'COMMIT_FORM' ? 'selected' : ''}>💾 Commit / Save Form</option>
                  <option value="CREATE_RECORD" ${item.actionType === 'CREATE_RECORD' ? 'selected' : ''}>➕ Insert Record</option>
                  <option value="DELETE_RECORD" ${item.actionType === 'DELETE_RECORD' ? 'selected' : ''}>❌ Delete Record</option>
                  <option value="CLEAR_FORM" ${item.actionType === 'CLEAR_FORM' ? 'selected' : ''}>🧹 Clear Form</option>
                </select>
              </td>
            </tr>
            <tr>
              <td class="prop-name">Enabled / Visible</td>
              <td class="prop-val" style="display:flex; gap:10px; align-items:center;">
                <label style="font-size:11px;display:flex;align-items:center;gap:4px;"><input type="checkbox" id="propEditEnabled" ${item.enabled !== false ? 'checked' : ''}> Enabled</label>
                <label style="font-size:11px;display:flex;align-items:center;gap:4px;"><input type="checkbox" id="propEditVisible" ${item.visible !== false ? 'checked' : ''}> Visible</label>
              </td>
            </tr>
          </table>
        </div>

        <div class="prop-group">
          <div class="prop-group-header">Visual Attributes (Color & Font)</div>
          <table class="prop-table">
            <tr>
              <td class="prop-name">${isBtn ? 'Button Color' : 'Fill / Back Color'}</td>
              <td class="prop-val">
                <div class="prop-color-row">
                  <input type="color" id="propEditBackColorPicker" value="${hexBg}">
                  <input type="text" class="prop-input" id="propEditBackColor" value="${escapeHtml(item.backColor || hexBg)}" placeholder="#e8e4d8">
                </div>
              </td>
            </tr>
            <tr>
              <td class="prop-name">Foreground / Text</td>
              <td class="prop-val">
                <div class="prop-color-row">
                  <input type="color" id="propEditForeColorPicker" value="${hexFg}">
                  <input type="text" class="prop-input" id="propEditForeColor" value="${escapeHtml(item.foreColor || hexFg)}" placeholder="#111111">
                </div>
              </td>
            </tr>
            <tr>
              <td class="prop-name">Font Name</td>
              <td class="prop-val">
                <select class="prop-input" id="propEditFontName">
                  ${['Tahoma, Arial, sans-serif', 'Arial, sans-serif', 'Verdana, sans-serif', 'Georgia, serif', "'JetBrains Mono', monospace"].map(f =>
                    `<option value="${f}" ${(item.fontName || '') === f ? 'selected' : ''}>${f.split(',')[0].replace(/'/g, '')}</option>`
                  ).join('')}
                </select>
              </td>
            </tr>
            <tr>
              <td class="prop-name">Font Size / Style</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <input type="number" class="prop-input" id="propEditFontSize" value="${item.fontSize || 11}" style="width:58px;">
                <select class="prop-input" id="propEditFontWeight">
                  <option value="normal" ${(item.fontWeight || 'normal') !== 'bold' ? 'selected' : ''}>Regular</option>
                  <option value="bold" ${item.fontWeight === 'bold' ? 'selected' : ''}>Bold</option>
                </select>
                <select class="prop-input" id="propEditJustification">
                  <option value="Start" ${(item.justification || 'Start') === 'Start' ? 'selected' : ''}>Left</option>
                  <option value="Center" ${item.justification === 'Center' ? 'selected' : ''}>Center</option>
                  <option value="End" ${item.justification === 'End' ? 'selected' : ''}>Right</option>
                </select>
              </td>
            </tr>
          </table>
        </div>

        <div class="prop-group">
          <div class="prop-group-header">Physical & Prompt Layout</div>
          <table class="prop-table">
            <tr>
              <td class="prop-name">X / Y Position</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <input type="number" class="prop-input" id="propEditX" value="${item.xPosition || 0}" title="X">
                <input type="number" class="prop-input" id="propEditY" value="${item.yPosition || 0}" title="Y">
              </td>
            </tr>
            <tr>
              <td class="prop-name">Width / Height</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <input type="number" class="prop-input" id="propEditW" value="${item.width ?? 100}" title="Width">
                <input type="number" class="prop-input" id="propEditH" value="${item.height ?? 20}" title="Height">
              </td>
            </tr>
            <tr><td class="prop-name">Prompt Text</td><td class="prop-val"><input type="text" class="prop-input" id="propEditPrompt" value="${escapeHtml(item.prompt || '')}"></td></tr>
            <tr>
              <td class="prop-name">Prompt Edge / Offset</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <select class="prop-input" id="propEditPromptEdge">
                  <option value="Start" ${(item.promptAttachmentEdge || 'Start') === 'Start' ? 'selected' : ''}>Start (Left)</option>
                  <option value="Top" ${item.promptAttachmentEdge === 'Top' ? 'selected' : ''}>Top (Above)</option>
                </select>
                <input type="number" class="prop-input" id="propEditPromptOffset" value="${item.promptAttachmentOffset ?? 6}" style="width:60px;">
              </td>
            </tr>
          </table>
        </div>

        <div class="prop-group">
          <div class="prop-group-header">Database & Validation</div>
          <table class="prop-table">
            <tr>
              <td class="prop-name">Database Item</td>
              <td class="prop-val" style="display:flex; gap:10px;">
                <label style="font-size:11px;display:flex;align-items:center;gap:4px;"><input type="checkbox" id="propEditDbItem" ${item.dml?.databaseItem !== false ? 'checked' : ''}> DB Item</label>
                <label style="font-size:11px;display:flex;align-items:center;gap:4px;"><input type="checkbox" id="propEditRequired" ${item.required === true ? 'checked' : ''}> Required</label>
              </td>
            </tr>
            <tr><td class="prop-name">Column Name</td><td class="prop-val"><input type="text" class="prop-input" id="propEditColumnName" value="${escapeHtml(item.columnName || item.name)}"></td></tr>
            <tr>
              <td class="prop-name">Data Type / Max Len</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <select class="prop-input" id="propEditDataType">
                  ${['Char', 'Varchar2', 'Number', 'Date', 'Datetime'].map(dt =>
                    `<option value="${dt}" ${(item.dataType || 'Char') === dt ? 'selected' : ''}>${dt}</option>`
                  ).join('')}
                </select>
                <input type="number" class="prop-input" id="propEditMaxLen" value="${item.dml?.maximumLength || 30}" style="width:65px;" title="Maximum Length">
              </td>
            </tr>
            <tr><td class="prop-name">Initial Value</td><td class="prop-val"><input type="text" class="prop-input" id="propEditInitialValue" value="${escapeHtml(item.dml?.initialValue || '')}" placeholder="(Optional)"></td></tr>
          </table>
          <div style="padding: 8px; display: flex; gap: 6px;">
            <button class="btn btn-sm btn-primary" id="propApplyBtn" style="flex: 1;">💾 Apply & Save Properties</button>
          </div>
        </div>
      `;

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

    } else if (target.type === 'GRAPHIC') {
      const g = target.graphic;
      const c = target.canvas;
      title = `Graphic: ${g.name}`;
      html += `
        <div class="prop-group">
          <div class="prop-group-header">Frame / Graphic Properties</div>
          <table class="prop-table">
            <tr><td class="prop-name">Name</td><td class="prop-val"><strong>${escapeHtml(g.name)}</strong></td></tr>
            <tr><td class="prop-name">Type</td><td class="prop-val">${escapeHtml(g.graphicsType || 'Frame')}</td></tr>
            <tr><td class="prop-name">Frame Title</td><td class="prop-val"><input type="text" class="prop-input" id="propGraphicFrameTitle" value="${escapeHtml(g.frameTitle || g.graphicsText || '')}" placeholder="Frame section title..."></td></tr>
            <tr>
              <td class="prop-name">X / Y Position</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <input type="number" class="prop-input" id="propGraphicX" value="${g.xPosition || 0}">
                <input type="number" class="prop-input" id="propGraphicY" value="${g.yPosition || 0}">
              </td>
            </tr>
            <tr>
              <td class="prop-name">Width / Height</td>
              <td class="prop-val" style="display:flex; gap:6px;">
                <input type="number" class="prop-input" id="propGraphicW" value="${g.width || 400}">
                <input type="number" class="prop-input" id="propGraphicH" value="${g.height || 180}">
              </td>
            </tr>
          </table>
          <div style="padding: 8px;">
            <button class="btn btn-sm btn-primary" id="propGraphicApplyBtn" style="width: 100%;">💾 Update Frame Title & Layout</button>
          </div>
        </div>
      `;
    } else if (target.type === 'BLOCK') {
      const b = target.block;
      title = `Block: ${b.name}`;

      html += `
        <div class="prop-group">
          <div class="prop-group-header">Data Block Properties</div>
          <table class="prop-table">
            <tr><td class="prop-name">Block Name</td><td class="prop-val"><strong>${escapeHtml(b.name)}</strong></td></tr>
            <tr><td class="prop-name">Query Data Source</td><td class="prop-val"><input type="text" class="prop-input" id="propBlockSource" value="${escapeHtml(b.queryDataSourceName || '')}" placeholder="Table or View Name"></td></tr>
            <tr><td class="prop-name">Records Displayed</td><td class="prop-val"><input type="number" class="prop-input" id="propBlockRecs" value="${b.recordsDisplayed || 1}" min="1" max="30"></td></tr>
            <tr><td class="prop-name">Record Distance</td><td class="prop-val"><input type="number" class="prop-input" id="propBlockDist" value="${b.distanceBetweenRecords || 0}" min="0" max="100"></td></tr>
            <tr><td class="prop-name">WHERE Clause</td><td class="prop-val"><input type="text" class="prop-input" id="propBlockWhere" value="${escapeHtml(b.dml?.whereClause || '')}" placeholder="Optional WHERE filter"></td></tr>
            <tr><td class="prop-name">ORDER BY Clause</td><td class="prop-val"><input type="text" class="prop-input" id="propBlockOrder" value="${escapeHtml(b.dml?.orderByClause || '')}" placeholder="e.g. EMPLOYEE_ID"></td></tr>
          </table>
          <div style="padding: 8px; display:flex; gap:6px;">
            <button class="btn btn-sm btn-primary" id="propBlockApplyBtn" style="flex:1;">💾 Save Block</button>
            <button class="btn btn-sm" onclick="document.getElementById('openWizardBtn')?.click()">🧙‍♂️ Layout Wizard</button>
          </div>
        </div>
      `;

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
      html += `
        <div class="prop-group">
          <div class="prop-group-header">Window Properties</div>
          <table class="prop-table">
            <tr><td class="prop-name">Window Name</td><td class="prop-val"><strong>${escapeHtml(w.name)}</strong></td></tr>
            <tr><td class="prop-name">Window Title</td><td class="prop-val"><input type="text" class="prop-input" id="propWindowTitle" value="${escapeHtml(w.title || '')}" placeholder="Form Window Title..."></td></tr>
            <tr><td class="prop-name">Width / Height</td><td class="prop-val" style="display:flex;gap:6px;">
              <input type="number" class="prop-input" id="propWindowW" value="${w.width || 650}">
              <input type="number" class="prop-input" id="propWindowH" value="${w.height || 450}">
            </td></tr>
          </table>
          <div style="padding: 8px;">
            <button class="btn btn-sm btn-primary" id="propWindowApplyBtn" style="width:100%;">💾 Update Window Title</button>
          </div>
        </div>
      `;
    } else if (target.type === 'MODULE') {
      const s = target.summary || {};
      const m = target.module || {};
      const primaryWin = (m.windows || [])[0];
      title = `Module: ${s.moduleName || m.key?.name || 'Form'}`;
      html += `
        <div class="prop-group">
          <div class="prop-group-header">Form Title & Header</div>
          <table class="prop-table">
            <tr><td class="prop-name">Module Name</td><td class="prop-val"><strong>${escapeHtml(s.moduleName || m.key?.name || '')}</strong></td></tr>
            <tr><td class="prop-name">Window Title</td><td class="prop-val"><input type="text" class="prop-input" id="propModuleWinTitle" value="${escapeHtml(primaryWin?.title || m.title || '')}" placeholder="Form Window Title..."></td></tr>
          </table>
          <div style="padding: 8px; display:flex; gap:6px;">
            <button class="btn btn-sm btn-primary" id="propModuleWinApplyBtn" style="flex:1;">💾 Update Form Title</button>
            <button class="btn btn-sm" onclick="document.getElementById('openWizardBtn')?.click()">🧙‍♂️ Block Wizard</button>
          </div>
        </div>
      `;
      html += renderPropGroup('Module Summary', [
        ['Module Type', s.moduleType || m.key?.type || 'FORM'],
        ['Forms Version', m.formsVersion || '12c / 14c'],
        ['Menu Module (.mmb)', m.menuModule || 'DEFAULT&SMARTBAR'],
        ['Coordinate System', `${m.coordinate?.coordinateSystem || 'Real'} (${m.coordinate?.realUnit || 'Point'})`],
        ['Blocks', (m.blocks || []).length],
        ['Canvases', (m.canvases || []).length],
        ['Windows', (m.windows || []).length],
        ['Menus', (m.menus || []).length],
        ['Triggers', s.triggerCount ?? (m.triggers || []).length],
        ['Program Units', s.programUnitCount ?? (m.programUnits || []).length]
      ]);
      const formTriggers = (m.triggers || []).filter(t => t.level === 'FORM');
      if (formTriggers.length > 0) {
        html += `
          <div class="prop-group">
            <div class="prop-group-header">Form-Level Triggers (${formTriggers.length})</div>
            <div class="trigger-pills">
              ${formTriggers.map(t => `<span class="trigger-pill" onclick="window.loadCodeFromPill('${escapeHtml(t.name)}', '${escapeHtml(t.textRef?.file || '')}')">⚡ ${escapeHtml(t.name)}</span>`).join('')}
            </div>
          </div>
        `;
      }
    } else if (target.type === 'MENU') {
      const m = target.menu || {};
      title = `Menu: ${m.name}`;
      html += renderPropGroup('Menu Definition (.mmb)', [
        ['Menu Name', m.name],
        ['Menu Items Count', (m.items || []).length],
        ['Items', (m.items || []).map(i => i.label || i.name).join(', ')]
      ]);
    } else if (target.type === 'MENU_ITEM') {
      const it = target.item || {};
      const m = target.menu || {};
      title = `Menu Item: ${m.name}.${it.name}`;
      html += renderPropGroup('Menu Item Properties (.mmb)', [
        ['Parent Menu', m.name || 'MAIN_MENU'],
        ['Item Name', it.name],
        ['Label', unescapeXml(it.label || it.name)],
        ['Menu Item Type', it.menuItemType || 'Plain'],
        ['Command Type', it.commandType || (it.subMenuName ? 'Menu' : 'PL/SQL')],
        ['Sub-Menu', it.subMenuName || '(None)'],
        ['Accelerator', it.keyboardAccelerator || '(None)'],
        ['Visible / Enabled', `${it.visible !== false ? 'Yes' : 'No'} / ${it.enabled !== false ? 'Yes' : 'No'}`],
        ['PL/SQL Source', it.commandRef?.file || '(None)']
      ]);
      if (it.commandRef?.file) {
        html += `
          <div style="padding: 8px; display:flex; gap:6px;">
            <button class="btn btn-sm btn-primary" style="flex:1;" onclick="window.loadCodeFromPill('Menu: ${escapeHtml(m.name)}.${escapeHtml(it.name)}', '${escapeHtml(it.commandRef.file)}')">📜 Inspect .mmb PL/SQL</button>
          </div>
        `;
      }
    } else {
      title = target.type;
      html = `<div class="empty-state">${target.type} properties inspected.</div>`;
    }

    inspectorTargetName.textContent = title;
    propertyInspector.innerHTML = html;

    const modalContent = document.getElementById('modalPropertyInspector');
    const modalTarget = document.getElementById('modalInspectorTarget');
    if (modalContent) modalContent.innerHTML = html;
    if (modalTarget) modalTarget.textContent = title;

    // Wire color pickers & Apply buttons for ITEM
    if (target.type === 'ITEM') {
      const bgPicker = document.getElementById('propEditBackColorPicker');
      const bgInput = document.getElementById('propEditBackColor');
      if (bgPicker && bgInput) {
        bgPicker.addEventListener('input', () => {
          bgInput.value = bgPicker.value;
          target.item.backColor = bgPicker.value;
          renderCanvasSvg();
        });
      }
      const fgPicker = document.getElementById('propEditForeColorPicker');
      const fgInput = document.getElementById('propEditForeColor');
      if (fgPicker && fgInput) {
        fgPicker.addEventListener('input', () => {
          fgInput.value = fgPicker.value;
          target.item.foreColor = fgPicker.value;
          renderCanvasSvg();
        });
      }

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
          const prompt = document.getElementById('propEditPrompt')?.value ?? null;
          const label = document.getElementById('propEditLabel')?.value ?? null;
          const itemType = document.getElementById('propEditItemType')?.value || item.itemType;
          const actionType = document.getElementById('propEditActionType')?.value || null;
          const backColor = document.getElementById('propEditBackColor')?.value || null;
          const foreColor = document.getElementById('propEditForeColor')?.value || null;
          const fontName = document.getElementById('propEditFontName')?.value || null;
          const fontSize = parseInt(document.getElementById('propEditFontSize')?.value, 10) || 11;
          const fontWeight = document.getElementById('propEditFontWeight')?.value || 'normal';
          const justification = document.getElementById('propEditJustification')?.value || 'Start';
          const enabled = document.getElementById('propEditEnabled')?.checked !== false;
          const visible = document.getElementById('propEditVisible')?.checked !== false;
          const promptEdge = document.getElementById('propEditPromptEdge')?.value || 'Start';
          const promptOffset = parseInt(document.getElementById('propEditPromptOffset')?.value, 10) || 6;
          const dbItem = document.getElementById('propEditDbItem')?.checked !== false;
          const required = document.getElementById('propEditRequired')?.checked === true;
          const columnName = document.getElementById('propEditColumnName')?.value || item.name;
          const dataType = document.getElementById('propEditDataType')?.value || 'Char';
          const maxLen = parseInt(document.getElementById('propEditMaxLen')?.value, 10) || 30;
          const initialValue = document.getElementById('propEditInitialValue')?.value || null;

          item.xPosition = x;
          item.yPosition = y;
          item.width = w;
          item.height = h;
          item.prompt = prompt;
          item.label = label;
          item.itemType = itemType;
          item.actionType = actionType;
          item.backColor = backColor;
          item.foreColor = foreColor;
          item.fontName = fontName;
          item.fontSize = fontSize;
          item.fontWeight = fontWeight;
          item.justification = justification;
          item.enabled = enabled;
          item.visible = visible;
          item.promptAttachmentEdge = promptEdge;
          item.promptAttachmentOffset = promptOffset;
          item.required = required;
          item.columnName = columnName;
          item.dataType = dataType;
          if (!item.dml) item.dml = {};
          item.dml.databaseItem = dbItem;
          item.dml.maximumLength = maxLen;
          item.dml.initialValue = initialValue;

          renderCanvasSvg();

          try {
            await checkedSave(`/api/forms/module/${encodeURIComponent(currentPayload.moduleIndex.key.name)}/item`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                blockName: block.name,
                itemName: item.name,
                x, y, width: w, height: h,
                prompt, label,
                backColor, foreColor,
                itemType, dataType, columnName,
                enabled, visible, required,
                promptAttachmentEdge: promptEdge,
                promptAttachmentOffset: promptOffset,
                initialValue,
                maximumLength: maxLen
              })
            });
            await syncModuleToServer();
            showBanner(`Saved properties for ${block.name}.${item.name}`);
            setTimeout(hideBanner, 2000);
          } catch (err) {
            showBanner('Failed to save properties: ' + err.message);
          }
        });
      }
    }

    // Wire GRAPHIC Frame Title & Geometry
    if (target.type === 'GRAPHIC') {
      const btn = document.getElementById('propGraphicApplyBtn');
      if (btn) {
        btn.addEventListener('click', async () => {
          const g = target.graphic;
          const ft = document.getElementById('propGraphicFrameTitle')?.value || '';
          if (g.graphicsType === 'Text') {
            g.graphicsText = ft;
          } else {
            g.frameTitle = ft;
          }
          g.xPosition = parseInt(document.getElementById('propGraphicX')?.value, 10) || 0;
          g.yPosition = parseInt(document.getElementById('propGraphicY')?.value, 10) || 0;
          g.width = parseInt(document.getElementById('propGraphicW')?.value, 10) || 400;
          g.height = parseInt(document.getElementById('propGraphicH')?.value, 10) || 180;
          renderCanvasSvg();
          await syncModuleToServer();
          showBanner(`Updated Frame '${g.name}'`);
          setTimeout(hideBanner, 2000);
        });
      }
    }

    // Wire WINDOW Title
    if (target.type === 'WINDOW') {
      const btn = document.getElementById('propWindowApplyBtn');
      if (btn) {
        btn.addEventListener('click', async () => {
          const w = target.window;
          w.title = document.getElementById('propWindowTitle')?.value || '';
          w.width = parseInt(document.getElementById('propWindowW')?.value, 10) || w.width;
          w.height = parseInt(document.getElementById('propWindowH')?.value, 10) || w.height;
          renderCanvasSvg();
          await syncModuleToServer();
          showBanner(`Updated Window Title to '${w.title}'`);
          setTimeout(hideBanner, 2000);
        });
      }
    }

    // Wire MODULE Window Title
    if (target.type === 'MODULE') {
      const btn = document.getElementById('propModuleWinApplyBtn');
      if (btn) {
        btn.addEventListener('click', async () => {
          const newTitle = document.getElementById('propModuleWinTitle')?.value || '';
          const m = currentPayload?.moduleIndex;
          if (m) {
            if (m.windows && m.windows.length > 0) {
              m.windows[0].title = newTitle;
            }
            m.title = newTitle;
            renderCanvasSvg();
            await syncModuleToServer();
            showBanner(`Updated Form Window Title to '${newTitle}'`);
            setTimeout(hideBanner, 2000);
          }
        });
      }
    }

    // Wire BLOCK Properties
    if (target.type === 'BLOCK') {
      const btn = document.getElementById('propBlockApplyBtn');
      if (btn) {
        btn.addEventListener('click', async () => {
          const b = target.block;
          b.queryDataSourceName = document.getElementById('propBlockSource')?.value || null;
          b.recordsDisplayed = parseInt(document.getElementById('propBlockRecs')?.value, 10) || 1;
          b.distanceBetweenRecords = parseInt(document.getElementById('propBlockDist')?.value, 10) || 0;
          if (!b.dml) b.dml = { databaseBlock: true, queryAllowed: true, insertAllowed: true, updateAllowed: true, deleteAllowed: true };
          b.dml.whereClause = document.getElementById('propBlockWhere')?.value || null;
          b.dml.orderByClause = document.getElementById('propBlockOrder')?.value || null;
          renderCanvasSvg();
          await syncModuleToServer();
          showBanner(`Updated Block '${b.name}'`);
          setTimeout(hideBanner, 2000);
        });
      }
    }
  }

  // Modern Forms Builder Studio: Left Tool Palette, Top Formatting Ribbon & Data Block / Layout Wizard
  function setupBuilderStudio() {
    const TABLE_SCHEMAS = {
      OFW_DEMO_EMPLOYEES: {
        blockName: 'EMP',
        frameTitle: 'Employee Directory',
        windowTitle: 'Employee Directory',
        columns: [
          { name: 'EMPLOYEE_ID', prompt: 'Employee #', type: 'Number', width: 76, pk: true },
          { name: 'FIRST_NAME', prompt: 'First name', type: 'Varchar2', width: 140 },
          { name: 'LAST_NAME', prompt: 'Last name', type: 'Varchar2', width: 140 },
          { name: 'JOB_TITLE', prompt: 'Job title', type: 'Varchar2', width: 170 },
          { name: 'DEPARTMENT_NAME', prompt: 'Department', type: 'Varchar2', width: 120 },
          { name: 'SALARY', prompt: 'Salary', type: 'Number', width: 96 },
          { name: 'HIRE_DATE', prompt: 'Hire date', type: 'Date', width: 110 },
          { name: 'EMAIL', prompt: 'Email', type: 'Varchar2', width: 170 },
          { name: 'STATUS', prompt: 'Status', type: 'Varchar2', width: 90 }
        ]
      },
      OFW_DEMO_SALES: {
        blockName: 'SALES',
        frameTitle: 'Sales Orders',
        windowTitle: 'Sales Orders',
        columns: [
          { name: 'ORDER_ID', prompt: 'Order #', type: 'Number', width: 76, pk: true },
          { name: 'CUSTOMER_NAME', prompt: 'Customer', type: 'Varchar2', width: 170 },
          { name: 'REGION', prompt: 'Region', type: 'Varchar2', width: 96 },
          { name: 'SALES_REP', prompt: 'Sales rep', type: 'Varchar2', width: 130 },
          { name: 'ORDER_STATUS', prompt: 'Status', type: 'Varchar2', width: 110 },
          { name: 'ORDER_DATE', prompt: 'Order date', type: 'Date', width: 110 },
          { name: 'SUBTOTAL', prompt: 'Subtotal', type: 'Number', width: 96 },
          { name: 'TAX_AMOUNT', prompt: 'Tax', type: 'Number', width: 88 },
          { name: 'ORDER_TOTAL', prompt: 'Order total', type: 'Number', width: 100 }
        ]
      },
      ORDERS: {
        blockName: 'ORDERS',
        frameTitle: 'Customer Orders',
        windowTitle: 'Order Management',
        columns: [
          { name: 'ORDER_ID', prompt: 'Order ID', type: 'Number', width: 80, pk: true },
          { name: 'CUSTOMER_NAME', prompt: 'Customer', type: 'Varchar2', width: 160 },
          { name: 'ORDER_DATE', prompt: 'Order Date', type: 'Date', width: 110 },
          { name: 'ORDER_STATUS', prompt: 'Status', type: 'Varchar2', width: 100 },
          { name: 'ORDER_TOTAL', prompt: 'Total ($)', type: 'Number', width: 100 }
        ]
      },
      ORDER_ITEMS: {
        blockName: 'ORDER_ITEMS',
        frameTitle: 'Order Line Items',
        windowTitle: 'Order Line Items',
        columns: [
          { name: 'ITEM_ID', prompt: 'Line #', type: 'Number', width: 65, pk: true },
          { name: 'PRODUCT_NAME', prompt: 'Product', type: 'Varchar2', width: 160 },
          { name: 'QUANTITY', prompt: 'Qty', type: 'Number', width: 65 },
          { name: 'UNIT_PRICE', prompt: 'Unit Price', type: 'Number', width: 90 }
        ]
      },
      CLAIMS: {
        blockName: 'CLAIMS',
        frameTitle: 'Insurance Claims Register',
        windowTitle: 'Claims Adjudication',
        columns: [
          { name: 'CLAIM_ID', prompt: 'Claim ID', type: 'Number', width: 80, pk: true },
          { name: 'POLICY_NUMBER', prompt: 'Policy #', type: 'Varchar2', width: 120 },
          { name: 'CLAIMANT_NAME', prompt: 'Claimant', type: 'Varchar2', width: 160 },
          { name: 'CLAIM_AMOUNT', prompt: 'Amount', type: 'Number', width: 100 },
          { name: 'CLAIM_STATUS', prompt: 'Status', type: 'Varchar2', width: 100 }
        ]
      },
      EMP: {
        blockName: 'EMP',
        frameTitle: 'Employees (EMP)',
        windowTitle: 'Employee Maintenance',
        columns: [
          { name: 'EMPNO', prompt: 'Emp No', type: 'Number', width: 75, pk: true },
          { name: 'ENAME', prompt: 'Name', type: 'Varchar2', width: 140 },
          { name: 'JOB', prompt: 'Job', type: 'Varchar2', width: 120 },
          { name: 'SAL', prompt: 'Salary', type: 'Number', width: 90 },
          { name: 'DEPTNO', prompt: 'Dept No', type: 'Number', width: 70 }
        ]
      },
      DEPT: {
        blockName: 'DEPT',
        frameTitle: 'Departments (DEPT)',
        windowTitle: 'Department Directory',
        columns: [
          { name: 'DEPTNO', prompt: 'Dept No', type: 'Number', width: 75, pk: true },
          { name: 'DNAME', prompt: 'Department', type: 'Varchar2', width: 150 },
          { name: 'LOC', prompt: 'Location', type: 'Varchar2', width: 140 }
        ]
      }
    };

    // Helper: Add In-Form Navigation Bar to current canvas
    async function addInFormNavigationButtons() {
      if (!currentPayload?.moduleIndex) return;
      const m = currentPayload.moduleIndex;
      const canvasName = canvasSelect.value || m.canvases?.[0]?.name || 'CANVAS_MAIN';
      let targetBlock = m.blocks.find(b => b.name === 'CONTROL') || m.blocks[0];
      if (!targetBlock) return;

      // Determine good Y position below existing items
      let maxY = 240;
      for (const b of m.blocks) {
        for (const it of b.items) {
          if (it.canvasName === canvasName && (it.yPosition || 0) > maxY) {
            maxY = (it.yPosition || 0) + (it.height || 22) + 18;
          }
        }
      }
      if (maxY > 410) maxY = 405;

      const navButtons = [
        { name: 'BTN_PREV', label: '< Previous', actionType: 'PREVIOUS_RECORD', x: 32, w: 96, bg: '#e8e4d8', fg: '#111111' },
        { name: 'BTN_NEXT', label: 'Next >', actionType: 'NEXT_RECORD', x: 138, w: 96, bg: '#e8e4d8', fg: '#111111' },
        { name: 'BTN_SEARCH', label: 'Search', actionType: 'EXECUTE_QUERY', x: 248, w: 88, bg: '#dbeafe', fg: '#1e3a8a' },
        { name: 'BTN_SHOW_ALL', label: 'Show all', actionType: 'SHOW_ALL', x: 344, w: 88, bg: '#e8e4d8', fg: '#111111' },
        { name: 'BTN_SAVE', label: 'Save', actionType: 'COMMIT_FORM', x: 440, w: 84, bg: '#dcfce7', fg: '#14532d' }
      ];

      let addedCount = 0;
      for (const nb of navButtons) {
        let existing = null;
        for (const b of m.blocks) {
          const found = b.items.find(i => i.name === nb.name);
          if (found) { existing = found; break; }
        }
        if (existing) {
          existing.canvasName = canvasName;
          existing.visible = true;
          existing.actionType = nb.actionType;
          existing.backColor = existing.backColor || nb.bg;
          existing.foreColor = existing.foreColor || nb.fg;
        } else {
          targetBlock.items.push({
            name: nb.name,
            itemType: 'Push Button',
            dataType: 'Char',
            canvasName,
            xPosition: nb.x,
            yPosition: maxY,
            width: nb.w,
            height: 24,
            label: nb.label,
            actionType: nb.actionType,
            backColor: nb.bg,
            foreColor: nb.fg,
            fontWeight: 'bold',
            enabled: true,
            visible: true,
            dml: { databaseItem: false }
          });
          addedCount++;
        }
      }

      renderNavigatorTree();
      renderCanvasSvg();
      await syncModuleToServer();
      showBanner(addedCount > 0
        ? `Added ${addedCount} In-Form Navigation & Action Buttons (< Previous, Next >, Search, Show all, Save)`
        : 'In-Form Navigation Buttons updated on canvas');
      setTimeout(hideBanner, 2500);
    }

    // Helper: Add Titled Frame to current canvas
    async function addTitledFrameToCanvas(customTitle) {
      if (!currentPayload?.moduleIndex) return;
      const m = currentPayload.moduleIndex;
      const canvasName = canvasSelect.value || (m.canvases?.[0]?.name) || 'CANVAS_MAIN';
      const canvas = m.canvases.find(c => c.name === canvasName);
      if (!canvas) return;
      if (!m.graphics) m.graphics = [];

      const frameName = `FRAME_${m.graphics.length + 1}`;
      const frameObj = {
        name: frameName,
        canvasName,
        graphicsType: 'Frame',
        frameTitle: customTitle || `${m.windows?.[0]?.title || m.key?.name || 'Form'} Details`,
        xPosition: 18,
        yPosition: 56,
        width: Math.max(380, (canvas.width || 620) - 36),
        height: Math.max(200, (canvas.height || 420) - 110)
      };
      m.graphics.push(frameObj);
      selectedObject = { type: 'GRAPHIC', graphic: frameObj, canvas };
      renderNavigatorTree();
      renderPropertyInspector(selectedObject);
      renderCanvasSvg();
      await syncModuleToServer();
      showBanner(`Added Titled Frame '${frameObj.frameTitle}' — edit title or size in Property Palette`);
      setTimeout(hideBanner, 2500);
    }

    // Wire Top Formatting Ribbon (#layoutFormatRibbon)
    const ribbonBackColor = document.getElementById('fmtBackColor');
    if (ribbonBackColor) {
      ribbonBackColor.addEventListener('input', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.backColor = ribbonBackColor.value;
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    const ribbonForeColor = document.getElementById('fmtForeColor');
    if (ribbonForeColor) {
      ribbonForeColor.addEventListener('input', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.foreColor = ribbonForeColor.value;
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    document.querySelectorAll('.swatch-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const bg = btn.getAttribute('data-fill');
        const fg = btn.getAttribute('data-fg');
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.backColor = bg;
          selectedObject.item.foreColor = fg;
          if (ribbonBackColor) ribbonBackColor.value = normalizeHexColor(bg);
          if (ribbonForeColor) ribbonForeColor.value = normalizeHexColor(fg);
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        } else {
          showBanner('Select a button or field on the canvas first to apply a color preset.');
          setTimeout(hideBanner, 1800);
        }
      });
    });

    const ribbonFontFamily = document.getElementById('fmtFontFamily');
    if (ribbonFontFamily) {
      ribbonFontFamily.addEventListener('change', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.fontName = ribbonFontFamily.value;
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    const ribbonFontSize = document.getElementById('fmtFontSize');
    if (ribbonFontSize) {
      ribbonFontSize.addEventListener('change', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.fontSize = parseInt(ribbonFontSize.value, 10) || 11;
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    const ribbonBoldBtn = document.getElementById('fmtBoldBtn');
    if (ribbonBoldBtn) {
      ribbonBoldBtn.addEventListener('click', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.fontWeight = selectedObject.item.fontWeight === 'bold' ? 'normal' : 'bold';
          ribbonBoldBtn.classList.toggle('active', selectedObject.item.fontWeight === 'bold');
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    const ribbonItalicBtn = document.getElementById('fmtItalicBtn');
    if (ribbonItalicBtn) {
      ribbonItalicBtn.addEventListener('click', async () => {
        if (selectedObject?.type === 'ITEM') {
          selectedObject.item.fontStyle = selectedObject.item.fontStyle === 'italic' ? 'normal' : 'italic';
          ribbonItalicBtn.classList.toggle('active', selectedObject.item.fontStyle === 'italic');
          renderCanvasSvg();
          renderPropertyInspector(selectedObject);
          await syncModuleToServer();
        }
      });
    }

    const addInFormNavBarBtn = document.getElementById('addInFormNavBarBtn');
    if (addInFormNavBarBtn) {
      addInFormNavBarBtn.addEventListener('click', addInFormNavigationButtons);
    }

    const addTitledFrameBtn = document.getElementById('addTitledFrameBtn');
    if (addTitledFrameBtn) {
      addTitledFrameBtn.addEventListener('click', () => addTitledFrameToCanvas());
    }

    // Wire Left Vertical Tool Palette (#canvasToolPalette)
    const palette = document.getElementById('canvasToolPalette');
    if (palette) {
      palette.querySelectorAll('.palette-tool').forEach(btn => {
        btn.addEventListener('click', async () => {
          const tool = btn.getAttribute('data-tool');
          if (!currentPayload?.moduleIndex) return;
          const m = currentPayload.moduleIndex;
          const canvasName = canvasSelect.value || (m.canvases?.[0]?.name) || 'CANVAS_MAIN';
          const block = (selectedObject?.block) || m.blocks[0];

          if (tool === 'select') {
            palette.querySelectorAll('.palette-tool').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            return;
          }
          if (tool === 'navBar') {
            await addInFormNavigationButtons();
            return;
          }
          if (tool === 'frame') {
            await addTitledFrameToCanvas();
            return;
          }
          if (tool === 'textGraphic') {
            const canvas = m.canvases.find(c => c.name === canvasName);
            if (canvas) {
              if (!m.graphics) m.graphics = [];
              const g = {
                name: `LABEL_${m.graphics.length + 1}`,
                canvasName,
                graphicsType: 'Text',
                graphicsText: 'Section Header Label',
                xPosition: 32,
                yPosition: 36,
                width: 180,
                height: 20
              };
              m.graphics.push(g);
              selectedObject = { type: 'GRAPHIC', graphic: g, canvas };
              renderNavigatorTree();
              renderPropertyInspector(selectedObject);
              renderCanvasSvg();
              await syncModuleToServer();
            }
            return;
          }
          if (tool === 'delete') {
            if (selectedObject?.type === 'ITEM' && selectedObject.block) {
              const idx = selectedObject.block.items.indexOf(selectedObject.item);
              if (idx >= 0) {
                selectedObject.block.items.splice(idx, 1);
                selectedObject = null;
                renderNavigatorTree();
                renderPropertyInspector(null);
                renderCanvasSvg();
                await syncModuleToServer();
                showBanner('Deleted selected item from canvas');
                setTimeout(hideBanner, 1800);
              }
            } else if (selectedObject?.type === 'GRAPHIC') {
              const idx = (m.graphics || []).indexOf(selectedObject.graphic);
              if (idx >= 0) {
                m.graphics.splice(idx, 1);
                selectedObject = null;
                renderNavigatorTree();
                renderPropertyInspector(null);
                renderCanvasSvg();
                await syncModuleToServer();
              }
            }
            return;
          }

          if (!block) return;
          const count = block.items.length + 1;
          let newItem = null;
          if (tool === 'button') {
            newItem = {
              name: `BTN_ACTION_${count}`,
              itemType: 'Push Button',
              dataType: 'Char',
              canvasName,
              xPosition: 420,
              yPosition: 60 + ((count % 6) * 32),
              width: 96,
              height: 24,
              label: 'Action Button',
              actionType: 'EXECUTE_QUERY',
              backColor: '#dbeafe',
              foreColor: '#1e3a8a',
              fontWeight: 'bold',
              enabled: true,
              visible: true,
              dml: { databaseItem: false }
            };
          } else if (tool === 'textItem' || tool === 'displayItem' || tool === 'checkbox') {
            const typeLabel = tool === 'displayItem' ? 'Display Item' : (tool === 'checkbox' ? 'Check Box' : 'Text Item');
            newItem = {
              name: `ITEM_${count}`,
              itemType: typeLabel,
              dataType: 'Varchar2',
              canvasName,
              xPosition: 140,
              yPosition: 80 + ((count % 8) * 28),
              width: tool === 'checkbox' ? 90 : 140,
              height: 20,
              prompt: tool === 'checkbox' ? '' : `Field ${count}`,
              label: tool === 'checkbox' ? `Option ${count}` : '',
              promptAttachmentEdge: 'Start',
              promptAttachmentOffset: 8,
              backColor: tool === 'displayItem' ? '#f3f2ee' : '#ffffff',
              foreColor: '#111111',
              enabled: tool !== 'displayItem',
              visible: true,
              dml: { databaseItem: true, maximumLength: 60 }
            };
          }

          if (newItem) {
            block.items.push(newItem);
            selectedObject = { type: 'ITEM', item: newItem, block };
            renderNavigatorTree();
            renderPropertyInspector(selectedObject);
            renderCanvasSvg();
            await syncModuleToServer();
            showBanner(`Added ${newItem.itemType} '${newItem.name}' — customize in Property Palette`);
            setTimeout(hideBanner, 2200);
          }
        });
      });
    }

    // Wire Data Block & Layout Wizard Modal (#blockLayoutWizardModal)
    const wizModal = document.getElementById('blockLayoutWizardModal');
    const openWizardBtn = document.getElementById('openWizardBtn');
    const openWizardFromTreeBtn = document.getElementById('openWizardFromTreeBtn');
    const wizTableSelect = document.getElementById('wizTableSelect');
    const wizBlockName = document.getElementById('wizBlockNameInput');
    const wizColumnsList = document.getElementById('wizColumnsContainer');
    const wizFrameTitle = document.getElementById('wizFrameTitleInput');
    const wizWindowTitle = document.getElementById('wizWindowTitleInput');
    const wizRecordsDisplayed = document.getElementById('wizRecordsDisplayedInput');
    const wizRecordDistance = document.getElementById('wizRecordDistInput');
    const wizStyleFormCard = document.getElementById('wizStyleFormCard');
    const wizStyleTabularCard = document.getElementById('wizStyleTabularCard');

    function populateWizardForTable(tableName) {
      const schema = TABLE_SCHEMAS[tableName] || TABLE_SCHEMAS.OFW_DEMO_EMPLOYEES;
      if (wizBlockName) wizBlockName.value = schema.blockName;
      if (wizFrameTitle) wizFrameTitle.value = schema.frameTitle;
      if (wizWindowTitle) wizWindowTitle.value = schema.windowTitle;
      if (wizColumnsList) {
        wizColumnsList.innerHTML = schema.columns.map(col => `
          <div class="wiz-col-row">
            <input type="checkbox" class="wiz-col-check" data-col="${escapeHtml(col.name)}" data-type="${escapeHtml(col.type)}" data-width="${col.width || 120}" checked>
            <span class="wiz-col-name">${escapeHtml(col.name)}</span>
            <span class="wiz-col-type">${escapeHtml(col.type)}</span>
            <input type="text" class="prop-input wiz-col-prompt" data-col="${escapeHtml(col.name)}" value="${escapeHtml(col.prompt)}" style="width:120px; padding:2px 6px; font-size:11px;" title="Field Prompt">
          </div>
        `).join('');
      }
    }

    document.getElementById('wizDescribeBtn')?.addEventListener('click', async event => {
      const table = document.getElementById('wizLiveTableInput').value.trim().toUpperCase();
      const status = document.getElementById('wizSchemaStatus');
      event.target.disabled = true;
      try {
        status.textContent = 'Reading database schema…';
        const response = await fetch('/api/db/describe', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ tableName: table, backend: dbBackendSelect.value,
            username: dbUserInput.value.trim() || null, password: dbPassInput.value || null })
        });
        const result = await response.json();
        if (!response.ok) throw new Error(result.error || 'Could not read schema');
        const columns = result.records.map(row => ({ name: row.values.COLUMN_NAME,
          type: /NUMBER|INT|REAL|FLOAT|DECIMAL/i.test(row.values.DATA_TYPE) ? 'Number' : /DATE|TIME/i.test(row.values.DATA_TYPE) ? 'Date' : 'Varchar2',
          prompt: row.values.COLUMN_NAME, width: 140 }));
        if (!columns.length) throw new Error('No columns found. Check the table name and connected schema.');
        TABLE_SCHEMAS[table] = { blockName: table, frameTitle: table, windowTitle: table, columns };
        if (![...wizTableSelect.options].some(option => option.value === table)) wizTableSelect.add(new Option(`${table} (database schema)`, table));
        wizTableSelect.value = table;
        populateWizardForTable(table);
        status.textContent = `Loaded ${columns.length} columns from ${table} using ${dbBackendSelect.value}.`;
      } catch (error) { status.textContent = error.message; }
      finally { event.target.disabled = false; }
    });

    function openWizardModal() {
      if (!wizModal) return;
      // Default table based on active module
      const modName = (currentPayload?.moduleIndex?.key?.name || '').toUpperCase();
      let defaultTable = 'OFW_DEMO_EMPLOYEES';
      if (modName.includes('SALES')) defaultTable = 'OFW_DEMO_SALES';
      else if (modName.includes('ORDER')) defaultTable = 'ORDERS';
      else if (modName.includes('CLAIM')) defaultTable = 'CLAIMS';
      if (wizTableSelect) wizTableSelect.value = defaultTable;
      populateWizardForTable(defaultTable);
      if (typeof wizModal.showModal === 'function') {
        wizModal.showModal();
      } else {
        wizModal.style.display = 'flex';
      }
    }

    if (openWizardBtn) openWizardBtn.addEventListener('click', openWizardModal);
    if (openWizardFromTreeBtn) openWizardFromTreeBtn.addEventListener('click', openWizardModal);

    if (wizTableSelect) {
      wizTableSelect.addEventListener('change', () => populateWizardForTable(wizTableSelect.value));
    }

    document.getElementById('wizSelectAllColsBtn')?.addEventListener('click', () => {
      wizColumnsList?.querySelectorAll('.wiz-col-check').forEach(cb => { cb.checked = true; });
    });
    document.getElementById('wizClearColsBtn')?.addEventListener('click', () => {
      wizColumnsList?.querySelectorAll('.wiz-col-check').forEach(cb => { cb.checked = false; });
    });

    if (wizStyleFormCard && wizStyleTabularCard) {
      wizStyleFormCard.addEventListener('click', () => {
        wizStyleFormCard.classList.add('active');
        wizStyleTabularCard.classList.remove('active');
        if (wizRecordsDisplayed) wizRecordsDisplayed.value = '1';
        if (wizRecordDistance) wizRecordDistance.value = '0';
      });
      wizStyleTabularCard.addEventListener('click', () => {
        wizStyleTabularCard.classList.add('active');
        wizStyleFormCard.classList.remove('active');
        if (wizRecordsDisplayed && parseInt(wizRecordsDisplayed.value, 10) < 2) wizRecordsDisplayed.value = '5';
        if (wizRecordDistance) wizRecordDistance.value = '4';
      });
    }

    document.getElementById('wizApplyBtn')?.addEventListener('click', async () => {
      if (!currentPayload?.moduleIndex) return;
      if (runtime.active) return showBanner('Stop the running form before changing its structure.');
      const mode = document.getElementById('wizBlockModeSelect').value;
      const m = structuredClone(currentPayload.moduleIndex);
      const tableName = wizTableSelect?.value || 'OFW_DEMO_EMPLOYEES';
      const blockName = (wizBlockName?.value || 'DATA_BLOCK').trim().toUpperCase();
      const frameTitle = wizFrameTitle?.value || '';
      const windowTitle = wizWindowTitle?.value || '';
      const layoutStyle = wizStyleTabularCard?.classList.contains('active') ? 'tabular' : 'form';
      const recordsDisplayed = parseInt(wizRecordsDisplayed?.value, 10) || (layoutStyle === 'tabular' ? 5 : 1);
      const distanceBetweenRecords = parseInt(wizRecordDistance?.value, 10) || 0;
      const includeNav = document.getElementById('wizIncludeNavBtns')?.checked !== false;
      const includeSearch = document.getElementById('wizIncludeActionBtns')?.checked !== false;

      const selectedCols = [];
      wizColumnsList?.querySelectorAll('.wiz-col-row').forEach(row => {
        const cb = row.querySelector('.wiz-col-check');
        const promptInput = row.querySelector('.wiz-col-prompt');
        if (cb && cb.checked) {
          selectedCols.push({
            name: cb.getAttribute('data-col'),
            type: cb.getAttribute('data-type') || 'Varchar2',
            width: parseInt(cb.getAttribute('data-width'), 10) || 120,
            prompt: promptInput?.value || cb.getAttribute('data-col')
          });
        }
      });

      if (selectedCols.length === 0) {
        showBanner('Select at least one column in the Data Block Wizard.');
        return;
      }

      if (!/^[A-Z][A-Z0-9_]*$/.test(blockName)) return showBanner('Use a valid Oracle block name: letters, digits and underscores.');
      if (mode === 'append' && m.blocks.some(b => b.name.toUpperCase() === blockName)) {
        return showBanner('That block already exists. Choose a new block name or Update / Replace.');
      }
      if (mode === 'newForm') {
        const name = document.getElementById('wizFormNameInput').value.trim().toUpperCase();
        if (!/^[A-Z][A-Z0-9_]*$/.test(name)) return showBanner('Enter a valid new form name.');
        m.key = { name, type: 'FORM' };
        m.sourceFile = '';
        m.convertedFile = '';
        for (const key of Object.keys(m)) if (Array.isArray(m[key])) m[key] = [];
        m.menuModule = null;
        m.coordinate = { coordinateSystem: 'Real', realUnit: 'Pixel', characterCellWidth: 8, characterCellHeight: 16, defaultFontScaling: true };
        m.windows = [{ name: 'MAIN_WINDOW', title: windowTitle || name, width: 900, height: 650 }];
        m.canvases = [{ name: 'CANVAS_MAIN', canvasType: 'Content', windowName: 'MAIN_WINDOW', width: 900, height: 650, tabPages: [] }];
      }
      const canvasName = mode === 'newForm' ? 'CANVAS_MAIN' : (canvasSelect.value || m.canvases?.[0]?.name || 'CANVAS_MAIN');
      const canvas = m.canvases.find(c => c.name === canvasName);

      // Update Window Title if provided
      if (windowTitle && m.windows && m.windows.length > 0) {
        m.windows[0].title = windowTitle;
      }

      // Build items with clean Form or Tabular coordinates
      const items = [];
      let maxItemY = 90;
      if (layoutStyle === 'tabular') {
        let curX = 28;
        const startY = 82;
        for (const col of selectedCols) {
          const w = Math.min(130, Math.max(64, col.width));
          items.push({
            name: col.name,
            itemType: 'Text Item',
            dataType: col.type,
            columnName: col.name,
            canvasName,
            xPosition: curX,
            yPosition: startY,
            width: w,
            height: 20,
            prompt: col.prompt,
            promptAttachmentEdge: 'Top',
            promptAttachmentOffset: 6,
            backColor: '#ffffff',
            foreColor: '#111111',
            enabled: true,
            visible: true,
            dml: { databaseItem: true, insertAllowed: true, updateAllowed: true, queryAllowed: true }
          });
          curX += w + 8;
        }
        maxItemY = startY + (recordsDisplayed * (20 + distanceBetweenRecords)) + 24;
      } else {
        // Two-column Form Layout
        for (let i = 0; i < selectedCols.length; i++) {
          const col = selectedCols[i];
          const isRightCol = (i % 2 === 1) && selectedCols.length > 4;
          const rowIdx = selectedCols.length > 4 ? Math.floor(i / 2) : i;
          const x = isRightCol ? 390 : 126;
          const y = 74 + (rowIdx * 32);
          items.push({
            name: col.name,
            itemType: 'Text Item',
            dataType: col.type,
            columnName: col.name,
            canvasName,
            xPosition: x,
            yPosition: y,
            width: col.width,
            height: 20,
            prompt: col.prompt,
            promptAttachmentEdge: 'Start',
            promptAttachmentOffset: 8,
            backColor: '#ffffff',
            foreColor: '#111111',
            enabled: true,
            visible: true,
            dml: { databaseItem: true, insertAllowed: true, updateAllowed: true, queryAllowed: true }
          });
          if (y + 36 > maxItemY) maxItemY = y + 36;
        }
      }

      // Add In-Form Navigation / Action Buttons directly onto the block
      let btnX = 32;
      const btnY = Math.min(410, maxItemY + 14);
      if (includeNav) {
        items.push({
          name: 'BTN_PREV',
          itemType: 'Push Button',
          dataType: 'Char',
          canvasName,
          xPosition: btnX,
          yPosition: btnY,
          width: 96,
          height: 24,
          label: '< Previous',
          actionType: 'PREVIOUS_RECORD',
          backColor: '#e8e4d8',
          foreColor: '#111111',
          fontWeight: 'bold',
          enabled: true,
          visible: true,
          dml: { databaseItem: false }
        });
        btnX += 104;
        items.push({
          name: 'BTN_NEXT',
          itemType: 'Push Button',
          dataType: 'Char',
          canvasName,
          xPosition: btnX,
          yPosition: btnY,
          width: 96,
          height: 24,
          label: 'Next >',
          actionType: 'NEXT_RECORD',
          backColor: '#e8e4d8',
          foreColor: '#111111',
          fontWeight: 'bold',
          enabled: true,
          visible: true,
          dml: { databaseItem: false }
        });
        btnX += 108;
      }
      if (includeSearch) {
        items.push({
          name: 'BTN_SEARCH',
          itemType: 'Push Button',
          dataType: 'Char',
          canvasName,
          xPosition: btnX,
          yPosition: btnY,
          width: 88,
          height: 24,
          label: 'Search',
          actionType: 'EXECUTE_QUERY',
          backColor: '#dbeafe',
          foreColor: '#1e3a8a',
          fontWeight: 'bold',
          enabled: true,
          visible: true,
          dml: { databaseItem: false }
        });
        btnX += 96;
        items.push({
          name: 'BTN_SHOW_ALL',
          itemType: 'Push Button',
          dataType: 'Char',
          canvasName,
          xPosition: btnX,
          yPosition: btnY,
          width: 88,
          height: 24,
          label: 'Show all',
          actionType: 'SHOW_ALL',
          backColor: '#e8e4d8',
          foreColor: '#111111',
          fontWeight: 'bold',
          enabled: true,
          visible: true,
          dml: { databaseItem: false }
        });
        btnX += 96;
        items.push({
          name: 'BTN_SAVE',
          itemType: 'Push Button',
          dataType: 'Char',
          canvasName,
          xPosition: btnX,
          yPosition: btnY,
          width: 84,
          height: 24,
          label: 'Save',
          actionType: 'COMMIT_FORM',
          backColor: '#dcfce7',
          foreColor: '#14532d',
          fontWeight: 'bold',
          enabled: true,
          visible: true,
          dml: { databaseItem: false }
        });
      }

      // Update or create block
      let block = m.blocks.find(b => b.name === blockName);
      if (block) {
        m.triggers = (m.triggers || []).filter(t => t.blockName?.toUpperCase() !== blockName || !t.itemName || items.some(i => i.name.toUpperCase() === t.itemName.toUpperCase()));
        block.queryDataSourceName = tableName;
        block.recordsDisplayed = recordsDisplayed;
        block.distanceBetweenRecords = distanceBetweenRecords;
        block.items = items;
      } else {
        block = {
          name: blockName,
          queryDataSourceName: tableName,
          recordsDisplayed,
          distanceBetweenRecords,
          recordOrientation: 'Vertical',
          dml: {
            databaseBlock: true,
            queryAllowed: true,
            insertAllowed: true,
            updateAllowed: true,
            deleteAllowed: true,
            whereClause: null,
            orderByClause: selectedCols[0]?.name || null
          },
          items
        };
        m.blocks.unshift(block);
      }

      // Add or update Titled Frame on canvas
      if (canvas && frameTitle) {
        if (!m.graphics) m.graphics = [];
        let existingFrame = m.graphics.find(g => g.canvasName === canvasName && g.name === `FRAME_${blockName}`);
        if (existingFrame) {
          existingFrame.frameTitle = frameTitle;
          existingFrame.xPosition = 16;
          existingFrame.yPosition = 46;
          existingFrame.width = Math.max(540, (canvas.width || 570) - 32);
          existingFrame.height = Math.max(240, btnY - 22);
        } else {
          m.graphics.push({
            name: `FRAME_${blockName}`,
            canvasName,
            graphicsType: 'Frame',
            frameTitle,
            xPosition: 16,
            yPosition: 46,
            width: Math.max(540, (canvas.width || 570) - 32),
            height: Math.max(240, btnY - 22)
          });
        }
      }

      try {
        const url = mode === 'newForm' ? '/api/forms/new' : `/api/forms/module/${encodeURIComponent(m.key.name)}/sync`;
        const response = await checkedSave(url, {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(m)
        });
        setModulePayload(await response.json());
      } catch (error) { showBanner(`Could not save generated form: ${error.message}`); return; }
      if (typeof wizModal.close === 'function') wizModal.close();
      else wizModal.style.display = 'none';
      selectedObject = { type: 'BLOCK', block: currentPayload.moduleIndex.blocks.find(b => b.name === blockName) };
      renderPropertyInspector(selectedObject);
      showBanner(`🧙‍♂️ Generated Block '${blockName}' (${selectedCols.length} columns, ${layoutStyle === 'tabular' ? `${recordsDisplayed}-row Tabular Grid` : 'Form Layout'}) — click ▶ Run or Search to query!`);
      setTimeout(hideBanner, 3200);
    });
  }

  function renderDiffChange(label, before, after) {
    return `<details class="diff-change-item"><summary>${escapeHtml(label)}</summary><div class="diff-values"><pre>${escapeHtml(before ?? '(absent)')}</pre><pre>${escapeHtml(after ?? '(absent)')}</pre></div></details>`;
  }

  function renderDiffVisuals(base, modified) {
    // Shared coordinate bounds preserve visible size and position differences.
    const items = module => (module.blocks || []).flatMap(b => (b.items || [])
      .filter(i => i.canvasName && i.visible !== false).map(i => ({ ...i, blockName: b.name })));
    const all = [...items(base), ...items(modified)];
    const minX = Math.min(0, ...all.map(i => i.xPosition || 0));
    const minY = Math.min(0, ...all.map(i => i.yPosition || 0));
    const width = Math.max(1, ...all.map(i => (i.xPosition || 0) + (i.width || 80))) - minX;
    const height = Math.max(1, ...all.map(i => (i.yPosition || 0) + (i.height || 24))) - minY;
    const draw = module => `<figure><figcaption>${escapeHtml(module.key.name)}</figcaption><svg role="img" aria-label="Item layout overview" viewBox="${minX - 10} ${minY - 10} ${width + 20} ${height + 20}">${items(module).map(i => `<g><title>${escapeHtml(`${i.blockName}.${i.name} · ${i.canvasName}`)}</title><rect x="${i.xPosition || 0}" y="${i.yPosition || 0}" width="${i.width || 80}" height="${i.height || 24}" fill="none" stroke="currentColor"/><text x="${(i.xPosition || 0) + 3}" y="${(i.yPosition || 0) + 13}" font-size="11" fill="currentColor">${escapeHtml(i.label || i.prompt || i.name)}</text></g>`).join('')}</svg></figure>`;
    return `<p class="diff-note">Item geometry overview at a shared scale. Canvases are overlaid; fonts, graphics and Oracle runtime styling are not reproduced.</p><div class="diff-visuals">${draw(base)}${draw(modified)}</div>`;
  }

  function renderDiffReport(diff, container = diffContainer) {
    if (!container) return;
    if (!diff || !diff.baseName) {
      container.innerHTML = '<div class="empty-state">No diff results available.</div>';
      return;
    }

    const hasDiff = (diff.modelChanges?.length || 0) > 0 || (diff.blocksAdded?.length || 0) > 0 ||
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
      container.innerHTML = html;
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
      for (const item of (b.itemsModified || [])) {
        for (const change of (item.propertyChanges || [])) html += renderDiffChange(`${item.name}.${change.property}`, change.oldValue, change.newValue);
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

    for (const change of (diff.modelChanges || [])) {
      html += renderDiffChange(`Parsed model section: ${change.property}`, change.oldValue, change.newValue);
    }
    for (const canvas of (diff.canvasesModified || [])) {
      for (const change of canvas.propertyChanges) html += renderDiffChange(`Canvas ${canvas.name}.${change.property}`, change.oldValue, change.newValue);
    }
    for (const window of (diff.windowsModified || [])) {
      for (const change of window.propertyChanges) html += renderDiffChange(`Window ${window.name}.${change.property}`, change.oldValue, change.newValue);
    }
    for (const name of (diff.formTriggersAdded || [])) html += `<p>+ Form trigger: ${escapeHtml(name)}</p>`;
    for (const name of (diff.formTriggersRemoved || [])) html += `<p>− Form trigger: ${escapeHtml(name)}</p>`;
    for (const trigger of [...(diff.formTriggersModified || []), ...(diff.blocksModified || []).flatMap(b => b.triggersModified || [])]) {
      html += renderDiffChange(`Trigger ${trigger.name}`, trigger.oldText, trigger.newText);
    }
    container.innerHTML = html;
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
    openBottomDrawer('drawerPlsql');
    loadCode(name, filePath, 'TRIGGER', null, false);
  };

  async function loadCode(name, relativePath, scope, sourceModule = null, switchView = false) {
    if (!currentPayload || !relativePath) return;
    const moduleName = sourceModule || currentPayload.summary.moduleName;
    codeTitle.textContent = name;
    codeScopeBadge.textContent = scope;

    const drawerTitle = document.getElementById('drawerCodeTitle');
    if (drawerTitle) drawerTitle.textContent = `${name} (${scope})`;
    const modalTitle = document.getElementById('modalCodeTitle');
    if (modalTitle) modalTitle.textContent = `${name} (${scope})`;

    try {
      const res = await fetch(`/api/module/${encodeURIComponent(moduleName)}/source?path=${encodeURIComponent(relativePath)}`);
      if (res.ok) {
        const data = await res.json();
        const highlighted = highlightPlsql(data.code);
        plsqlCodeDisplay.innerHTML = highlighted;
        const drawerPre = document.getElementById('drawerPlsqlDisplay');
        if (drawerPre) drawerPre.innerHTML = highlighted;
        const modalPre = document.getElementById('modalPlsqlDisplay');
        if (modalPre) modalPre.innerHTML = highlighted;
        if (switchView) switchTab('codeTab');
      } else {
        const errText = `Could not load PL/SQL source: ${await res.text()}`;
        plsqlCodeDisplay.textContent = errText;
      }
    } catch (e) {
      plsqlCodeDisplay.textContent = `Error fetching source: ${e.message}`;
    }
  }

  // XML Source tab default: the full XML of the open form, menu, library, or report.
  async function loadFullXml(moduleName) {
    xmlTitle.textContent = `${moduleName} (full XML)`;
    xmlLinesBadge.textContent = '';
    xmlSliceDisplay.textContent = '<!-- Loading XML source... -->';
    try {
      const res = await fetch(`/api/module/${encodeURIComponent(moduleName)}/xml`);
      if (!res.ok) {
        xmlSliceDisplay.textContent = `<!-- No XML source available for ${moduleName} (binary .fmb files have no XML until converted). -->`;
        return;
      }
      const data = await res.json();
      if (currentPayload && currentPayload.summary.moduleName !== moduleName) return;
      xmlSliceDisplay.textContent = data.xml;
      xmlLinesBadge.textContent = `${data.endLine - data.startLine + 1} lines`;
    } catch (e) {
      xmlSliceDisplay.textContent = `<!-- Could not load XML source: ${e.message} -->`;
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
  let currentReportPayload = null;

  document.getElementById('reportTemplateSelect')?.addEventListener('change', () => {
    if (currentReportPayload && reportPreviewFrame) {
      reportPreviewFrame.srcdoc = applyReportTemplateTheme(currentReportPayload.html, document.getElementById('reportTemplateSelect')?.value || 'blue');
    }
  });

  document.getElementById('exportReportXmlBtn')?.addEventListener('click', () => {
    const report = currentReportPayload?.report || reportDefinition;
    if (!report) {
      showBanner('Select a report before exporting its data model.');
      return;
    }
    const xmlStr = serializeReportToXml(report);
    const blob = new Blob([xmlStr], { type: 'application/xml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${(report.name || 'report').toLowerCase()}_report.xml`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    showBanner(`💾 Exported Oracle Report XML (${a.download}) with the parsed data model. Paper layout and unparsed Oracle metadata are not included.`);
    setTimeout(hideBanner, 3600);
  });

  function applyReportTemplateTheme(html, theme) {
    if (!html) return '';
    const themes = {
      blue:  { accent: '#1e3a8a', border: '#2563eb', thBg: '#eff6ff', thText: '#1e40af', cardBg: '#f8fafc' },
      beige: { accent: '#7c2d12', border: '#c2410c', thBg: '#ffedd5', thText: '#7c2d12', cardBg: '#fffbeb' },
      wine:  { accent: '#831843', border: '#be185d', thBg: '#fce7f3', thText: '#831843', cardBg: '#fdf2f8' },
      green: { accent: '#14532d', border: '#15803d', thBg: '#dcfce7', thText: '#14532d', cardBg: '#f0fdf4' },
      gray:  { accent: '#1f2937', border: '#4b5563', thBg: '#f3f4f6', thText: '#1f2937', cardBg: '#f9fafb' }
    };
    const t = themes[theme] || themes.blue;
    const overrideCss = `
      <style>
        .report-title { color: ${t.accent} !important; border-bottom-color: ${t.border} !important; }
        .badge { background: ${t.border} !important; }
        .parent-group-card { background: ${t.cardBg} !important; }
        .child-table th, .report-table th { background: ${t.thBg} !important; color: ${t.thText} !important; }
      </style>
    `;
    return html.replace('</head>', `${overrideCss}</head>`);
  }

  function serializeReportToXml(rep) {
    const lines = [];
    lines.push(`<?xml version="1.0" encoding="UTF-8"?>`);
    lines.push(`<report name="${escapeHtml(rep.name || 'REPORT')}" title="${escapeHtml(rep.title || '')}" unitOfMeasurement="${escapeHtml(rep.unit_of_measurement || 'inch')}">`);
    lines.push(`  <data>`);
    for (const p of (rep.data?.parameters || [])) {
      lines.push(`    <userParameter name="${escapeHtml(p.name)}" datatype="${escapeHtml(p.datatype || 'character')}"${p.initial_value == null ? '' : ` initialValue="${escapeHtml(p.initial_value)}"`}/>`);
    }
    for (const ds of (rep.data?.data_sources || [])) {
      lines.push(`    <dataSource name="${escapeHtml(ds.name)}">`);
      lines.push(`      <select>${escapeHtml(ds.select_sql || '')}</select>`);
      for (const g of (ds.groups || [])) {
        lines.push(`      <group name="${escapeHtml(g.name)}">`);
        for (const c of (g.data_items || g.columns || [])) {
          lines.push(`        <dataItem name="${escapeHtml(c.name)}" datatype="${escapeHtml(c.datatype || 'character')}" width="${c.width || 20}"/>`);
        }
        for (const f of (g.formulas || [])) {
          lines.push(`        <formula name="${escapeHtml(f.name)}" source="${escapeHtml(f.source || f.plsql || '')}" datatype="${escapeHtml(f.datatype || 'number')}"/>`);
        }
        for (const s of (g.summaries || [])) {
          lines.push(`        <summary name="${escapeHtml(s.name)}" source="${escapeHtml(s.source)}" function="${escapeHtml(String(s.function || 'sum').toLowerCase())}" reset="${escapeHtml(s.reset || s.reset_at || g.name)}"/>`);
        }
        lines.push(`      </group>`);
      }
      lines.push(`    </dataSource>`);
    }
    for (const lnk of (rep.data?.links || [])) {
      lines.push(`    <link name="${escapeHtml(lnk.name || 'L_LINK')}" parentGroup="${escapeHtml(lnk.parent_group)}" parentColumn="${escapeHtml(lnk.parent_column)}" childQuery="${escapeHtml(lnk.child_query || lnk.child_group || '')}" childColumn="${escapeHtml(lnk.child_column)}" condition="eq"/>`);
    }
    lines.push(`  </data>`);
    lines.push(`</report>`);
    return lines.join('\n');
  }

  async function loadReportsList() {
    if (!reportSelect) return;
    try {
      const res = await fetch('/api/reports');
      if (res.ok) {
        const reports = await res.json();
        const selected = reportSelect.value;
        reportSelect.replaceChildren(...reports.map(r => new Option(`${r.name} - ${r.title || ''}`, r.file_name)));
        // Default to OFW_EMP_DIRECTORY if available so it matches OFW_DEMO_EMPLOYEES
        if (selected && reports.some(r => r.file_name === selected)) {
          reportSelect.value = selected;
        } else if (reports.some(r => r.file_name === 'ofw_employee_directory_report.xml')) {
          reportSelect.value = 'ofw_employee_directory_report.xml';
        }
        if (!reportDefinitionName) await inspectReport();
      }
    } catch (e) {
      console.warn("Could not load reports list:", e);
    }
  }

  let reportRequestVersion = 0;
  let reportDefinitionName = null;
  let reportDefinition = null;

  function invalidateReport(message) {
    reportRequestVersion++;
    currentReportPayload = null;
    if (printReportBtn) printReportBtn.disabled = true;
    if (runReportBtn) runReportBtn.disabled = false;
    if (reportPreviewFrame) reportPreviewFrame.srcdoc = '';
    const status = document.getElementById('reportStatus');
    if (status) status.textContent = message;
    if (reportDataTree) reportDataTree.textContent = message;
    if (document.getElementById('reportsTab')?.classList.contains('active') && propertyInspector) {
      propertyInspector.textContent = message;
    }
  }

  function showReportDefinition(report, name) {
    reportDefinition = report;
    reportDefinitionName = name;
    const fields = document.getElementById('reportParameterFields');
    fields.replaceChildren();
    for (const [index, parameter] of (report.data?.parameters || []).entries()) {
      const label = document.createElement('label');
      const input = document.createElement('input');
      input.id = `reportParameter${index}`;
      input.name = parameter.name;
      input.type = 'text';
      input.value = parameter.initial_value ?? '';
      input.autocomplete = 'off';
      label.htmlFor = input.id;
      label.textContent = `${parameter.name} (${parameter.datatype || 'character'})`;
      label.append(input);
      fields.append(label);
      input.addEventListener('input', () => invalidateReport('Parameters changed. Run to refresh the preview.'));
    }
    if (!fields.children.length) fields.textContent = 'This report has no declared parameters.';
    renderReportPlsqlStudio(report, null);
  }

  async function inspectReport() {
    if (!reportSelect?.value) return;
    invalidateReport('Loading report definition…');
    const version = reportRequestVersion;
    const name = reportSelect.value;
    reportDefinition = null;
    reportDefinitionName = null;
    document.getElementById('reportParameterFields').replaceChildren();
    document.getElementById('reportCodeUnitsList').replaceChildren();
    document.getElementById('reportPlsqlDisplay').textContent = '';
    if (runReportBtn) runReportBtn.disabled = true;
    try {
      const response = await fetch(`/api/reports/${encodeURIComponent(name)}/definition`);
      const report = await response.json();
      if (!response.ok) throw new Error(report.error || 'Could not load report definition.');
      if (version !== reportRequestVersion) return false;
      showReportDefinition(report, name);
      invalidateReport('Definition loaded. Review parameters, then run the report.');
      return true;
    } catch (error) {
      if (version === reportRequestVersion) invalidateReport(error.message);
      return false;
    }
  }

  async function executeReport() {
    if (!reportSelect?.value) return;
    if (reportDefinitionName !== reportSelect.value && !await inspectReport()) return;
    const reportName = reportSelect.value;
    const backend = reportBackendSelect?.value || 'mock';
    const parameters = Object.fromEntries(Array.from(
      document.querySelectorAll('#reportParameterFields input'), input => [input.name, input.value]
    ));
    invalidateReport('Running report…');
    const version = reportRequestVersion;
    if (runReportBtn) runReportBtn.disabled = true;
    try {
      const res = await fetch('/api/reports/run', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ report_name: reportName, backend, parameters })
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || 'Report execution failed');
      if (version !== reportRequestVersion) return;
      currentReportPayload = data;
      const theme = document.getElementById('reportTemplateSelect')?.value || 'blue';
      if (reportPreviewFrame) reportPreviewFrame.srcdoc = applyReportTemplateTheme(data.html, theme);
      if (printReportBtn) printReportBtn.disabled = false;
      document.getElementById('reportStatus').textContent = `Completed · ${reportName} · ${backend}`;
      renderReportDataTree(data.execution, data.report);
      renderReportPlsqlStudio(data.report, data.execution);
      if (document.getElementById('reportsTab')?.classList.contains('active')) {
        renderReportPropertyInspector('REPORT', { report: data.report, execution: data.execution });
      }
    } catch (error) {
      if (version === reportRequestVersion) invalidateReport(`Report failed: ${error.message}`);
    } finally {
      if (version === reportRequestVersion && runReportBtn) runReportBtn.disabled = false;
    }
  }

  function renderReportPlsqlStudio(report, execution) {
    const unitsListEl = document.getElementById('reportCodeUnitsList');
    const codeTitleEl = document.getElementById('reportCodeTitle');
    const scopeBadgeEl = document.getElementById('reportCodeScopeBadge');
    const codeDisplayEl = document.getElementById('reportPlsqlDisplay');
    if (!unitsListEl || !codeDisplayEl || !report) return;

    const units = [];
    const repName = report.name || execution?.report_name || 'REPORT';

    // 1. Extract PL/SQL Formula Columns (CF_*) from all data sources & groups
    for (const ds of (report.data?.data_sources || [])) {
      for (const grp of (ds.groups || [])) {
        for (const f of (grp.formulas || [])) {
          const rawExpr = (f.source || f.plsql || '').trim();
          const code = rawExpr || '-- No formula source was extracted.';
          units.push({
            section: 'PL/SQL Formula Columns',
            name: f.name,
            badge: 'FORMULA',
            badgeClass: 'formula',
            sub: `${grp.name} · ${rawExpr}`,
            scope: `FORMULA COLUMN (${grp.name}.${f.name})`,
            code
          });
        }
      }
    }

    // 2. Oracle Reports Lifecycle PL/SQL Triggers (BeforeParameterForm, AfterParameterForm, BeforeReport, BetweenPages, AfterReport)
    const explicitTriggers = report.triggers || [];
    if (explicitTriggers.length > 0) {
      for (const t of explicitTriggers) {
        units.push({
          section: 'Report Triggers',
          name: t.name,
          badge: 'TRIGGER',
          badgeClass: 'trigger',
          sub: 'Report Trigger',
          scope: `REPORT TRIGGER (${repName})`,
          code: t.plsql || t.body || '-- Trigger body'
        });
      }
    }

    // 3. Data Model SQL Queries (Q_*) & Summary Columns (CS_*)
    for (const ds of (report.data?.data_sources || [])) {
      const grpDesc = (ds.groups || []).map(g => {
        const cols = (g.data_items || g.columns || []).map(c => c.name).join(', ');
        const sums = (g.summaries || []).map(s => `${s.name} := ${String(s.function || 'SUM').toUpperCase()}(${s.source}) [Reset: ${s.reset || s.reset_at || g.name}]`).join('\n--   ');
        return `-- Group ${g.name} Columns: ${cols}` + (sums ? `\n--   ${sums}` : '');
      }).join('\n');
      const code = [
        `-- =====================================================================`,
        `-- Data Model SQL Query: ${ds.name}`,
        `-- ReportModule : ${repName}`,
        `${grpDesc}`,
        `-- =====================================================================`,
        (ds.select_sql || '').trim() + ';'
      ].join('\n');
      units.push({
        section: 'Data Model SQL & Links',
        name: ds.name,
        badge: 'SQL',
        badgeClass: 'sql',
        sub: `${(ds.groups || []).length} Group(s)`,
        scope: `DATA SOURCE SQL (${ds.name})`,
        code
      });
    }

    for (const lnk of (report.data?.links || [])) {
      const childTarget = lnk.child_query || lnk.child_group || 'Q_CHILD';
      const code = [
        `-- =====================================================================`,
        `-- Oracle Reports Master-Detail Data Link: ${lnk.name || 'L_LINK'}`,
        `-- Parent Group : ${lnk.parent_group}.${lnk.parent_column}`,
        `-- Child Query  : ${childTarget}.${lnk.child_column}`,
        `-- =====================================================================`,
        `-- Declarative link metadata; no SQL source was extracted for this link.`,
        `-- Condition: ${lnk.condition || 'eq'}`
      ].join('\n');
      units.push({
        section: 'Data Model SQL & Links',
        name: lnk.name || 'L_LINK',
        badge: 'LINK',
        badgeClass: 'sql',
        sub: `${lnk.parent_group} → ${childTarget}`,
        scope: `DATA LINK (${lnk.name || 'L_LINK'})`,
        code
      });
    }

    // Render grouped list
    let currentSection = '';
    let html = '';
    units.forEach((u, idx) => {
      if (u.section !== currentSection) {
        currentSection = u.section;
        html += `<div class="report-unit-section-title">${escapeHtml(currentSection)}</div>`;
      }
      html += `
        <div class="report-unit-item ${idx === 0 ? 'active' : ''}" data-rep-unit-idx="${idx}">
          <div class="report-unit-item-top">
            <span class="report-unit-name">${escapeHtml(u.name)}</span>
            <span class="report-unit-badge ${escapeHtml(u.badgeClass)}">${escapeHtml(u.badge)}</span>
          </div>
          <div class="report-unit-sub">${escapeHtml(u.sub)}</div>
        </div>
      `;
    });

    unitsListEl.innerHTML = html || '<div class="empty-state">No SQL or formula source was extracted.</div>';
    codeDisplayEl.textContent = '';
    if (codeTitleEl) codeTitleEl.textContent = 'Report source';
    if (scopeBadgeEl) scopeBadgeEl.textContent = 'EXTRACTED SOURCE';

    const selectUnit = (idx) => {
      const u = units[idx];
      if (!u) return;
      unitsListEl.querySelectorAll('.report-unit-item').forEach((el, i) => {
        el.classList.toggle('active', i === idx);
      });
      if (codeTitleEl) codeTitleEl.textContent = u.name;
      if (scopeBadgeEl) scopeBadgeEl.textContent = u.scope;
      codeDisplayEl.innerHTML = highlightPlsql(u.code);
    };

    unitsListEl.querySelectorAll('.report-unit-item').forEach(el => {
      el.addEventListener('click', () => {
        const idx = parseInt(el.getAttribute('data-rep-unit-idx'), 10) || 0;
        selectUnit(idx);
      });
    });

    if (units.length > 0) {
      selectUnit(0);
    }
  }

  function renderReportDataTree(execution, report = null) {
    if (!reportDataTree) return;
    let html = '<div class="dm-diagram-wrap">';

    // 1. Report Summary Card (clickable to open Report Property Sheet)
    html += `<div class="report-summary-box" data-dm-node="REPORT" style="cursor:pointer;" title="Click to inspect Report Properties & Paper Layout in Property Palette">`;
    html += `<h5>📊 ${escapeHtml(execution.report_name)} (${escapeHtml(execution.unit)})</h5>`;
    const sumKeys = Object.keys(execution.report_summaries || {});
    if (sumKeys.length === 0) {
      html += '<div class="report-sum-row"><span class="report-sum-label">No grand summaries</span></div>';
    } else {
      for (const k of sumKeys) {
        const val = execution.report_summaries[k];
        html += `<div class="report-sum-row"><span class="report-sum-label">Σ ${escapeHtml(k)}:</span> <span class="report-sum-value">${typeof val === 'number' ? val.toLocaleString(undefined, {minimumFractionDigits: 2}) : val}</span></div>`;
      }
    }
    html += '</div>';

    // 2. Interactive Data Model Diagram (Queries Q_* -> Groups G_* -> Columns, Formulas CF_*, Summaries CS_*)
    if (report && report.data?.data_sources) {
      for (const ds of report.data.data_sources) {
        html += `<div class="dm-query-block">`;
        html += `<div class="dm-query-pill" data-dm-node="QUERY" data-dm-name="${escapeHtml(ds.name)}" title="Click to inspect & edit SQL Query in Property Palette">SQL: ${escapeHtml(ds.name)}</div>`;
        for (const grp of (ds.groups || [])) {
          const recCount = (execution.group_records?.[grp.name] || []).length;
          html += `<div class="dm-connector-line"></div>`;
          html += `<div class="dm-group-card">`;
          html += `
            <div class="dm-group-header" data-dm-node="GROUP" data-dm-name="${escapeHtml(grp.name)}">
              <span>${escapeHtml(grp.name)}</span>
              <span style="font-size:10px; opacity:0.85;">${recCount} rows</span>
            </div>
            <div class="dm-group-body">
          `;
          for (const col of (grp.data_items || grp.columns || [])) {
            html += `
              <div class="dm-col-row" data-dm-node="COLUMN" data-dm-group="${escapeHtml(grp.name)}" data-dm-name="${escapeHtml(col.name)}">
                <span><span class="dm-col-icon col">${escapeHtml((col.datatype || 'CHR').slice(0, 3).toUpperCase())}</span>${escapeHtml(col.name)}</span>
                <span style="font-size:9.5px; color:var(--text-muted);">${escapeHtml(col.datatype || 'char')}</span>
              </div>
            `;
          }
          for (const f of (grp.formulas || [])) {
            html += `
              <div class="dm-col-row" data-dm-node="FORMULA" data-dm-group="${escapeHtml(grp.name)}" data-dm-name="${escapeHtml(f.name)}" title="Click to inspect & edit PL/SQL Formula Column">
                <span><span class="dm-col-icon fx">f(x)</span><strong>${escapeHtml(f.name)}</strong></span>
                <span style="font-size:9.5px; color:#d8b4fe;">${escapeHtml(f.source || 'PL/SQL')}</span>
              </div>
            `;
          }
          for (const s of (grp.summaries || [])) {
            html += `
              <div class="dm-col-row" data-dm-node="SUMMARY" data-dm-group="${escapeHtml(grp.name)}" data-dm-name="${escapeHtml(s.name)}" title="Click to inspect Summary Column">
                <span><span class="dm-col-icon sum">Σ</span><strong>${escapeHtml(s.name)}</strong></span>
                <span style="font-size:9.5px; color:#6ee7b7;">${escapeHtml(String(s.function || 'SUM'))}(${escapeHtml(s.source)})</span>
              </div>
            `;
          }
          html += `</div></div>`;
        }
        html += `</div>`;
      }
    }

    html += '</div>';
    reportDataTree.innerHTML = html;

    // Bind clicks on Data Model nodes to open their Report Property Sheet in the right-hand Property Palette
    reportDataTree.querySelectorAll('[data-dm-node]').forEach(el => {
      el.addEventListener('click', (e) => {
        e.stopPropagation();
        const kind = el.getAttribute('data-dm-node');
        const name = el.getAttribute('data-dm-name');
        const grpName = el.getAttribute('data-dm-group');
        renderReportPropertyInspector(kind, { report, execution, name, groupName: grpName });
      });
    });
  }

  function renderReportPropertyInspector(kind, ctx) {
    if (!propertyInspector || !ctx?.report) return;
    const rep = ctx.report;
    let title = `Report: ${rep.name}`;
    let html = '';

    if (kind === 'QUERY') {
      const ds = (rep.data?.data_sources || []).find(d => d.name === ctx.name);
      if (ds) {
        title = `Report Query: ${ds.name}`;
        html += `
          <div class="prop-group">
            <div class="prop-group-header">Data Model SQL Query (${escapeHtml(ds.name)})</div>
            <table class="prop-table">
              <tr><td class="prop-name">Query Name</td><td class="prop-val"><strong>${escapeHtml(ds.name)}</strong></td></tr>
              <tr><td class="prop-name">Max Rows</td><td class="prop-val"><input type="number" class="prop-input" id="repPropMaxRows" value="${ds.max_rows || 500}"></td></tr>
              <tr><td class="prop-name">Groups</td><td class="prop-val">${(ds.groups || []).map(g => g.name).join(', ')}</td></tr>
            </table>
            <div style="padding: 8px;">
              <label style="font-size:11px; color:var(--text-muted); display:block; margin-bottom:4px;">SQL SELECT Statement:</label>
              <textarea id="repPropSqlText" class="prop-input" rows="6" style="font-family:var(--font-mono); font-size:11px;">${escapeHtml(ds.select_sql || '')}</textarea>
              <button class="btn btn-sm btn-primary" id="repPropApplySqlBtn" style="width:100%; margin-top:6px;">💾 Update SQL &amp; Preview</button>
            </div>
          </div>
        `;
      }
    } else if (kind === 'FORMULA') {
      let foundFormula = null;
      for (const ds of (rep.data?.data_sources || [])) {
        for (const g of (ds.groups || [])) {
          const f = (g.formulas || []).find(x => x.name === ctx.name);
          if (f) foundFormula = f;
        }
      }
      if (foundFormula) {
        title = `Formula Column: ${foundFormula.name}`;
        html += `
          <div class="prop-group">
            <div class="prop-group-header">PL/SQL Formula Column (${escapeHtml(foundFormula.name)})</div>
            <table class="prop-table">
              <tr><td class="prop-name">Column Name</td><td class="prop-val"><strong>${escapeHtml(foundFormula.name)}</strong></td></tr>
              <tr><td class="prop-name">Group</td><td class="prop-val">${escapeHtml(ctx.groupName || '')}</td></tr>
              <tr><td class="prop-name">Data Type</td><td class="prop-val">${escapeHtml(foundFormula.datatype || 'number')}</td></tr>
            </table>
            <div style="padding: 8px;">
              <label style="font-size:11px; color:var(--text-muted); display:block; margin-bottom:4px;">PL/SQL Formula Expression:</label>
              <textarea id="repPropFormulaText" class="prop-input" rows="4" style="font-family:var(--font-mono); font-size:11px;">${escapeHtml(foundFormula.source || foundFormula.plsql || '')}</textarea>
              <button class="btn btn-sm btn-primary" id="repPropApplyFormulaBtn" style="width:100%; margin-top:6px;">💾 Update Formula</button>
            </div>
          </div>
        `;
      }
    } else if (kind === 'SUMMARY') {
      let foundSum = null;
      for (const ds of (rep.data?.data_sources || [])) {
        for (const g of (ds.groups || [])) {
          const s = (g.summaries || []).find(x => x.name === ctx.name);
          if (s) foundSum = s;
        }
      }
      if (foundSum) {
        title = `Summary Column: ${foundSum.name}`;
        html += renderPropGroup('Summary Column Properties', [
          ['Name', foundSum.name],
          ['Function', String(foundSum.function || 'SUM').toUpperCase()],
          ['Source Column', foundSum.source],
          ['Reset At Group', foundSum.reset || foundSum.reset_at || ctx.groupName || 'Report']
        ]);
      }
    } else {
      title = `Report: ${rep.name}`;
      const allSums = [];
      for (const ds of (rep.data?.data_sources || [])) {
        for (const g of (ds.groups || [])) {
          for (const s of (g.summaries || [])) {
            allSums.push(`${s.name} = ${String(s.function || 'SUM').toUpperCase()}(${s.source})`);
          }
        }
      }
      html += `
        <div class="prop-group">
          <div class="prop-group-header">Oracle Report Module Properties</div>
          <table class="prop-table">
            <tr><td class="prop-name">Report Name</td><td class="prop-val"><strong>${escapeHtml(rep.name)}</strong></td></tr>
            <tr><td class="prop-name">Report Title</td><td class="prop-val"><input type="text" class="prop-input" id="repPropTitleInput" value="${escapeHtml(rep.title || '')}"></td></tr>
            <tr><td class="prop-name">Unit</td><td class="prop-val">${escapeHtml(rep.unit_of_measurement || 'inch')}</td></tr>
            <tr><td class="prop-name">Data Queries</td><td class="prop-val">${(rep.data?.data_sources || []).map(d => d.name).join(', ')}</td></tr>
            <tr><td class="prop-name">Data Links</td><td class="prop-val">${(rep.data?.links || []).map(l => `${l.name || 'L_LINK'} (${l.parent_group} → ${l.child_query || l.child_group})`).join(', ') || '(Single Group)'}</td></tr>
            <tr><td class="prop-name">Summaries</td><td class="prop-val">${allSums.join(', ') || '(None)'}</td></tr>
          </table>
          <div style="padding: 8px; display:flex; gap:6px;">
            <button class="btn btn-sm btn-primary" id="repPropApplyTitleBtn" style="flex:1;">💾 Update Report Title</button>
            <button class="btn btn-sm btn-secondary" onclick="document.getElementById('exportReportXmlBtn')?.click()">💾 Export XML</button>
          </div>
        </div>
      `;
    }

    inspectorTargetName.textContent = title;
    propertyInspector.innerHTML = html;

    document.getElementById('repPropApplyTitleBtn')?.addEventListener('click', () => {
      const newTitle = document.getElementById('repPropTitleInput')?.value || rep.title;
      rep.title = newTitle;
      if (currentReportPayload?.html && reportPreviewFrame) {
        currentReportPayload.html = currentReportPayload.html.replace(/<div class="report-title">[\s\S]*?<\/div>/, `<div class="report-title">${escapeHtml(newTitle)}</div>`);
        const theme = document.getElementById('reportTemplateSelect')?.value || 'blue';
        reportPreviewFrame.srcdoc = applyReportTemplateTheme(currentReportPayload.html, theme);
      }
      showBanner(`Updated Report Title to "${newTitle}".`);
      setTimeout(hideBanner, 3000);
    });

    document.getElementById('repPropApplySqlBtn')?.addEventListener('click', () => {
      const ds = (rep.data?.data_sources || []).find(d => d.name === ctx.name);
      if (ds) {
        ds.select_sql = document.getElementById('repPropSqlText')?.value || ds.select_sql;
        ds.max_rows = parseInt(document.getElementById('repPropMaxRows')?.value, 10) || ds.max_rows;
        renderReportPlsqlStudio(rep, ctx.execution);
        showBanner(`Updated SQL Query '${ds.name}' in Report model — click 💾 Export XML to save.`);
        setTimeout(hideBanner, 3200);
      }
    });

    document.getElementById('repPropApplyFormulaBtn')?.addEventListener('click', () => {
      for (const ds of (rep.data?.data_sources || [])) {
        for (const g of (ds.groups || [])) {
          const f = (g.formulas || []).find(x => x.name === ctx.name);
          if (f) {
            f.plsql = document.getElementById('repPropFormulaText')?.value || f.plsql;
            renderReportPlsqlStudio(rep, ctx.execution);
            showBanner(`Updated PL/SQL Formula '${f.name}' — click 💾 Export XML to save.`);
            setTimeout(hideBanner, 3200);
          }
        }
      }
    });
  }
})();
