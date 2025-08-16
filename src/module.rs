use std::vec;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::{
    Identifier, Stmt, StmtClassDef, StmtFunctionDef,
    statement_visitor::{StatementVisitor, walk_stmt},
};
use ruff_text_size::TextRange;

pub(crate) struct ModuleCollector<'a> {
    overloads: Option<(Identifier, Vec<&'a StmtFunctionDef>)>,
    functions: Vec<(Vec<TextRange>, Vec<&'a StmtFunctionDef>)>,
}

impl<'a> ModuleCollector<'a> {
    pub(crate) fn new() -> Self {
        Self {
            overloads: None,
            functions: vec![],
        }
    }

    pub(crate) fn init(&mut self, module: &'a ParsedModuleRef) {
        let body = module.suite();
        self.visit_body(body);
    }

    pub(crate) fn find_functions(&self, range: &TextRange) -> Vec<&StmtFunctionDef> {
        let mut found = vec![];
        for (ranges, defs) in &self.functions {
            if ranges.iter().any(|target| target == range) {
                found.extend(defs);
            }
        }
        found
    }

    pub(crate) fn list_functions(&self) -> Vec<&StmtFunctionDef> {
        let mut found = vec![];
        for (_, defs) in &self.functions {
            found.extend(defs);
        }
        found
    }

    fn collect_function(
        &mut self,
        def: &'a StmtFunctionDef,
    ) -> (Vec<TextRange>, Vec<&'a StmtFunctionDef>) {
        if let Some((overload_name, ..)) = &self.overloads
            && overload_name != &def.name
        {
            self.overloads = None
        }
        if def.decorator_list.iter().any(|d| {
            d.expression
                .clone()
                .name_expr()
                .map(|name| name.id.as_str() == "overload")
                .unwrap_or(false)
        }) {
            if let Some((_, overloads)) = &mut self.overloads {
                overloads.push(def);
            } else {
                self.overloads = Some((def.name.clone(), vec![def]))
            }
        }
        if let Some((_, overloads)) = &mut self.overloads {
            (vec![def.range], [vec![def], overloads.clone()].concat())
        } else {
            (vec![def.range], vec![def])
        }
    }
}

impl<'a> StatementVisitor<'a> for ModuleCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::FunctionDef(def) = stmt {
            let collected = self.collect_function(def);
            self.functions.extend_one(collected);
        } else if let Stmt::ClassDef(StmtClassDef { body, range, .. }) = stmt {
            let mut fns = vec![];
            for cls_stmt in body {
                if let Stmt::FunctionDef(def @ StmtFunctionDef { name, .. }) = cls_stmt
                    && CLS_INIT_FNS.iter().any(|n| **name == **n)
                {
                    fns.push(def);
                }
            }
            if !fns.is_empty() {
                self.functions.extend_one((vec![*range], fns));
            }
        }
        walk_stmt(self, stmt);
    }
}

const CLS_INIT_FNS: [&str; 6] = [
    "__init__",
    "__new__",
    "__enter__",
    "__exit__",
    "__aenter__",
    "__aexit__",
];
