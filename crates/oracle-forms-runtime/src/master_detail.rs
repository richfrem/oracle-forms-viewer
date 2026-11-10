use oracle_forms_core::model::RelationInfo;
use std::collections::HashMap;

/// Declarative Master-Detail relationship manager between parent and child blocks.
#[derive(Debug, Clone)]
pub struct MasterDetailLink {
    pub name: String,
    pub master_block: String,
    pub detail_block: String,
    pub join_condition: String,
    pub auto_query: bool,
    pub prevent_masterless_operations: bool,
    pub deferred: bool,
}

impl MasterDetailLink {
    pub fn from_relation(master_block: &str, rel: &RelationInfo) -> Option<Self> {
        let detail = rel.detail_block.as_deref()?.trim().to_string();
        if detail.is_empty() {
            return None;
        }

        let join = rel.join_condition.as_deref().unwrap_or("").trim().to_string();
        let unescaped_join = join
            .replace("&amp;#10;", " ")
            .replace("&amp;#13;", " ")
            .replace("&#10;", " ")
            .replace("&#13;", " ")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">");

        Some(Self {
            name: rel.name.clone(),
            master_block: master_block.to_string(),
            detail_block: detail,
            join_condition: unescaped_join,
            auto_query: rel.auto_query.unwrap_or(true),
            prevent_masterless_operations: rel.prevent_masterless_operations.unwrap_or(true),
            deferred: rel.deferred.unwrap_or(false),
        })
    }

    /// The join condition with every `MASTER.COLUMN` reference turned into a Forms bind
    /// (`:MASTER.COLUMN`), so the query layer binds master values as parameters instead
    /// of pasting them into the SQL text. String literals are left untouched.
    pub fn detail_where_with_binds(&self) -> String {
        let master = self.master_block.to_ascii_uppercase();
        let chars: Vec<char> = self.join_condition.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            out.push('\'');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '#' | '.')) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let prev = out.chars().last();
                let is_master_ref = word
                    .split_once('.')
                    .map(|(b, col)| b.eq_ignore_ascii_case(&master) && !col.is_empty())
                    .unwrap_or(false);
                if is_master_ref && prev != Some(':') {
                    out.push(':');
                }
                out.push_str(&word);
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }

    /// Display-only rendering of the join with master values inlined as literals.
    /// Queries use [`MasterDetailLink::detail_where_with_binds`] instead.
    pub fn evaluate_detail_where(
        &self,
        master_values: &HashMap<String, serde_json::Value>,
    ) -> String {
        let mut detail_where = self.join_condition.clone();
        let master_prefix = format!("{}.", self.master_block.to_ascii_uppercase());

        // Replace references to MASTER_BLOCK.COLUMN with their literal values
        for (col, val) in master_values {
            let col_ref = format!("{}{}", master_prefix, col.to_ascii_uppercase());
            let val_str = match val {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                serde_json::Value::Bool(b) => if *b { "'Y'".to_string() } else { "'N'".to_string() },
                serde_json::Value::Null => "NULL".to_string(),
                _ => val.to_string(),
            };

            // Case-insensitive replacement
            let re_str = format!(r"(?i)\b{}\b", regex::escape(&col_ref));
            if let Ok(re) = regex::Regex::new(&re_str) {
                detail_where = re.replace_all(&detail_where, val_str.as_str()).to_string();
            }
        }

        detail_where
    }
}
