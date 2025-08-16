use itertools::Itertools;
use ruff_db::files::File;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{
    Expr, ExprAttribute, ExprCall, ExprSubscript, Stmt, StmtFunctionDef, StmtTry,
};
use ty_project::Db;
use ty_python_semantic::{ResolvedDefinition, definitions_for_attribute, definitions_for_name};

use crate::transitive_error::call_stack::CallStack;
use crate::transitive_error::capture_stack::ExceptionCaptureStack;
use crate::transitive_error::extract::{extract_caught_exceptions, extract_errors};
use crate::transitive_error::raise::FunctionRaise;

pub(crate) fn get_transitive_errors<'a>(
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &Vec<String>,
    call_stack: CallStack,
    exception_capture_stack: &'a ExceptionCaptureStack,
) -> Vec<FunctionRaise> {
    FunctionTransitiveErrorVisitor::new(
        db,
        file,
        func,
        target_exceptions,
        call_stack,
        exception_capture_stack,
    )
    .transitive_errors()
}

pub(crate) struct FunctionTransitiveErrorVisitor<'a> {
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &'a Vec<String>,
    errors: Vec<FunctionRaise>,
    call_stack: CallStack,
    exception_capture_stack: &'a ExceptionCaptureStack,
    try_block_exceptions: Vec<Vec<String>>,
}

impl<'a> FunctionTransitiveErrorVisitor<'a> {
    pub(crate) fn new(
        db: &'a dyn Db,
        file: File,
        func: &'a StmtFunctionDef,
        target_exceptions: &'a Vec<String>,
        call_stack: CallStack,
        exception_capture_stack: &'a ExceptionCaptureStack,
    ) -> Self {
        Self {
            db,
            file,
            func,
            target_exceptions,
            errors: vec![],
            call_stack,
            exception_capture_stack,
            try_block_exceptions: vec![],
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
        if let Stmt::Raise(raise) = stmt {
            if raise.exc.is_none() {
                if let Some(handler_exceptions) = self
                    .exception_capture_stack
                    .get_current_handler_exceptions()
                {
                    for exc_name in handler_exceptions {
                        if exc_name == "*ALL*" {
                            if let Some(try_exceptions) = self.try_block_exceptions.last() {
                                for try_exc in try_exceptions {
                                    if self.target_exceptions.is_empty()
                                        || self.target_exceptions.contains(try_exc)
                                    {
                                        self.errors.extend_one(FunctionRaise::direct(
                                            self.file,
                                            try_exc.clone(),
                                            raise.range,
                                        ));
                                    }
                                }
                            }
                            continue;
                        }
                        if !self.target_exceptions.is_empty()
                            && !self.target_exceptions.contains(&exc_name)
                        {
                            continue;
                        }
                        self.errors.extend_one(FunctionRaise::direct(
                            self.file,
                            exc_name.clone(),
                            raise.range,
                        ));
                    }
                }
            } else if let Some(name) = try_extract_call_target_name(raise.exc.as_deref()) {
                if (self.target_exceptions.is_empty() || self.target_exceptions.contains(&name))
                    && !self.exception_capture_stack.is_captured(&name)
                {
                    self.errors
                        .extend_one(FunctionRaise::direct(self.file, name, raise.range));
                }
            } else if let Some(Expr::Name(_name_expr)) = raise.exc.as_deref()
                && self.exception_capture_stack.in_handler()
                    && let Some(handler_exceptions) = self
                        .exception_capture_stack
                        .get_current_handler_exceptions()
                    {
                        for exc_name in handler_exceptions {
                            if exc_name == "*ALL*" {
                                continue;
                            }
                            if !self.target_exceptions.is_empty()
                                && !self.target_exceptions.contains(&exc_name)
                            {
                                continue;
                            }
                            self.errors.extend_one(FunctionRaise::direct(
                                self.file,
                                exc_name.clone(),
                                raise.range,
                            ));
                        }
                    }
            walk_stmt(self, stmt);
        } else if let Stmt::Try(StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) = stmt
        {
            let mut try_exceptions = Vec::new();
            let saved_errors_len = self.errors.len();

            self.visit_body(body);

            for error in &self.errors[saved_errors_len..] {
                let exc_name = error.name().clone();
                if !try_exceptions.contains(&exc_name) {
                    try_exceptions.push(exc_name);
                }
            }

            let caught_exceptions = handlers
                .iter()
                .flat_map(extract_caught_exceptions)
                .collect::<Vec<_>>();

            self.exception_capture_stack.push(caught_exceptions.clone());

            let mut filtered_errors = Vec::new();
            for (idx, error) in self.errors.iter().enumerate() {
                if idx < saved_errors_len || !self.exception_capture_stack.is_captured(error.name())
                {
                    filtered_errors.push(error.clone());
                }
            }
            self.errors = filtered_errors;

            self.try_block_exceptions.push(try_exceptions);
            self.exception_capture_stack.pop();

            for handler in handlers {
                if let Some(except_handler) = handler.as_except_handler() {
                    let handler_exceptions = extract_caught_exceptions(handler);
                    self.exception_capture_stack
                        .push_handler_exceptions(handler_exceptions);
                    self.visit_body(&except_handler.body);
                    self.exception_capture_stack.pop_handler_exceptions();
                }
            }
            self.try_block_exceptions.pop();
            self.visit_body(orelse);
            self.visit_body(finalbody);
        } else {
            walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            let defs = definitions_for_call_func(self.db, self.file, *call.func.clone());
            if let Some(defs) = defs {
                for def in defs {
                    if let ResolvedDefinition::Definition(def) = def {
                        let definition_file = def.file(self.db);
                        let definition_path = match definition_file.path(self.db) {
                            ruff_db::files::FilePath::System(path)
                                if !self.db.project().is_file_included(self.db, path) =>
                            {
                                continue;
                            }
                            ruff_db::files::FilePath::System(path) => path,
                            ruff_db::files::FilePath::SystemVirtual(_) => continue,
                            ruff_db::files::FilePath::Vendored(_) => continue,
                        };
                        if let Some(name) = def.name(self.db) {
                            let key = (definition_path.as_str().into(), name);
                            if self.call_stack.contains(&key) {
                                continue;
                            }
                        }
                        let transitive_errors = extract_errors(
                            self.db,
                            self.file,
                            call.range,
                            definition_file,
                            def,
                            self.target_exceptions,
                            self.call_stack.clone(),
                            self.exception_capture_stack,
                        );
                        self.errors.extend(
                            transitive_errors
                                .into_iter()
                                .filter(|e| !self.exception_capture_stack.is_captured(e.name())),
                        )
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
