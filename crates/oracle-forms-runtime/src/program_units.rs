//! Registry of callable PL/SQL program units: a form's own procedures, functions and
//! packages, plus those of its attached libraries (`.pll` / `.pld`).
//!
//! Name resolution follows Forms: the form's own units first, then attached libraries
//! in attach order. Inside a package body, unqualified names resolve to the same
//! package first.

use oracle_forms_core::model::{ModuleIndex, ProgramUnitInfo};
use oracle_forms_plsql::ast::{
    Declaration, ExceptionHandler, PackageBodyItem, PackageSpecItem, ParameterDecl, PlsqlUnit, Statement,
    VariableDecl,
};
use oracle_forms_plsql::Parser;
use std::collections::HashMap;
use std::sync::Arc;

/// A procedure or function body that can be executed.
#[derive(Debug, Clone)]
pub struct Routine {
    /// Upper-case unqualified name.
    pub name: String,
    /// Owning package (upper case), if any.
    pub package: Option<String>,
    pub params: Vec<ParameterDecl>,
    pub declarations: Vec<Declaration>,
    pub body: Vec<Statement>,
    pub exception_handlers: Vec<ExceptionHandler>,
    pub is_function: bool,
    /// Where it came from (form or library name), for diagnostics.
    pub origin: String,
}

/// Package-level state declarations.
#[derive(Debug, Clone, Default)]
pub struct PackageDef {
    pub name: String,
    pub variables: Vec<VariableDecl>,
}

#[derive(Debug, Clone, Default)]
pub struct ProgramRegistry {
    /// Keyed by `NAME` (standalone) or `PKG.NAME` (packaged). First definition wins.
    routines: HashMap<String, Arc<Routine>>,
    packages: HashMap<String, PackageDef>,
    /// Parse problems, reported rather than silently ignored.
    pub errors: Vec<String>,
}

impl ProgramRegistry {
    pub fn from_module(module: &ModuleIndex) -> Self {
        let mut reg = Self::default();
        reg.add_units(&module.key.name, &module.program_units);
        reg
    }

    /// Adds an attached library's units. Units already defined (by the form or an
    /// earlier library) are not replaced.
    pub fn attach_library(&mut self, library: &ModuleIndex) {
        self.add_units(&library.key.name, &library.program_units);
    }

    fn add_units(&mut self, origin: &str, units: &[ProgramUnitInfo]) {
        for pu in units {
            let source = pu.source_text.as_deref().unwrap_or(&pu.first_line);
            if source.trim().is_empty() {
                continue;
            }
            let mut parser = Parser::new(source);
            let parsed = parser.parse_all_units();
            for e in &parser.errors {
                self.errors.push(format!("{} / {}: {}", origin, pu.name, e));
            }
            for unit in parsed {
                self.add_unit(origin, unit);
            }
        }
    }

    fn insert(&mut self, key: String, routine: Routine) {
        self.routines.entry(key).or_insert_with(|| Arc::new(routine));
    }

    fn add_unit(&mut self, origin: &str, unit: PlsqlUnit) {
        match unit {
            PlsqlUnit::Procedure(p) => {
                let name = p.name.to_ascii_uppercase();
                self.insert(
                    name.clone(),
                    Routine {
                        name,
                        package: None,
                        params: p.params,
                        declarations: p.declarations,
                        body: p.body,
                        exception_handlers: p.exception_handlers,
                        is_function: false,
                        origin: origin.to_string(),
                    },
                );
            }
            PlsqlUnit::Function(f) => {
                let name = f.name.to_ascii_uppercase();
                self.insert(
                    name.clone(),
                    Routine {
                        name,
                        package: None,
                        params: f.params,
                        declarations: f.declarations,
                        body: f.body,
                        exception_handlers: f.exception_handlers,
                        is_function: true,
                        origin: origin.to_string(),
                    },
                );
            }
            PlsqlUnit::PackageSpec(spec) => {
                let pkg = spec.name.to_ascii_uppercase();
                let entry = self.packages.entry(pkg.clone()).or_insert_with(|| PackageDef { name: pkg.clone(), variables: vec![] });
                for item in spec.items {
                    if let PackageSpecItem::Variable(v) = item {
                        entry.variables.push(v);
                    }
                }
            }
            PlsqlUnit::PackageBody(body) => {
                let pkg = body.name.to_ascii_uppercase();
                self.packages.entry(pkg.clone()).or_insert_with(|| PackageDef { name: pkg.clone(), variables: vec![] });
                for item in body.items {
                    match item {
                        PackageBodyItem::Procedure(p) => {
                            let name = p.name.to_ascii_uppercase();
                            self.insert(
                                format!("{}.{}", pkg, name),
                                Routine {
                                    name,
                                    package: Some(pkg.clone()),
                                    params: p.params,
                                    declarations: p.declarations,
                                    body: p.body,
                                    exception_handlers: p.exception_handlers,
                                    is_function: false,
                                    origin: origin.to_string(),
                                },
                            );
                        }
                        PackageBodyItem::Function(f) => {
                            let name = f.name.to_ascii_uppercase();
                            self.insert(
                                format!("{}.{}", pkg, name),
                                Routine {
                                    name,
                                    package: Some(pkg.clone()),
                                    params: f.params,
                                    declarations: f.declarations,
                                    body: f.body,
                                    exception_handlers: f.exception_handlers,
                                    is_function: true,
                                    origin: origin.to_string(),
                                },
                            );
                        }
                        PackageBodyItem::Variable(v) => {
                            if let Some(def) = self.packages.get_mut(&pkg) {
                                def.variables.push(v);
                            }
                        }
                    }
                }
            }
            // Anonymous blocks or statement lists in a program-unit slot are not callable.
            PlsqlUnit::AnonymousBlock(_) | PlsqlUnit::StatementList(_) => {}
        }
    }

    /// Resolves a call target. `current_package` lets unqualified names inside a package
    /// body find sibling routines.
    pub fn resolve(&self, name: &str, current_package: Option<&str>) -> Option<Arc<Routine>> {
        let upper = name.to_ascii_uppercase();
        if !upper.contains('.') {
            if let Some(pkg) = current_package {
                if let Some(r) = self.routines.get(&format!("{}.{}", pkg, upper)) {
                    return Some(r.clone());
                }
            }
        }
        self.routines.get(&upper).cloned()
    }

    pub fn package(&self, name: &str) -> Option<&PackageDef> {
        self.packages.get(&name.to_ascii_uppercase())
    }

    pub fn is_package(&self, name: &str) -> bool {
        self.packages.contains_key(&name.to_ascii_uppercase())
    }

    pub fn routine_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.routines.keys().cloned().collect();
        names.sort();
        names
    }
}
