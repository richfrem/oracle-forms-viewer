use crate::ast::*;
use crate::parser::Parser;
use crate::symbols::SymbolTable;
use oracle_forms_core::model::ModuleIndex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraphEdge {
    pub caller: String,
    pub callee: String,
    pub call_type: String, // "Builtin", "LocalUnit", "Library", "Database"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<CallGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemReference {
    pub unit_name: String,
    pub access_type: String, // "Read" or "Write"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemCrossRef {
    pub items: HashMap<String, Vec<ItemReference>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StatementCategory {
    Supported,
    Delegated,
    Unsupported(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutabilityReport {
    pub total_statements: usize,
    pub supported_count: usize,
    pub delegated_count: usize,
    pub unsupported_count: usize,
    pub support_percentage: f64,
    pub details: Vec<(String, StatementCategory)>,
}

pub struct CodeIntelligence;

impl CodeIntelligence {
    /// Analyzes a parsed module and generates CallGraph, ItemCrossRef, and ExecutabilityReport.
    pub fn analyze(module: &ModuleIndex) -> (CallGraph, ItemCrossRef, ExecutabilityReport) {
        let mut symbols = SymbolTable::new();
        symbols.populate_from_module(module);

        let mut call_nodes = HashSet::new();
        let mut call_edges = Vec::new();
        let mut item_refs: HashMap<String, Vec<ItemReference>> = HashMap::new();
        let mut statement_details = Vec::new();

        // 1. Analyze all triggers in the module
        for trigger in &module.triggers {
            let unit_name = match (&trigger.block_name, &trigger.item_name) {
                (Some(b), Some(i)) => format!("{}.{}.{}", b, i, trigger.name),
                (Some(b), None) => format!("{}.{}", b, trigger.name),
                _ => trigger.name.clone(),
            };
            call_nodes.insert(unit_name.clone());

            let source = trigger.source_text.as_deref().unwrap_or(&trigger.first_line);
            let mut parser = Parser::new(source);
            if let Ok(ast) = parser.parse_unit() {
                Self::walk_ast(
                    &ast,
                    &unit_name,
                    &symbols,
                    &mut call_nodes,
                    &mut call_edges,
                    &mut item_refs,
                    &mut statement_details,
                );
            }
        }

        // 2. Analyze all Program Units in the module
        for pu in &module.program_units {
            let unit_name = pu.name.clone();
            call_nodes.insert(unit_name.clone());

            let source = pu.source_text.as_deref().unwrap_or(&pu.first_line);
            let mut parser = Parser::new(source);
            if let Ok(ast) = parser.parse_unit() {
                Self::walk_ast(
                    &ast,
                    &unit_name,
                    &symbols,
                    &mut call_nodes,
                    &mut call_edges,
                    &mut item_refs,
                    &mut statement_details,
                );
            }
        }

        let total = statement_details.len();
        let mut supported = 0;
        let mut delegated = 0;
        let mut unsupported = 0;

        for (_stmt, cat) in &statement_details {
            match cat {
                StatementCategory::Supported => supported += 1,
                StatementCategory::Delegated => delegated += 1,
                StatementCategory::Unsupported(_) => unsupported += 1,
            }
        }

        let pct = if total > 0 {
            ((supported + delegated) as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let call_graph = CallGraph {
            nodes: call_nodes.into_iter().collect(),
            edges: call_edges,
        };

        let xref = ItemCrossRef { items: item_refs };

        let report = ExecutabilityReport {
            total_statements: total,
            supported_count: supported,
            delegated_count: delegated,
            unsupported_count: unsupported,
            support_percentage: pct,
            details: statement_details,
        };

        (call_graph, xref, report)
    }

    fn walk_ast(
        unit: &PlsqlUnit,
        unit_name: &str,
        symbols: &SymbolTable,
        nodes: &mut HashSet<String>,
        edges: &mut Vec<CallGraphEdge>,
        item_refs: &mut HashMap<String, Vec<ItemReference>>,
        details: &mut Vec<(String, StatementCategory)>,
    ) {
        match unit {
            PlsqlUnit::AnonymousBlock(b) => {
                Self::walk_statements(&b.body, unit_name, symbols, nodes, edges, item_refs, details);
            }
            PlsqlUnit::Procedure(p) => {
                Self::walk_statements(&p.body, unit_name, symbols, nodes, edges, item_refs, details);
            }
            PlsqlUnit::Function(f) => {
                Self::walk_statements(&f.body, unit_name, symbols, nodes, edges, item_refs, details);
            }
            PlsqlUnit::StatementList(stmts) => {
                Self::walk_statements(stmts, unit_name, symbols, nodes, edges, item_refs, details);
            }
            PlsqlUnit::PackageSpec(s) => {
                for item in &s.items {
                    if let PackageSpecItem::Procedure(p) = item {
                        Self::walk_statements(&p.body, unit_name, symbols, nodes, edges, item_refs, details);
                    }
                }
            }
            PlsqlUnit::PackageBody(b) => {
                for item in &b.items {
                    if let PackageBodyItem::Procedure(p) = item {
                        Self::walk_statements(&p.body, unit_name, symbols, nodes, edges, item_refs, details);
                    }
                }
            }
        }
    }

    fn walk_statements(
        stmts: &[Statement],
        unit_name: &str,
        symbols: &SymbolTable,
        nodes: &mut HashSet<String>,
        edges: &mut Vec<CallGraphEdge>,
        item_refs: &mut HashMap<String, Vec<ItemReference>>,
        details: &mut Vec<(String, StatementCategory)>,
    ) {
        for stmt in stmts {
            match stmt {
                Statement::Assignment { target, value } => {
                    // Target is written
                    if let Expr::Bind(bind_name) = target {
                        let clean = bind_name.trim_start_matches(':').to_ascii_uppercase();
                        item_refs.entry(clean).or_default().push(ItemReference {
                            unit_name: unit_name.to_string(),
                            access_type: "Write".to_string(),
                        });
                    }
                    // Value is read
                    Self::walk_expr(value, unit_name, symbols, nodes, edges, item_refs);
                    details.push((format!("{:?} := ...", target), StatementCategory::Supported));
                }
                Statement::Call { target, args } => {
                    let target_upper = target.to_ascii_uppercase();
                    nodes.insert(target_upper.clone());

                    let call_type = if let Some(sym) = symbols.resolve(&target_upper) {
                        format!("{:?}", sym.kind)
                    } else if Self::is_unsupported_builtin(&target_upper) {
                        "UnsupportedBuiltin".to_string()
                    } else {
                        "Database".to_string()
                    };

                    edges.push(CallGraphEdge {
                        caller: unit_name.to_string(),
                        callee: target_upper.clone(),
                        call_type,
                    });

                    for arg in args {
                        Self::walk_expr(arg, unit_name, symbols, nodes, edges, item_refs);
                    }

                    if Self::is_unsupported_builtin(&target_upper) {
                        details.push((
                            format!("{}(...)", target),
                            StatementCategory::Unsupported(format!("Legacy builtin '{}'", target)),
                        ));
                    } else {
                        details.push((format!("{}(...)", target), StatementCategory::Supported));
                    }
                }
                Statement::If {
                    condition,
                    then_branch,
                    elsifs,
                    else_branch,
                } => {
                    Self::walk_expr(condition, unit_name, symbols, nodes, edges, item_refs);
                    Self::walk_statements(then_branch, unit_name, symbols, nodes, edges, item_refs, details);
                    for (c, branch) in elsifs {
                        Self::walk_expr(c, unit_name, symbols, nodes, edges, item_refs);
                        Self::walk_statements(branch, unit_name, symbols, nodes, edges, item_refs, details);
                    }
                    if let Some(eb) = else_branch {
                        Self::walk_statements(eb, unit_name, symbols, nodes, edges, item_refs, details);
                    }
                    details.push(("IF ... END IF".to_string(), StatementCategory::Supported));
                }
                Statement::Loop { body } => {
                    Self::walk_statements(body, unit_name, symbols, nodes, edges, item_refs, details);
                    details.push(("LOOP ... END LOOP".to_string(), StatementCategory::Supported));
                }
                Statement::WhileLoop { condition, body } => {
                    Self::walk_expr(condition, unit_name, symbols, nodes, edges, item_refs);
                    Self::walk_statements(body, unit_name, symbols, nodes, edges, item_refs, details);
                    details.push(("WHILE ... LOOP".to_string(), StatementCategory::Supported));
                }
                Statement::ForLoop {
                    lower,
                    upper,
                    body,
                    ..
                } => {
                    Self::walk_expr(lower, unit_name, symbols, nodes, edges, item_refs);
                    Self::walk_expr(upper, unit_name, symbols, nodes, edges, item_refs);
                    Self::walk_statements(body, unit_name, symbols, nodes, edges, item_refs, details);
                    details.push(("FOR ... LOOP".to_string(), StatementCategory::Supported));
                }
                Statement::Sql(sql_stmt) => match sql_stmt {
                    SqlStatement::SelectInto {
                        into_targets,
                        where_clause,
                        raw_sql,
                        ..
                    } => {
                        for t in into_targets {
                            if let Expr::Bind(bind_name) = t {
                                let clean = bind_name.trim_start_matches(':').to_ascii_uppercase();
                                item_refs.entry(clean).or_default().push(ItemReference {
                                    unit_name: unit_name.to_string(),
                                    access_type: "Write".to_string(),
                                });
                            }
                        }
                        if let Some(w) = where_clause {
                            // Scan binds in where clause
                            for part in w.split_whitespace() {
                                if part.starts_with(':') {
                                    let clean = part.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.');
                                    let bind_clean = clean.trim_start_matches(':').to_ascii_uppercase();
                                    item_refs.entry(bind_clean).or_default().push(ItemReference {
                                        unit_name: unit_name.to_string(),
                                        access_type: "Read".to_string(),
                                    });
                                }
                            }
                        }
                        details.push((raw_sql.clone(), StatementCategory::Delegated));
                    }
                    SqlStatement::Dml { raw_sql, .. } => {
                        details.push((raw_sql.clone(), StatementCategory::Delegated));
                    }
                },
                Statement::Return(expr_opt) => {
                    if let Some(e) = expr_opt {
                        Self::walk_expr(e, unit_name, symbols, nodes, edges, item_refs);
                    }
                    details.push(("RETURN".to_string(), StatementCategory::Supported));
                }
                Statement::Raise(name_opt) => {
                    details.push((
                        format!("RAISE {:?}", name_opt),
                        StatementCategory::Supported,
                    ));
                }
                Statement::Null => {
                    details.push(("NULL;".to_string(), StatementCategory::Supported));
                }
                Statement::Block(b) => {
                    Self::walk_statements(&b.body, unit_name, symbols, nodes, edges, item_refs, details);
                    details.push(("BEGIN ... END;".to_string(), StatementCategory::Supported));
                }
                Statement::Opaque { raw, error } => {
                    details.push((
                        raw.clone(),
                        StatementCategory::Unsupported(error.clone().unwrap_or_else(|| "Opaque syntax".to_string())),
                    ));
                }
            }
        }
    }

    fn walk_expr(
        expr: &Expr,
        unit_name: &str,
        symbols: &SymbolTable,
        nodes: &mut HashSet<String>,
        edges: &mut Vec<CallGraphEdge>,
        item_refs: &mut HashMap<String, Vec<ItemReference>>,
    ) {
        match expr {
            Expr::Bind(bind_name) => {
                let clean = bind_name.trim_start_matches(':').to_ascii_uppercase();
                item_refs.entry(clean).or_default().push(ItemReference {
                    unit_name: unit_name.to_string(),
                    access_type: "Read".to_string(),
                });
            }
            Expr::FunctionCall { name, args } => {
                let name_upper = name.to_ascii_uppercase();
                nodes.insert(name_upper.clone());
                let call_type = if let Some(sym) = symbols.resolve(&name_upper) {
                    format!("{:?}", sym.kind)
                } else {
                    "Database".to_string()
                };
                edges.push(CallGraphEdge {
                    caller: unit_name.to_string(),
                    callee: name_upper,
                    call_type,
                });
                for a in args {
                    Self::walk_expr(a, unit_name, symbols, nodes, edges, item_refs);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::walk_expr(left, unit_name, symbols, nodes, edges, item_refs);
                Self::walk_expr(right, unit_name, symbols, nodes, edges, item_refs);
            }
            Expr::UnaryOp { expr, .. } => {
                Self::walk_expr(expr, unit_name, symbols, nodes, edges, item_refs);
            }
            Expr::MemberAccess { object, .. } => {
                Self::walk_expr(object, unit_name, symbols, nodes, edges, item_refs);
            }
            Expr::Parenthesized(e) => {
                Self::walk_expr(e, unit_name, symbols, nodes, edges, item_refs);
            }
            _ => {}
        }
    }

    fn is_unsupported_builtin(name: &str) -> bool {
        let unsupported = [
            "HOST",
            "OLE2",
            "TEXT_IO",
            "CLIENT_TEXT_IO",
            "WEBUTIL",
            "RUN_PRODUCT",
            "D2KWUTIL",
        ];
        let n_upper = name.to_ascii_uppercase();
        unsupported.iter().any(|u| n_upper.starts_with(u))
    }
}
