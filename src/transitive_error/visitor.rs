use itertools::Itertools;
use ruff_db::files::File;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, ExprCall, Stmt, StmtFunctionDef, StmtTry, WithItem};
use ty_project::Db;
use ty_python_semantic::{ResolvedDefinition, definitions_for_attribute, definitions_for_name};

use crate::transitive_error::call_stack::CallStack;
use crate::transitive_error::capture_stack::ExceptionCaptureStack;
use crate::transitive_error::context_manager::{
    apply_generator_context_manager, context_manager_effects, is_generator_context_manager,
};
use crate::transitive_error::decorator::apply_decorators;
use crate::transitive_error::exception::Exception;
use crate::transitive_error::extract::{
    extract_caught_exceptions, extract_errors, try_extract_exception_from_expr,
};
use crate::transitive_error::raise::FunctionRaise;

pub(crate) fn get_transitive_errors<'a>(
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &'a ExceptionCaptureStack,
) -> Vec<FunctionRaise> {
    let errors = FunctionTransitiveErrorVisitor::new(
        db,
        file,
        func,
        target_exceptions,
        call_stack.clone(),
        exception_capture_stack,
    )
    .transitive_errors();
    normalize_errors(apply_decorators(
        db,
        file,
        func,
        target_exceptions,
        call_stack,
        exception_capture_stack,
        errors,
    ))
}

pub(crate) struct FunctionTransitiveErrorVisitor<'a> {
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &'a Vec<Exception>,
    errors: Vec<FunctionRaise>,
    call_stack: CallStack,
    exception_capture_stack: ExceptionCaptureStack,
    try_block_exceptions: Vec<Vec<Exception>>,
    callable_errors: Vec<(String, Vec<FunctionRaise>)>,
    yield_errors: Vec<FunctionRaise>,
}

