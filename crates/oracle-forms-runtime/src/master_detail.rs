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

    /// Evaluates the join condition by substituting master record column values into the WHERE expression for the detail block.
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
