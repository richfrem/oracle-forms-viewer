use crate::model::{BoundSql, DbBackend, DbError, SqlValue};
use oracle_forms_core::model::{BlockInfo, ItemInfo};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GeneratedQuery {
    pub table_name: String,
    pub sql: String,
    pub bound_sql: BoundSql,
    pub selected_columns: Vec<String>,
    pub column_to_item: Vec<(String, String)>, // (ColumnName, ItemName)
    /// Forms references (e.g. `:ORDERS.CUSTOMER_ID`) found in the block's WHERE clause,
    /// in placeholder order. Their values in `bound_sql` start as NULL and must be filled
    /// with [`GeneratedQuery::resolve_binds`] before execution.
    pub bind_refs: Vec<BindRef>,
}

impl GeneratedQuery {
    /// Fills every bind reference using `resolver`. Fails if any reference is unknown,
    /// rather than silently querying with NULL.
    pub fn resolve_binds<F>(&mut self, mut resolver: F) -> Result<(), DbError>
    where
        F: FnMut(&str) -> Option<SqlValue>,
    {
        for r in &self.bind_refs {
            let value = resolver(&r.reference).ok_or_else(|| {
                DbError::QueryError(format!("Unresolved bind reference :{} in block WHERE clause", r.reference))
            })?;
            if let Some(slot) = self.bound_sql.binds.iter_mut().find(|(name, _)| *name == r.placeholder) {
                slot.1 = value;
            }
        }
        self.bind_refs.clear();
        Ok(())
    }
}

/// One Forms bind reference rewritten to a dialect placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRef {
    /// Placeholder as it appears in the rewritten SQL and in `BoundSql.binds`
    /// (`:b1` for Oracle, `?1` for SQLite).
    pub placeholder: String,
    /// The original Forms reference, upper-cased, without the colon (`BLOCK.ITEM`).
    pub reference: String,
}

/// Result of [`rewrite_binds`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenSql {
    pub sql: String,
    pub refs: Vec<BindRef>,
}

impl RewrittenSql {
    /// Binds with NULL values, keyed by placeholder, ready to be filled.
    pub fn null_binds(&self) -> Vec<(String, SqlValue)> {
        self.refs.iter().map(|r| (r.placeholder.clone(), SqlValue::Null)).collect()
    }
}

