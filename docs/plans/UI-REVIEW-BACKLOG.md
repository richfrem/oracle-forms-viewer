# UI review backlog

Outstanding items from an earlier review of the workbench UI. They were the last section of `docs/PLUGINS-ORACLE-FORMS-BACKLOG.md`, a planning document for the Oracle plugin that was superseded by the skills themselves and removed on 2026-11-14. Only the items below were still open.

Status checked 2026-11-14 by searching the source; none was checked in a running browser, so "outstanding" means the code shows no sign of the fix.

| # | Item | Evidence it is still open |
|---|---|---|
| 1 | Remove `overflow-x` on `.top-bar` and `.canvas-toolbar`; compact the header controls at 1280 px width | `ui/styles.css` line 59: `.top-bar` still has `overflow-x: auto`. There is no rule named `.canvas-toolbar`; the nearest, `.canvas-controls` (line 631), also has `overflow-x: auto` |
| 2 | Filter the `<New_Item>` placeholder menu items when extracting binary `.mmb` files | No reference to `New_Item` anywhere in `crates/` or `ui/` (Oracle's Forms2XML output does contain `Label="<New_Item>"` for separators, so the placeholder is real) |
| 3 | Cache-busting or `no-store` headers for the embedded UI assets | No `Cache-Control` or `no-store` in `crates/oracle-forms-server/src` |
| 4 | A standalone `.mmb` inspector canvas (a menu tree) instead of the stale form canvas | No menu-tree rendering in `ui/app.js`; the README also lists a standalone menu canvas as outstanding |

Related idea (not from the original review, added 2026-11-14): a status strip in the header showing the Forms server (WebLogic) host and state, the database host and state, and the Rust app, with a "last checked" time. It needs a light server endpoint with short timeouts (the existing Oracle readiness API timed out at 3 seconds while WebLogic was booting), polled in the background, and the server must do the probing, since on Windows the browser is on Windows and the containers are in WSL.
