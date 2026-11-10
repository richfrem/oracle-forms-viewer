# Oracle Forms Runtime Feature Catalog & Parity Mapping

Reference guide detailing supported, partial, and unsupported Oracle Forms features when comparing Oracle Fusion Middleware 14c against native Rust Oracle Forms Workbench (OFW).

---

## 1. Supported Built-ins (`oracle-forms-runtime`)

| Built-in | Category | Behavior in Rust OFW |
| :--- | :--- | :--- |
| `GO_ITEM(item_name)` | Navigation | Navigates cursor to item, fires validation and WHEN-NEW-ITEM-INSTANCE |
| `GO_BLOCK(block_name)` | Navigation | Navigates to block, updates SYSTEM.CURSOR_BLOCK |
| `GO_RECORD(record_num)` | Navigation | Jumps to specific record position (1-based index) |
| `NEXT_ITEM` / `PREVIOUS_ITEM` | Navigation | Advances or rewinds item focus in canvas tab order |
| `NEXT_RECORD` / `PREVIOUS_RECORD` | Navigation | Moves to adjacent record in block buffer |
| `FIRST_RECORD` / `LAST_RECORD` | Navigation | Navigates to bounds of queried record set |
| `EXECUTE_QUERY` | Lifecycle | Executes block query against database backend |
| `ENTER_QUERY` | Lifecycle | Enters query-by-example (QBE) mode |
| `CREATE_RECORD` | Lifecycle | Inserts new dirty record into active block buffer |
| `DELETE_RECORD` | Lifecycle | Marks active record for deletion during commit |
| `CLEAR_RECORD` | Lifecycle | Reverts current record buffer |
| `CLEAR_BLOCK` | Lifecycle | Flushes record cache of target block |
| `CLEAR_FORM` | Lifecycle | Resets entire form state and all block buffers |
| `COMMIT_FORM` | Lifecycle | Executes atomic commit pipeline with validation & savepoints |
| `EXIT_FORM` | Lifecycle | Terminates interactive session |
| `FIND_ALERT` | Dialogs | Resolves alert object definition |
| `SHOW_ALERT` | Dialogs | Displays 3-button modal with resumable continuation |
| `SET_ALERT_PROPERTY` | Dialogs | Dynamic title and message text updates |
| `SET_ALERT_BUTTON_PROPERTY` | Dialogs | Dynamic button label overrides |
| `SET_ITEM_PROPERTY` | Properties | Toggles ENABLED, VISIBLE, LABEL, colors, etc. |
| `GET_ITEM_PROPERTY` | Properties | Reads runtime item state and attributes |
| `SET_BLOCK_PROPERTY` | Properties | Overrides DEFAULT_WHERE and ORDER_BY clauses |
| `GET_BLOCK_PROPERTY` | Properties | Reads block status and properties |
| `SET_WINDOW_PROPERTY` | Properties | Updates window title and dimensions |
| `GET_WINDOW_PROPERTY` | Properties | Reads window dimensions |
| `SET_CANVAS_PROPERTY` | Properties | Toggles canvas visibility |
| `MESSAGE` | UI | Logs informational message to console / banner |
| `SELECT ... INTO ...` | Embedded SQL | Positional bind mapping with NO_DATA_FOUND detection |
| `SAVEPOINT` / `ROLLBACK` | Transaction | Transaction demarcation with named savepoints |

---

## 2. Supported Event Triggers

| Trigger Event | Timing & Trigger Conditions |
| :--- | :--- |
| `WHEN-BUTTON-PRESSED` | User clicks a push button item |
| `WHEN-VALIDATE-ITEM` | Field loses focus or value changes |
| `WHEN-VALIDATE-RECORD` | Record navigation or commit occurs |
| `WHEN-NEW-FORM-INSTANCE` | Form session initializes |
| `WHEN-NEW-BLOCK-INSTANCE` | Focus moves to a different block |
| `WHEN-NEW-RECORD-INSTANCE` | Focus moves to a different record |
| `WHEN-NEW-ITEM-INSTANCE` | Focus moves to a different item |
| `PRE-QUERY` / `POST-QUERY` | Fires before SQL generation and after each row is fetched |
| `ON-SELECT` | Custom query override trigger |
| `PRE-COMMIT` / `ON-COMMIT` | Validation and commit processing phases |
| `POST-COMMIT` / `POST-DATABASE-COMMIT` | Fires after successful database transaction commit |
| `ON-INSERT` / `ON-UPDATE` / `ON-DELETE` | Custom DML record handlers |
| `ON-LOCK` | Pessimistic locking override |
| `ON-ERROR` / `ON-MESSAGE` | Custom error and message trapping |
| `KEY-EXEQRY` / `KEY-ENTQRY` | Standard keyboard shortcut triggers |
| `KEY-COMMIT` / `KEY-EXIT` | Keyboard commit and exit handlers |
| `KEY-NEXTREC` / `KEY-PREVREC` | Keyboard record navigation |

---

## 3. Supported System Variables

- `SYSTEM.CURSOR_BLOCK`
- `SYSTEM.CURSOR_ITEM`
- `SYSTEM.CURSOR_RECORD`
- `SYSTEM.CURSOR_VALUE`
- `SYSTEM.TRIGGER_BLOCK`
- `SYSTEM.TRIGGER_ITEM`
- `SYSTEM.TRIGGER_RECORD`
- `SYSTEM.BLOCK_STATUS` (`NEW`, `INSERT`, `CHANGED`, `QUERY`)
- `SYSTEM.RECORD_STATUS` (`NEW`, `INSERT`, `CHANGED`, `QUERY`)
- `SYSTEM.FORM_STATUS` (`NEW`, `CHANGED`, `QUERY`)
- `SYSTEM.MODE` (`NORMAL`, `ENTER-QUERY`)
- `SYSTEM.FORM_SUCCESS` / `SYSTEM.FORM_FAILURE` / `SYSTEM.FORM_FATAL`
- `SYSTEM.LAST_QUERY`

---

## 4. Unsupported or Deprecated Features

The following features exist in legacy Oracle Forms but are deliberately not implemented in the native Rust stack due to security sandboxing, deprecation, or replacement with web standards:

| Feature | Legacy Context | Modern Replacement |
| :--- | :--- | :--- |
| `HOST` | OS command execution | REST API or containerized worker tasks |
| `RUN_PRODUCT` / `RUN_REPORT_OBJECT` | Oracle Reports 6i/12c launcher | Native HTML5 Reports engine (`oracle-forms-server/reports`) |
| `OLE2.*` / `DDE.*` / `D2KWUTIL.*` | Windows 32-bit Win32/COM automation | Modern Web APIs and server-side services |
| `TEXT_IO` / `CLIENT_TEXT_IO` | Server/client filesystem reads | Standard browser file uploads and downloads |
| `WEBUTIL` | Java applet client desktop bridge | Native browser HTML5 capabilities |
| `CALL_FORM` / `OPEN_FORM` | Multi-form window stack | Multi-tab / single-page application routing |