/// Rewrites Forms-style bind references in `sql` into dialect placeholders.
///
/// Recognises `:name` and `:a.b` references in any case. Leaves alone: string literals
/// (`'..''..'`), alternative quoting (`q'[..]'`, `q'{..}'`, etc.), quoted identifiers
/// (`"a:b"`), `--` and `/* */` comments, and the PL/SQL assignment operator `:=`.
/// Placeholders are numbered from `start_index`.
pub fn rewrite_binds(sql: &str, backend: DbBackend, start_index: usize) -> RewrittenSql {
    let chars: Vec<char> = sql.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(sql.len());
    let mut refs = Vec::new();
    let mut counter = start_index;
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        // q'X...X' alternative quoting (Oracle). The closing delimiter pairs ( [ { <.
        if (ch == 'q' || ch == 'Q')
            && next == Some('\'')
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
        {
            if let Some(&open) = chars.get(i + 2) {
                let close = match open {
                    '[' => ']',
                    '(' => ')',
                    '{' => '}',
                    '<' => '>',
                    other => other,
                };
                let mut j = i + 3;
                while j + 1 < len && !(chars[j] == close && chars[j + 1] == '\'') {
                    j += 1;
                }
                let end = (j + 2).min(len);
                out.extend(&chars[i..end]);
                i = end;
                continue;
            }
        }

        match ch {
            '\'' | '"' => {
                // Literal or quoted identifier; a doubled quote is an escaped quote.
                out.push(ch);
                i += 1;
                while i < len {
                    out.push(chars[i]);
                    if chars[i] == ch {
                        if chars.get(i + 1) == Some(&ch) {
                            out.push(ch);
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '-' if next == Some('-') => {
                while i < len && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                let start = i;
                i += 2;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                let end = (i + 2).min(len);
                out.extend(&chars[start..end]);
                i = end;
            }
            ':' if next == Some('=') => {
                out.push_str(":=");
                i += 2;
            }
            ':' if next.map(|c| c.is_alphabetic() || c == '_' || c == '$').unwrap_or(false) => {
                i += 1;
                let mut name = String::new();
                while i < len {
                    let c = chars[i];
                    let continues_name = c.is_alphanumeric()
                        || matches!(c, '_' | '$' | '#')
                        || (c == '.' && chars.get(i + 1).map(|n| n.is_alphabetic() || *n == '_').unwrap_or(false));
                    if !continues_name {
                        break;
                    }
                    name.push(c);
                    i += 1;
                }
                let placeholder = match backend {
                    DbBackend::Sqlite => format!("?{}", counter),
                    _ => format!(":b{}", counter),
                };
                out.push_str(&placeholder);
                refs.push(BindRef {
                    placeholder,
                    reference: name.to_ascii_uppercase(),
                });
                counter += 1;
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    RewrittenSql { sql: out, refs }
}

pub struct SqlGenerator;

impl SqlGenerator {
    /// Rewrites Oracle Forms bind references (`:BLOCK.ITEM`, `:ITEM`, `:GLOBAL.X`,
    /// `:PARAMETER.X`, `:SYSTEM.X`) into dialect placeholders. See [`rewrite_binds`].
    pub fn rewrite_where_binds(where_clause: &str, backend: DbBackend) -> RewrittenSql {
        rewrite_binds(where_clause, backend, 1)
    }

    /// Generates a dialect-aware SELECT query from a Form Block definition.
    pub fn generate_select(
        block: &BlockInfo,
        backend: DbBackend,
        limit: Option<usize>,
    ) -> Result<GeneratedQuery, DbError> {
        // 1. Resolve table / data source name. A control block (no data source) cannot be
        //    queried; its items are filled by triggers instead.
        let table_name = block
            .query_data_source_name
            .as_deref()
            .or_else(|| {
                block
                    .dml
                    .as_ref()
                    .and_then(|d| d.dml_data_target_name.as_deref())
            })
            .unwrap_or("")
            .trim()
            .to_string();

        if table_name.is_empty() {
            return Err(DbError::MissingDataSource);
        }

        // 2. Identify candidate database columns from items
        let mut selected_columns = Vec::new();
        let mut column_to_item = Vec::new();

        let has_explicit_columns = block.items.iter().any(|i| {
            i.column_name.is_some()
                || i.dml.as_ref().and_then(|d| d.database_item) == Some(true)
        });

        for item in &block.items {
            let is_non_data = item
                .item_type
                .as_deref()
                .map(|s| {
                    s.eq_ignore_ascii_case("Push Button")
                        || s.eq_ignore_ascii_case("Radio Group")
                        || s.eq_ignore_ascii_case("Check Box")
                })
                .unwrap_or(false);
            if is_non_data {
                continue;
            }

            let col_name_opt = item
                .column_name
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            if has_explicit_columns {
                if let Some(col) = col_name_opt {
                    let col_upper = col.to_ascii_uppercase();
                    let clean_col = if let Some((_prefix, actual_col)) = col_upper.split_once('.') {
                        actual_col.to_string()
                    } else {
                        col_upper.clone()
                    };
                    if !selected_columns.contains(&clean_col) {
                        selected_columns.push(clean_col.clone());
                    }
                    column_to_item.push((clean_col, item.name.clone()));
                }
            } else {
                let item_col = item.name.to_ascii_uppercase();
                if !selected_columns.contains(&item_col) {
                    selected_columns.push(item_col.clone());
                }
                column_to_item.push((item_col, item.name.clone()));
            }
        }

        // Eliminate SELECT *: if no database columns exist, return typed error
        if selected_columns.is_empty() {
            return Err(DbError::NoDatabaseColumns(block.name.clone()));
        }

        // Prepend hidden ROWID for Oracle backend
        if backend == DbBackend::Oracle || backend == DbBackend::OracleThin {
            selected_columns.insert(0, "ROWID".to_string());
        }

        let select_list = selected_columns.join(", ");

        // 3. Assemble base query
        let mut sql = format!("SELECT {} FROM {}", select_list, table_name);
        let mut binds = Vec::new();
        let mut bind_refs: Vec<BindRef> = Vec::new();

        // 4. Append WHERE clause if present and rewrite binds
        if let Some(where_clause) = block
            .dml
            .as_ref()
            .and_then(|d| d.where_clause.as_deref())
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
        {
            let unescaped_where = where_clause
                .replace("&amp;#10;", " ")
                .replace("&amp;#13;", " ")
                .replace("&#10;", " ")
                .replace("&#13;", " ")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace(['\n', '\r'], " ");
            let clean_where = unescaped_where
                .strip_prefix("WHERE ")
                .or_else(|| unescaped_where.strip_prefix("where "))
                .unwrap_or(&unescaped_where)
                .trim();

            let rewritten = Self::rewrite_where_binds(clean_where, backend);
            sql.push_str(&format!(" WHERE {}", rewritten.sql));
            binds.extend(rewritten.null_binds());
            bind_refs.extend(rewritten.refs);
        }

        // 5. Append ORDER BY clause if present
        if let Some(order_clause) = block
            .dml
            .as_ref()
            .and_then(|d| d.order_by_clause.as_deref())
            .map(|o| o.trim())
            .filter(|o| !o.is_empty())
        {
            let unescaped_order = order_clause
                .replace("&amp;#10;", " ")
                .replace("&amp;#13;", " ")
                .replace("&#10;", " ")
                .replace("&#13;", " ")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace(['\n', '\r'], " ");
            let clean_order = unescaped_order
                .strip_prefix("ORDER BY ")
                .or_else(|| unescaped_order.strip_prefix("order by "))
                .unwrap_or(&unescaped_order)
                .trim();
            sql.push_str(&format!(" ORDER BY {}", clean_order));
        }

        // 6. Append dialect-specific limit clause
        if let Some(lim) = limit {
            match backend {
                DbBackend::Oracle | DbBackend::OracleThin => {
                    sql.push_str(&format!(" FETCH FIRST {} ROWS ONLY", lim));
                }
                DbBackend::Sqlite | DbBackend::Postgres | DbBackend::Mock => {
                    sql.push_str(&format!(" LIMIT {}", lim));
                }
            }
        }

        let bound_sql = BoundSql {
            sql: sql.clone(),
            binds,
        };

        Ok(GeneratedQuery {
            table_name,
            sql,
            bound_sql,
            selected_columns,
            column_to_item,
            bind_refs,
        })
    }

    /// Table that DML and row locks target: the block's DML Data Target Name if set,
    /// otherwise its Query Data Source Name. Control blocks have neither.
    pub fn dml_table(block: &BlockInfo) -> Result<String, DbError> {
        block
            .dml
            .as_ref()
            .and_then(|d| d.dml_data_target_name.clone())
            .or_else(|| block.query_data_source_name.clone())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or(DbError::MissingDataSource)
    }

    /// Checks whether an item qualifies as a database item (not non-data or UI buttons).
    pub fn is_database_item(item: &ItemInfo, has_explicit_columns: bool) -> bool {
        if let Some(dml) = &item.dml {
            if dml.database_item == Some(false) {
                return false;
            }
        }
        if let Some(itype) = &item.item_type {
            if itype.eq_ignore_ascii_case("Push Button")
                || itype.eq_ignore_ascii_case("Button")
            {
                return false;
            }
        }
        if has_explicit_columns {
            item.column_name.is_some()
                || item.dml.as_ref().and_then(|d| d.database_item) == Some(true)
        } else {
            true
        }
    }

    /// Generates genuine Forms SELECT ... FOR UPDATE NOWAIT row lock statement.
    pub fn generate_lock(
        block: &BlockInfo,
        backend: DbBackend,
        row_id: &str,
    ) -> Result<BoundSql, DbError> {
        let table_name = Self::dml_table(block)?;

        let has_explicit_columns = block.items.iter().any(|i| {
            i.column_name.is_some()
                || i.dml.as_ref().and_then(|d| d.database_item) == Some(true)
        });

        let mut columns = Vec::new();
        for item in &block.items {
            if !Self::is_database_item(item, has_explicit_columns) {
                continue;
            }
            if item.dml.as_ref().and_then(|d| d.query_allowed) == Some(false) {
                continue;
            }
            let col = item.column_name.clone().unwrap_or_else(|| item.name.clone());
            if !columns.contains(&col) {
                columns.push(col);
            }
        }
        if columns.is_empty() {
            columns.push("ROWID".to_string());
        }

        let cols_joined = columns.join(", ");
        let (sql, binds) = match backend {
            DbBackend::Oracle | DbBackend::OracleThin => (
                format!(
                    "SELECT {} FROM {} WHERE ROWID = :b1 FOR UPDATE NOWAIT",
                    cols_joined, table_name
                ),
                vec![(":b1".to_string(), SqlValue::Text(row_id.to_string()))],
            ),
            _ => (
                format!("SELECT {} FROM {} WHERE ROWID = ?", cols_joined, table_name),
                vec![("?1".to_string(), SqlValue::Text(row_id.to_string()))],
            ),
        };

        Ok(BoundSql { sql, binds })
    }

    /// Generates parameterized INSERT statement for a new record.
    pub fn generate_insert(
        block: &BlockInfo,
        backend: DbBackend,
        values: &HashMap<String, serde_json::Value>,
    ) -> Result<BoundSql, DbError> {
        let table_name = Self::dml_table(block)?;

        let has_explicit_columns = block.items.iter().any(|i| {
            i.column_name.is_some()
                || i.dml.as_ref().and_then(|d| d.database_item) == Some(true)
        });

        let mut cols = Vec::new();
        let mut binds = Vec::new();
        let mut placeholders = Vec::new();
        let mut bind_idx = 1;

        for item in &block.items {
            if !Self::is_database_item(item, has_explicit_columns) {
                continue;
            }
            if item.dml.as_ref().and_then(|d| d.insert_allowed) == Some(false) {
                continue;
            }
            let col = item.column_name.clone().unwrap_or_else(|| item.name.clone());
            let item_name = item.name.clone();

            if let Some(val) = values.get(&item_name).or_else(|| values.get(&col)) {
                if !val.is_null() {
                    cols.push(col);
                    let sql_val = match val {
                        serde_json::Value::Number(num) => SqlValue::Number(num.to_string()),
                        serde_json::Value::String(s) => SqlValue::Text(s.clone()),
                        serde_json::Value::Bool(b) => SqlValue::Boolean(*b),
                        _ => SqlValue::Text(val.to_string()),
                    };

                    match backend {
                        DbBackend::Oracle | DbBackend::OracleThin => {
                            let placeholder = format!(":b{}", bind_idx);
                            binds.push((placeholder.clone(), sql_val));
                            placeholders.push(placeholder);
                        }
                        _ => {
                            placeholders.push("?".to_string());
                            binds.push((format!("?{}", bind_idx), sql_val));
                        }
                    }
                    bind_idx += 1;
                }
            }
        }

        if cols.is_empty() {
            return Err(DbError::QueryError("No columns to insert".to_string()));
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            cols.join(", "),
            placeholders.join(", ")
        );

        Ok(BoundSql { sql, binds })
    }

    /// Generates parameterized UPDATE statement for a modified record by ROWID.
    pub fn generate_update(
        block: &BlockInfo,
        backend: DbBackend,
        values: &HashMap<String, serde_json::Value>,
        row_id: &str,
    ) -> Result<BoundSql, DbError> {
        let table_name = Self::dml_table(block)?;

        let has_explicit_columns = block.items.iter().any(|i| {
            i.column_name.is_some()
                || i.dml.as_ref().and_then(|d| d.database_item) == Some(true)
        });

        let mut assignments = Vec::new();
        let mut binds = Vec::new();
        let mut bind_idx = 1;

        for item in &block.items {
            if !Self::is_database_item(item, has_explicit_columns) {
                continue;
            }
            if item.dml.as_ref().and_then(|d| d.update_allowed) == Some(false) {
                continue;
            }
            let col = item.column_name.clone().unwrap_or_else(|| item.name.clone());
            let item_name = item.name.clone();

            if let Some(val) = values.get(&item_name).or_else(|| values.get(&col)) {
                let sql_val = match val {
                    serde_json::Value::Null => SqlValue::Null,
                    serde_json::Value::Number(num) => SqlValue::Number(num.to_string()),
                    serde_json::Value::String(s) => SqlValue::Text(s.clone()),
                    serde_json::Value::Bool(b) => SqlValue::Boolean(*b),
                    _ => SqlValue::Text(val.to_string()),
                };

                match backend {
                    DbBackend::Oracle | DbBackend::OracleThin => {
                        let placeholder = format!(":b{}", bind_idx);
                        assignments.push(format!("{} = {}", col, placeholder));
                        binds.push((placeholder, sql_val));
                    }
                    _ => {
                        assignments.push(format!("{} = ?", col));
                        binds.push((format!("?{}", bind_idx), sql_val));
                    }
                }
                bind_idx += 1;
            }
        }

        if assignments.is_empty() {
            return Err(DbError::QueryError("No columns to update".to_string()));
        }

        let (where_clause, rowid_bind) = match backend {
            DbBackend::Oracle | DbBackend::OracleThin => (
                format!("WHERE ROWID = :b{}", bind_idx),
                (format!(":b{}", bind_idx), SqlValue::Text(row_id.to_string())),
            ),
            _ => (
                "WHERE ROWID = ?".to_string(),
                (format!("?{}", bind_idx), SqlValue::Text(row_id.to_string())),
            ),
        };
        binds.push(rowid_bind);

        let sql = format!(
            "UPDATE {} SET {} {}",
            table_name,
            assignments.join(", "),
            where_clause
        );

        Ok(BoundSql { sql, binds })
    }

    /// Generates parameterized DELETE statement for a record by ROWID.
    pub fn generate_delete(
        block: &BlockInfo,
        backend: DbBackend,
        row_id: &str,
    ) -> Result<BoundSql, DbError> {
        let table_name = Self::dml_table(block)?;

        let (sql, binds) = match backend {
            DbBackend::Oracle | DbBackend::OracleThin => (
                format!("DELETE FROM {} WHERE ROWID = :b1", table_name),
                vec![(":b1".to_string(), SqlValue::Text(row_id.to_string()))],
            ),
            _ => (
                format!("DELETE FROM {} WHERE ROWID = ?", table_name),
                vec![("?1".to_string(), SqlValue::Text(row_id.to_string()))],
            ),
        };

        Ok(BoundSql { sql, binds })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_forms_core::model::{BlockDml, ItemInfo};

    #[test]
    fn test_generate_oracle_select() {
        let block = BlockInfo {
            name: "ORDERS".to_string(),
            query_data_source_name: Some("ORDERS_TABLE".to_string()),
            property_class: None,
            items: vec![
                ItemInfo {
                    name: "ORDER_ID".to_string(),
                    item_type: Some("Text Item".to_string()),
                    column_name: Some("ORDER_ID".to_string()),
                    data_type: Some("Number".to_string()),
                    ..Default::default()
                },
                ItemInfo {
                    name: "CUSTOMER".to_string(),
                    item_type: Some("Text Item".to_string()),
                    column_name: Some("CUSTOMER_NAME".to_string()),
                    data_type: Some("Char".to_string()),
                    ..Default::default()
                },
            ],
            trigger_names: vec![],
            inherited: None,
            source_ref: None,
            dml: Some(BlockDml {
                where_clause: Some("STATUS = 'ACTIVE' AND CUSTOMER_ID = :ORDERS.CUST_REF".to_string()),
                order_by_clause: Some("ORDER_ID DESC".to_string()),
                ..Default::default()
            }),
            data_source_column_count: 2,
            relations: vec![],
            records_displayed: 1,
            distance_between_records: 0,
            record_orientation: "Vertical".to_string(),
        };

        let gen = SqlGenerator::generate_select(&block, DbBackend::Oracle, Some(25)).unwrap();
        assert_eq!(gen.table_name, "ORDERS_TABLE");
        assert_eq!(
            gen.sql,
            "SELECT ROWID, ORDER_ID, CUSTOMER_NAME FROM ORDERS_TABLE WHERE STATUS = 'ACTIVE' AND CUSTOMER_ID = :b1 ORDER BY ORDER_ID DESC FETCH FIRST 25 ROWS ONLY"
        );
        assert_eq!(gen.selected_columns, vec!["ROWID", "ORDER_ID", "CUSTOMER_NAME"]);
        assert_eq!(gen.bound_sql.binds.len(), 1);
        // The bind is keyed by the placeholder that appears in the SQL...
        assert_eq!(gen.bound_sql.binds[0].0, ":b1");
        // ...and the Forms reference is reported separately so a session can resolve it.
        assert_eq!(
            gen.bind_refs,
            vec![BindRef { placeholder: ":b1".into(), reference: "ORDERS.CUST_REF".into() }]
        );

        let mut resolved = gen.clone();
        resolved
            .resolve_binds(|r| (r == "ORDERS.CUST_REF").then(|| SqlValue::Number("42".into())))
            .unwrap();
        assert_eq!(resolved.bound_sql.binds[0].1, SqlValue::Number("42".into()));

        let mut unresolved = gen.clone();
        assert!(unresolved.resolve_binds(|_| None).is_err(), "unknown refs must not become NULL");
    }

    #[test]
    fn rewrite_binds_handles_case_literals_and_comments() {
        let r = rewrite_binds(
            "dept_id = :emp.dept_id AND name <> ':not.a.bind' AND x = q'[a :b c]' -- :c\n AND \"A:B\" = 1 /* :d */ AND y = :GLOBAL.user_id",
            DbBackend::Oracle,
            1,
        );
        assert_eq!(
            r.refs.iter().map(|b| b.reference.as_str()).collect::<Vec<_>>(),
            vec!["EMP.DEPT_ID", "GLOBAL.USER_ID"]
        );
        assert!(r.sql.contains("dept_id = :b1"));
        assert!(r.sql.contains("':not.a.bind'"));
        assert!(r.sql.contains("q'[a :b c]'"));
        assert!(r.sql.contains("-- :c"));
        assert!(r.sql.contains("/* :d */"));
        assert!(r.sql.ends_with("y = :b2"));
    }

    #[test]
    fn rewrite_binds_ignores_assignment_and_escaped_quotes() {
        let r = rewrite_binds("v := 'it''s :x'; :out := :inp", DbBackend::Sqlite, 1);
        assert_eq!(r.sql, "v := 'it''s :x'; ?1 := ?2");
        assert_eq!(r.refs[0].reference, "OUT");
        assert_eq!(r.refs[1].reference, "INP");
    }

    #[test]
    fn test_empty_block_columns_returns_error() {
        let block = BlockInfo {
            name: "CONTROL".to_string(),
            query_data_source_name: Some("CONTROL".to_string()),
            property_class: None,
            items: vec![ItemInfo {
                name: "BTN_SAVE".to_string(),
                item_type: Some("Push Button".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = SqlGenerator::generate_select(&block, DbBackend::Oracle, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            DbError::NoDatabaseColumns(name) => assert_eq!(name, "CONTROL"),
            e => panic!("Unexpected error: {:?}", e),
        }
    }
}
