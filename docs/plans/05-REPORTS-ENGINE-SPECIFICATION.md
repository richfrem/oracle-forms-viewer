# 05: Oracle Reports Engine (Research Spike Specification)

> **Document Status**: Research Spike Specification (Phase G)  
> **Target Persona**: Analyst / Modernizer  
> **Key Objective**: Investigate the feasibility of decoding Oracle Reports `.rdf` binary streams versus Reports XML exports before committing to rendering engine development.

---

## 1. Background & Problem Statement

Oracle officially deprecated Oracle Reports starting with Fusion Middleware 12c and omitted it from standalone distributions. Many legacy Oracle Forms applications invoke reports programmatically:
```plsql
report_id := FIND_REPORT_OBJECT('INVOICE_REPORT');
SET_REPORT_OBJECT_PROPERTY(report_id, REPORT_OTHER, 'p_order_id=' || :orders.order_id);
report_job_id := RUN_REPORT_OBJECT(report_id);
```

While adding a lightweight reports preview capability would provide value for modernization analysis, binary `.rdf` files present distinct reverse-engineering challenges compared to `.fmb` files.

---

## 2. Research Spike Protocol (Step 1 Gate)

Before scheduling any rendering or calculation engine development, Phase G commences with an **investigative research spike**:

### Step 1: Ingestion Feasibility Assessment
1. **Binary Container Analysis (`.rdf`)**:
   - Verify whether `.rdf` binary streams follow identical ROS 60050 chunk structures as `.fmb` files.
   - Determine whether query data models, group definitions, and repeating frame layouts can be recovered from raw byte streams without proprietary Oracle Reports Builder libraries.
2. **Reports XML Exports**:
   - Test Oracle 12c/14c `rwconverter` headless XML exports for Reports (`stype=rdffile dtype=xmlfile`).
   - Assess whether Reports XML provides a complete, lossless data model and layout tree.
3. **Go / No-Go Gate**:
   - If binary `.rdf` decoding proves excessively brittle, restrict the supported ingestion format to **Reports XML**, or document `.rdf` conversion via the existing testbed skill (`converting-oracle-artifacts/scripts/convert_rpt.py`).

---

## 3. Reports Engine Pipeline (Subject to Spike Validation)

Assuming Step 1 proves feasible:

### 3.1 Data Model Evaluation
1. **Query Execution**: Executes report SQL queries via `oracle-forms-db`.
2. **Data Groups**: Builds hierarchical master-detail groups (e.g. `G_ORDERS` master group, `G_ORDER_ITEMS` detail group).
3. **Formula & Summary Columns**:
   - `Formula Columns` (`CF_`): Evaluated per record.
   - `Summary Columns` (`CS_`): Running sums, counts, and averages reset per group.

### 3.2 Layout Model Engine
- **Repeating Frames (`R_`)**: Dynamic vertical repetition per record.
- **Fields (`F_`)**: Bound to group columns with currency and date format masks.
- **Boilerplate (`B_`)**: Static labels, borders, and embedded graphics.

### 3.3 Output Renderers
- **HTML5 Paged Media**: Semantic HTML5 tables utilizing CSS Paged Media (`@page { size: letter portrait; margin: 0.5in; }`).
- **Vectorized PDF**: Direct PDF output via `printpdf` with embedded standard fonts.
