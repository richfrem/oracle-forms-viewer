# 05: Oracle Reports Engine (Research Spike & Architecture Specification)

> **Document Status**: Research Spike Completed / Architecture Approved (Phase G)  
> **Target Persona**: Analyst / Modernizer  
> **Key Objective**: Deliver native report ingestion, data model evaluation, and responsive HTML5 paged media rendering for Oracle Reports definitions.

---

## 1. Background & Problem Statement

Oracle officially deprecated Oracle Reports starting with Fusion Middleware 12c and omitted it from standalone distributions. Many legacy Oracle Forms applications invoke reports programmatically:
```plsql
report_id := FIND_REPORT_OBJECT('INVOICE_REPORT');
SET_REPORT_OBJECT_PROPERTY(report_id, REPORT_OTHER, 'p_order_id=' || :orders.order_id);
report_job_id := RUN_REPORT_OBJECT(report_id);
```

While adding a lightweight reports preview capability provides critical modernization insights, binary `.rdf` files present distinct reverse-engineering challenges compared to `.fmb` files.

---

## 2. Research Spike Findings & Assessment (Step 1 Gate)

The Phase G research spike evaluated two primary ingestion paths:

### 2.1 Binary Container Analysis (`.rdf` and `.tdf`)
- **Container Format**: Direct inspection of Oracle Reports template binaries (e.g. `rwbeige.tdf`, `rwblue.tdf`) confirms that Oracle Reports binaries share the exact **`ROS.60050`** compound storage header format with Oracle Forms `.fmb` and `.olb` files.
- **PL/SQL & String Recovery**: Our existing `oracle-forms-binary` heuristic scanner successfully extracts embedded Program Units and functions (e.g., `NEW_PAGE`, `BACK`, `FORWARD`, `PAGENUM` from `rwbeige.tdf`).
- **Layout & Data Model Limitations**: Unlike Forms FMBs where item geometries and block properties follow consistent property-ID tuples, Reports layout matrices (repeating frames, elastic sizing, cross-product matrix groupings, page anchors) are encoded in internal C-runtime memory structures (`librw.so`). Without proprietary Oracle Reports Builder libraries, deserializing raw binary layout streams is brittle and prone to structural drift.

### 2.2 Reports XML Exports (`reports.dtd`)
- **Schema Completeness**: Oracle publishes a complete, standardized Document Type Definition (`reports.dtd`) spanning 1,688 lines.
- **Lossless Representation**: Reports XML completely captures:
  - `<dataSource>` queries with full SQL `SELECT` statements.
  - Hierarchical `<group>` models with `<dataItem>` types, scale, and precision.
  - Inter-query `<link>` elements with `parentGroup`, `parentColumn`, `childQuery`, `childColumn`, and `condition`.
  - Summary columns (`<summary>` with functions `sum`, `average`, `count`, `min`, `max`, `percentOfTotal`, and `reset` boundaries: `report`, `page`, or group).
  - Formula columns (`<formula>` with computation source expressions).
  - Complete `<layout>` trees with `<section>`, `<body>`, `<margin>`, `<repeatingFrame>` (print direction `down`/`across`), `<frame>`, `<field>` (format masks, sources), and `<geometryInfo>` coordinates (`x`, `y`, `width`, `height`).

### 2.3 Spike Conclusion & Go / No-Go Decision
- **Decision**: **Pure-native Reports XML parsing** in `oracle-forms-xml` with AST in `oracle-forms-core`, paired with:
  1. Direct binary text/code extraction heuristics for `.rdf` via `oracle-forms-binary`.
  2. Batch converter bridge via `plugins/oracle-forms/.../convert_rpt.py` utilizing the containerized Oracle Home for headless `.rdf` $\leftrightarrow$ XML conversion.
  3. Native Rust Data Model Evaluator & HTML5 Paged Media Renderer in `oracle-forms-runtime`.

---

## 3. Reports Engine Pipeline

### 3.1 Data Model Evaluation (`ReportDataEvaluator`)
1. **Query Execution**: Executes report SQL queries via `DatabaseDriver` (Oracle 23ai testbed, SQLite, or Mock).
2. **Data Groups & Linking**: Resolves master-detail relationships across queries (e.g., `G_DEPT` master linked to `G_EMP` detail by `DEPARTMENT_ID`).
3. **Formula & Summary Columns**:
   - `Formula Columns` (`CF_`): Evaluated per record row.
   - `Summary Columns` (`CS_`): Accumulates running sums, counts, averages, and percentages of total, resetting cleanly at designated group or report boundaries.

### 3.2 Layout Model Engine (`ReportRenderer`)
- **Repeating Frames (`R_`)**: Dynamic vertical repetition per record row, preserving hierarchical nesting.
- **Fields (`F_`)**: Bound to group columns or summaries with currency (`$#,##0.00`), integer, and date format masks.
- **Boilerplate (`B_`)**: Static report headers, column labels, borders, and company titles.

### 3.3 Output Rendering
- **HTML5 Paged Media**: Semantic HTML5 document utilizing CSS Paged Media:
  ```css
  @page {
    size: letter portrait;
    margin: 0.5in;
    @bottom-right { content: "Page " counter(page); }
  }
  ```
- **Print & PDF Support**: Direct browser print dialog triggering (`window.print()`), responsive preview iframe in web viewer, and standalone CLI HTML export.
