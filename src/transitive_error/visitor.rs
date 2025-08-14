use itertools::Itertools;
use ruff_db::files::File;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprSubscript, Stmt, StmtFunctionDef};
use ty_project::Db;
use ty_python_semantic::{ResolvedDefinition, definitions_for_attribute, definitions_for_name};

use crate::transitive_error::extract::extract_errors;
use crate::transitive_error::raise::FunctionRaise;
use crate::transitive_error::stack::CallStack;

pub(crate) fn get_transitive_errors<'a>(
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &Vec<String>,
    stack: CallStack,
) -> Vec<FunctionRaise> {
    FunctionTransitiveErrorVisitor::new(db, file, func, target_exceptions, stack)
        .transitive_errors()
}

pub(crate) struct FunctionTransitiveErrorVisitor<'a> {
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &'a Vec<String>,
    errors: Vec<FunctionRaise>,
    stack: CallStack,
}

impl<'a> FunctionTransitiveErrorVisitor<'a> {
    pub(crate) fn new(
        db: &'a dyn Db,
        file: File,
        func: &'a StmtFunctionDef,
        target_exceptions: &'a Vec<String>,
        stack: CallStack,
    ) -> Self {
        Self {
            db,
            file,
            func,
            target_exceptions,
            errors: vec![],
            stack,
        }
    }

    pub(crate) fn transitive_errors(&mut self) -> Vec<FunctionRaise> {
        self.visit_body(&self.func.body);
        self.errors = self
            .errors
            .clone()
            .into_iter()
            .sorted_by_key(|e| e.sort_key())
            .chunk_by(|e| e.group_key())
            .into_iter()
            .map(|(_, es)| es.into_iter().next().unwrap())
            .collect();
        self.errors.clone()
    }
}

impl<'a> Visitor<'a> for FunctionTransitiveErrorVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Raise(raise) = stmt
            && let Some(name) = try_extract_call_target_name(raise.exc.as_deref())
            && (self.target_exceptions.is_empty() || self.target_exceptions.contains(&name))
        {
            self.errors
                .extend_one(FunctionRaise::direct(self.file, name, raise.range));
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            let defs = definitions_for_call_func(self.db, self.file, *call.func.clone());
            if let Some(defs) = defs {
                for def in defs {
                    if let ResolvedDefinition::Definition(def) = def {
                        let definition_file = def.file(self.db);
                        if let Some(path) = definition_file.path(self.db).as_system_path()
                            && !self.db.project().is_file_included(self.db, path)
                        {
                            continue;
                        }
                        if let Some(name) = def.name(self.db) {
                            let key = (definition_file.path(self.db).as_str().into(), name);
                            if self.stack.contains(&key) {
                                continue;
                            }
                        }
                        self.errors.extend(extract_errors(
                            self.db,
                            self.file,
                            call.range,
                            definition_file,
                            def,
                            self.target_exceptions,
                            self.stack.clone(),
                        ))
                    }
                }
            }
        }
        walk_expr(self, expr);
    }
}

#[inline(always)]
fn try_extract_call_target_name(expr: Option<&Expr>) -> Option<String> {
    if let Some(Expr::Call(ExprCall { func, .. })) = expr {
        if let Expr::Name(ref name) = **func {
            return Some((*name.id.as_str()).into());
        }
        if let Expr::Attribute(ExprAttribute { ref attr, .. }) = **func {
            return Some((*attr.as_str()).into());
        }
        if let Expr::Subscript(ExprSubscript { ref value, .. }) = **func
            && let Expr::Name(ref name) = **value
        {
            return Some((*name.id.as_str()).into());
        }
    }
    None
}

fn definitions_for_call_func<'a>(
    db: &'a dyn Db,
    file: File,
    func: Expr,
) -> Option<Vec<ResolvedDefinition<'a>>> {
    if let Expr::Name(ref name) = func {
        return Some(definitions_for_name(db, file, name));
    } else if let Expr::Attribute(ref attr) = func {
        return Some(definitions_for_attribute(db, file, attr));
    }
    None
}