impl<'a> FunctionTransitiveErrorVisitor<'a> {
    pub(crate) fn new(
        db: &'a dyn Db,
        file: File,
        func: &'a StmtFunctionDef,
        target_exceptions: &'a Vec<Exception>,
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
            exception_capture_stack: exception_capture_stack.clone(),
            try_block_exceptions: vec![],
            callable_errors: vec![],
            yield_errors: vec![],
        }
    }

    pub(crate) fn with_callable_errors(
        mut self,
        callable_errors: Vec<(String, Vec<FunctionRaise>)>,
    ) -> Self {
        self.callable_errors = callable_errors;
        self
    }

    pub(crate) fn with_yield_errors(mut self, yield_errors: Vec<FunctionRaise>) -> Self {
        self.yield_errors = yield_errors;
        self
    }

    pub(crate) fn transitive_errors(&mut self) -> Vec<FunctionRaise> {
        self.visit_body(&self.func.body);
        self.errors = normalize_errors(self.errors.clone());
        self.errors.clone()
    }

    fn visit_with_items(&mut self, items: &'a [WithItem], body: &'a [Stmt], is_async: bool) {
        let Some((item, remaining_items)) = items.split_first() else {
            self.visit_body(body);
            return;
        };

        let is_generator =
            is_generator_context_manager(self.db, self.file, &item.context_expr, is_async);
        // Calling a generator function only creates the context manager. Its arguments are
        // evaluated now, while its body is evaluated around the yield below.
        if is_generator {
            self.visit_context_manager_arguments(&item.context_expr);
        } else {
            self.visit_expr(&item.context_expr);
        }
        let effects = context_manager_effects(
            self.db,
            self.file,
            &item.context_expr,
            is_async,
            self.target_exceptions,
            self.call_stack.clone(),
            &self.exception_capture_stack,
        );
        self.errors.extend(effects.enter_errors);

        // Multiple context managers are equivalent to nested `with` statements. An outer exit can
        // therefore suppress failures from an inner enter, body, or exit, but never its own enter.
        let protected_errors_start = self.errors.len();
        self.visit_with_items(remaining_items, body, is_async);
        if is_generator {
            let protected_errors = self.errors.split_off(protected_errors_start);
            if let Some(generator_errors) = apply_generator_context_manager(
                self.db,
                self.file,
                &item.context_expr,
                is_async,
                self.target_exceptions,
                self.call_stack.clone(),
                &self.exception_capture_stack,
                protected_errors,
            ) {
                self.errors.extend(generator_errors);
            }
            return;
        }
        if effects.suppresses_exceptions {
            self.errors.truncate(protected_errors_start);
        } else if !effects.suppressed_exceptions.is_empty() {
            let mut protected_errors = self.errors.split_off(protected_errors_start);
            protected_errors.retain(|error| {
                !effects
                    .suppressed_exceptions
                    .iter()
                    .any(|suppressed| error.name().is_subclass_of(suppressed))
            });
            self.errors.extend(protected_errors);
        }
        self.errors.extend(effects.exit_errors);
    }

    fn visit_context_manager_arguments(&mut self, expression: &'a Expr) {
        let Some(call) = expression.as_call_expr() else {
            return;
        };
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            self.visit_expr(&attribute.value);
        }
        for argument in &call.arguments.args {
            self.visit_expr(argument);
        }
        for keyword in &call.arguments.keywords {
            self.visit_expr(&keyword.value);
        }
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
                    for exc in handler_exceptions {
                        if exc.name == "BaseException" && exc.bases.is_empty() {
                            if let Some(try_exceptions) = self.try_block_exceptions.last() {
                                for try_exc in try_exceptions {
                                    if self.target_exceptions.is_empty()
                                        || self
                                            .target_exceptions
                                            .iter()
                                            .any(|t| try_exc.is_subclass_of(t))
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
                            && !self.target_exceptions.iter().any(|t| exc.is_subclass_of(t))
                        {
                            continue;
                        }
                        self.errors.extend_one(FunctionRaise::direct(
                            self.file,
                            exc.clone(),
                            raise.range,
                        ));
                    }
                }
            } else if let Some(Expr::Call(ExprCall { func, .. })) = raise.exc.as_deref()
                && let Some(exc) = try_extract_exception_from_expr(self.db, self.file, func)
            {
                if (self.target_exceptions.is_empty()
                    || self.target_exceptions.iter().any(|t| exc.is_subclass_of(t)))
                    && !self.exception_capture_stack.is_captured(&exc)
                {
                    self.errors
                        .extend_one(FunctionRaise::direct(self.file, exc, raise.range));
                }
            } else if let Some(Expr::Name(_name_expr)) = raise.exc.as_deref()
                && self.exception_capture_stack.in_handler()
                && let Some(handler_exceptions) = self
                    .exception_capture_stack
                    .get_current_handler_exceptions()
            {
                for exc in handler_exceptions {
                    if exc.name == "BaseException" && exc.bases.is_empty() {
                        continue;
                    }
                    if !self.target_exceptions.is_empty()
                        && !self.target_exceptions.iter().any(|t| exc.is_subclass_of(t))
                    {
                        continue;
                    }
                    self.errors.extend_one(FunctionRaise::direct(
                        self.file,
                        exc.clone(),
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
                let exc = error.name().clone();
                if !try_exceptions.iter().any(|t| exc.is_subclass_of(t)) {
                    try_exceptions.push(exc);
                }
            }

            let caught_exceptions = handlers
                .iter()
                .flat_map(|h| extract_caught_exceptions(self.db, self.file, h))
                .collect::<Vec<_>>();

            self.exception_capture_stack =
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
            self.exception_capture_stack = self.exception_capture_stack.pop();

            for handler in handlers {
                if let Some(except_handler) = handler.as_except_handler() {
                    let handler_exceptions = extract_caught_exceptions(self.db, self.file, handler);
                    self.exception_capture_stack = self
                        .exception_capture_stack
                        .push_handler_exceptions(handler_exceptions);
                    self.visit_body(&except_handler.body);
                    self.exception_capture_stack =
                        self.exception_capture_stack.pop_handler_exceptions();
                }
            }
            self.try_block_exceptions.pop();
            self.visit_body(orelse);
            self.visit_body(finalbody);
        } else if let Stmt::With(with_statement) = stmt {
            self.visit_with_items(
                &with_statement.items,
                &with_statement.body,
                with_statement.is_async,
            );
        } else {
            walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if matches!(expr, Expr::Yield(_)) {
            self.errors.extend(
                self.yield_errors
                    .clone()
                    .into_iter()
                    .filter(|error| !self.exception_capture_stack.is_captured(error.name())),
            );
        } else if let Expr::Call(call) = expr {
            // Calling a decorated generator creates a context manager without running its body.
            // The body is analyzed when that value is used by a matching `with` statement.
            if is_generator_context_manager(self.db, self.file, expr, false)
                || is_generator_context_manager(self.db, self.file, expr, true)
            {
                self.visit_context_manager_arguments(expr);
                return;
            }
            let callable_errors = call.func.as_name_expr().and_then(|name| {
                self.callable_errors
                    .iter()
                    .find(|(callable, _)| callable == name.id.as_str())
                    .map(|(_, errors)| errors.clone())
            });
            if let Some(callable_errors) = callable_errors {
                self.errors.extend(
                    callable_errors
                        .into_iter()
                        .filter(|error| !self.exception_capture_stack.is_captured(error.name())),
                );
            } else if let Some(defs) =
                definitions_for_call_func(self.db, self.file, *call.func.clone())
            {
                for def in defs {
                    if let ResolvedDefinition::Definition(def) = def {
                        let definition_file = def.file(self.db);
                        let definition_path = match definition_file.path(self.db) {
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
                            self.target_exceptions.clone(),
                            self.call_stack.clone(),
                            self.exception_capture_stack.clone(),
                        )
                        .to_vec();
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

pub(crate) fn normalize_errors(errors: Vec<FunctionRaise>) -> Vec<FunctionRaise> {
    errors
        .into_iter()
        .sorted_by_key(|error| error.sort_key())
        .chunk_by(|error| error.group_key())
        .into_iter()
        .map(|(_, errors)| errors.into_iter().next().unwrap())
        .collect()
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
