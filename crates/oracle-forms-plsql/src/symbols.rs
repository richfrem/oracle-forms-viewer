use oracle_forms_core::model::ModuleIndex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Builtin,
    ProgramUnit,
    LibraryUnit,
    Block,
    Item,
    SystemVariable,
    GlobalVariable,
    LocalVariable,
    Parameter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub scope: String, // e.g. "GLOBAL", "SYSTEM", "MODULE", "BLOCK1", "proc_name"
    pub description: Option<String>,
}

pub struct SymbolTable {
    symbols: HashMap<String, Symbol>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut table = Self {
            symbols: HashMap::new(),
        };
        table.register_builtins();
        table
    }

    fn register_builtins(&mut self) {
        let builtins = [
            ("GO_ITEM", "Moves focus to target item"),
            ("GO_BLOCK", "Moves focus to target block"),
            ("NEXT_ITEM", "Moves focus to next navigable item"),
            ("PREVIOUS_ITEM", "Moves focus to previous item"),
            ("NEXT_RECORD", "Navigates to next record in block"),
            ("PREVIOUS_RECORD", "Navigates to previous record in block"),
            ("FIRST_RECORD", "Navigates to first record in block"),
            ("LAST_RECORD", "Navigates to last record in block"),
            ("CREATE_RECORD", "Inserts a new blank record into current block"),
            ("DELETE_RECORD", "Marks current record for deletion"),
            ("CLEAR_RECORD", "Reverts current record changes or deletes new row"),
            ("CLEAR_BLOCK", "Clears all records in target block"),
            ("CLEAR_FORM", "Rolls back uncommitted changes and resets form"),
            ("EXECUTE_QUERY", "Executes SQL query for active block"),
            ("ENTER_QUERY", "Enters Query-by-Example mode"),
            ("COMMIT_FORM", "Executes commit event transaction pipeline"),
            ("EXIT_FORM", "Exits the active form"),
            ("CALL_FORM", "Runs another form without dismissing current form"),
            ("NEW_FORM", "Replaces current form with a new form"),
            ("OPEN_FORM", "Opens another form independently"),
            ("DO_KEY", "Executes key trigger corresponding to action"),
            ("MESSAGE", "Displays message on status bar or alert"),
            ("SHOW_ALERT", "Displays interactive modal alert dialog"),
            ("SYNCHRONIZE", "Flushes pending UI updates"),
            ("SET_WINDOW_PROPERTY", "Modifies window geometry or title"),
            ("GET_WINDOW_PROPERTY", "Retrieves window property"),
            ("SET_ITEM_PROPERTY", "Modifies item state or appearance"),
            ("GET_ITEM_PROPERTY", "Retrieves item property"),
            ("SET_BLOCK_PROPERTY", "Modifies block property"),
            ("GET_BLOCK_PROPERTY", "Retrieves block property"),
            ("GET_CANVAS_PROPERTY", "Retrieves canvas property"),
            ("NAME_IN", "Dynamically evaluates item or variable value"),
            ("COPY", "Dynamically sets item or variable value"),
            ("DEFAULT_VALUE", "Sets default global variable value"),
            ("ERASE", "Deallocates global variable"),
            ("SHOW_LOV", "Displays List of Values"),
            ("LIST_VALUES", "Invokes default LOV for item"),
            ("SQLERRM", "Returns last SQL error message"),
            ("FORM_TRIGGER_FAILURE", "Built-in exception raising validation/nav abort"),
        ];

        for (name, desc) in builtins {
            self.symbols.insert(
                name.to_ascii_uppercase(),
                Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Builtin,
                    scope: "BUILTIN".to_string(),
                    description: Some(desc.to_string()),
                },
            );
        }
    }

    /// Populates symbol table from parsed ModuleIndex.
    pub fn populate_from_module(&mut self, module: &ModuleIndex) {
        // 1. Blocks and Items
        for block in &module.blocks {
            let b_upper = block.name.to_ascii_uppercase();
            self.symbols.insert(
                b_upper.clone(),
                Symbol {
                    name: block.name.clone(),
                    kind: SymbolKind::Block,
                    scope: "MODULE".to_string(),
                    description: Some("Form Data/Control Block".to_string()),
                },
            );

            for item in &block.items {
                let full_item = format!("{}.{}", b_upper, item.name.to_ascii_uppercase());
                self.symbols.insert(
                    full_item.clone(),
                    Symbol {
                        name: item.name.clone(),
                        kind: SymbolKind::Item,
                        scope: b_upper.clone(),
                        description: item.item_type.clone(),
                    },
                );
            }
        }

        // 2. Form Program Units
        for pu in &module.program_units {
            let pu_upper = pu.name.to_ascii_uppercase();
            self.symbols.insert(
                pu_upper,
                Symbol {
                    name: pu.name.clone(),
                    kind: SymbolKind::ProgramUnit,
                    scope: "MODULE".to_string(),
                    description: Some(format!("{:?}", pu.unit_type)),
                },
            );
        }

        // 3. Attached Libraries
        for lib in &module.attached_libraries {
            let lib_upper = lib.name.to_ascii_uppercase();
            self.symbols.insert(
                lib_upper,
                Symbol {
                    name: lib.name.clone(),
                    kind: SymbolKind::LibraryUnit,
                    scope: "LIBRARY".to_string(),
                    description: Some("Attached Library (.pll/.pld)".to_string()),
                },
            );
        }
    }

    /// Resolves an identifier, bind variable, or function call name against known symbols.
    pub fn resolve(&self, ident: &str) -> Option<Symbol> {
        let clean = ident.trim().trim_start_matches(':').to_ascii_uppercase();

        if clean.starts_with("SYSTEM.") {
            return Some(Symbol {
                name: clean.clone(),
                kind: SymbolKind::SystemVariable,
                scope: "SYSTEM".to_string(),
                description: Some("Oracle Forms System Variable".to_string()),
            });
        }

        if clean.starts_with("GLOBAL.") {
            return Some(Symbol {
                name: clean.clone(),
                kind: SymbolKind::GlobalVariable,
                scope: "GLOBAL".to_string(),
                description: Some("Oracle Forms Session Global Variable".to_string()),
            });
        }

        self.symbols.get(&clean).cloned()
    }

    pub fn all_symbols(&self) -> Vec<&Symbol> {
        self.symbols.values().collect()
    }
}
