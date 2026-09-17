use itertools::Itertools;
use ruff_db::files::File;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, ExprCall, Stmt, StmtFunctionDef, StmtTry, WithItem};
use ruff_text_size::Ranged;
use ty_project::Db;
use ty_python_semantic::ResolvedDefinition;

use crate::AnalysisOptions;
use crate::transitive_error::analysis::{
    AnalysisGap, AnalysisGapImpact, AnalysisGapKind, FunctionAnalysis,
};
use crate::transitive_error::call_stack::CallStack;
use crate::transitive_error::capture_stack::ExceptionCaptureStack;
use crate::transitive_error::context_manager::{
    apply_configured_context_manager_effects, apply_generator_context_manager,
    context_manager_effects, is_generator_context_manager,
};
use crate::transitive_error::decorator::apply_decorators;
use crate::transitive_error::exception::Exception;
use crate::transitive_error::extract::{
    definitions_for_expression, extract_analysis, extract_caught_exceptions, is_module_attribute,
    is_opaque_call_target, try_extract_exception_from_expr,
};
use crate::transitive_error::higher_order::{
    CallableErrors, callable_analysis_for_call, eager_stdlib_callback_errors,
};
use crate::transitive_error::raise::FunctionRaise;

pub(crate) fn get_transitive_analysis<'a>(
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &'a ExceptionCaptureStack,
    analysis_options: &'a AnalysisOptions,
) -> FunctionAnalysis {
    get_transitive_analysis_with_callable_errors(
        db,
        file,
        func,
        target_exceptions,
        call_stack,
        exception_capture_stack,
        analysis_options,
        vec![],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn get_transitive_analysis_with_callable_errors<'a>(
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &'a ExceptionCaptureStack,
    analysis_options: &'a AnalysisOptions,
    callable_errors: CallableErrors,
) -> FunctionAnalysis {
    let analysis = FunctionTransitiveErrorVisitor::new(
        db,
        file,
        func,
        target_exceptions,
        call_stack.clone(),
        exception_capture_stack,
        analysis_options,
    )
    .with_callable_errors(callable_errors)
    .transitive_analysis();
    apply_decorators(
        db,
        file,
        func,
        target_exceptions,
        call_stack,
        exception_capture_stack,
        analysis_options,
        analysis,
    )
}

pub(crate) struct FunctionTransitiveErrorVisitor<'a> {
    db: &'a dyn Db,
    file: File,
    func: &'a StmtFunctionDef,
    target_exceptions: &'a Vec<Exception>,
    errors: Vec<FunctionRaise>,
    gaps: Vec<AnalysisGap>,
    call_stack: CallStack,
    exception_capture_stack: ExceptionCaptureStack,
    analysis_options: &'a AnalysisOptions,
    try_block_exceptions: Vec<Vec<Exception>>,
    callable_errors: CallableErrors,
    yield_errors: Vec<FunctionRaise>,
    opaque_values: std::collections::HashSet<String>,
}

impl<'a> FunctionTransitiveErrorVisitor<'a> {
    pub(crate) fn new(
        db: &'a dyn Db,
        file: File,
        func: &'a StmtFunctionDef,
        target_exceptions: &'a Vec<Exception>,
        call_stack: CallStack,
        exception_capture_stack: &'a ExceptionCaptureStack,
        analysis_options: &'a AnalysisOptions,
    ) -> Self {
        Self {
            db,
            file,
            func,
            target_exceptions,
            errors: vec![],
            gaps: vec![],
            call_stack,
            exception_capture_stack: exception_capture_stack.clone(),
            analysis_options,
            try_block_exceptions: vec![],
            callable_errors: vec![],
            yield_errors: vec![],
            opaque_values: std::collections::HashSet::new(),
        }
    }

    pub(crate) fn with_callable_errors(mut self, callable_errors: CallableErrors) -> Self {
        self.callable_errors = callable_errors;
        self
    }

    pub(crate) fn with_yield_errors(mut self, yield_errors: Vec<FunctionRaise>) -> Self {
        self.yield_errors = yield_errors;
        self
    }

    pub(crate) fn transitive_analysis(&mut self) -> FunctionAnalysis {
        self.visit_body(&self.func.body);
        self.errors = normalize_errors(self.errors.clone());
        let mut seen = std::collections::HashSet::new();
        self.gaps.retain(|gap| seen.insert(gap.clone()));
        FunctionAnalysis {
            errors: self.errors.clone(),
            gaps: self.gaps.clone(),
        }
    }

    fn visit_with_items(&mut self, items: &'a [WithItem], body: &'a [Stmt], is_async: bool) {
        let Some((item, remaining_items)) = items.split_first() else {
            self.visit_body(body);
            return;
        };

        let opaque_context_manager = item
            .context_expr
            .as_call_expr()
            .is_some_and(|call| self.call_has_opaque_receiver(call));
        let is_generator = !opaque_context_manager
            && is_generator_context_manager(self.db, self.file, &item.context_expr, is_async);
        // Calling a generator function only creates the context manager. Its arguments are
        // evaluated now, while its body is evaluated around the yield below.
        if is_generator {
            self.visit_context_manager_arguments(&item.context_expr);
        } else {
            self.visit_expr(&item.context_expr);
        }
        if opaque_context_manager {
            if let Some(optional_vars) = item.optional_vars.as_deref() {
                self.mark_opaque_target(optional_vars);
            }
            self.gaps.push(AnalysisGap::new(
                AnalysisGapKind::UnmodeledContextManager,
                AnalysisGapImpact::Both,
                self.file,
                item.context_expr.range(),
                None,
            ));
            self.visit_with_items(remaining_items, body, is_async);
            return;
        }
        let effects = context_manager_effects(
            self.db,
            self.file,
            &item.context_expr,
            is_async,
            self.target_exceptions,
            self.call_stack.clone(),
            &self.exception_capture_stack,
            self.analysis_options,
        );
        self.gaps.extend(effects.gaps.clone());
        let configured_effects = apply_configured_context_manager_effects(
            self.db,
            self.file,
            &item.context_expr,
            self.analysis_options,
            Vec::new(),
        );
        self.gaps.extend(configured_effects.gaps);
        if !is_generator && !effects.recognized && !configured_effects.recognized {
            self.gaps.push(AnalysisGap::new(
                AnalysisGapKind::UnmodeledContextManager,
                AnalysisGapImpact::Both,
                self.file,
                item.context_expr.range(),
                None,
            ));
        }
        self.errors.extend(effects.enter_errors);
        if let Some(optional_vars) = item.optional_vars.as_deref() {
            // Resolving the runtime value returned by `__(a)enter__` requires the same inference
            // that can fail for native and highly dynamic context managers. Calls through the
            // bound value are therefore tracked as opaque analysis gaps.
            self.mark_opaque_target(optional_vars);
        }

        // Multiple context managers are equivalent to nested `with` statements. An outer exit can
        // therefore suppress failures from an inner enter, body, or exit, but never its own enter.
        let protected_errors_start = self.errors.len();
        self.visit_with_items(remaining_items, body, is_async);
        let protected_errors = self.errors.split_off(protected_errors_start);
        let configured_effects = apply_configured_context_manager_effects(
            self.db,
            self.file,
            &item.context_expr,
            self.analysis_options,
            protected_errors,
        );
        self.gaps.extend(configured_effects.gaps);
        if is_generator {
            if let Some(generator_analysis) = apply_generator_context_manager(
                self.db,
                self.file,
                &item.context_expr,
                is_async,
                self.target_exceptions,
                self.call_stack.clone(),
                &self.exception_capture_stack,
                self.analysis_options,
                configured_effects.errors,
            ) {
                self.errors.extend(generator_analysis.errors);
                self.gaps.extend(generator_analysis.gaps);
            }
            return;
        }
        let mut protected_errors = configured_effects.errors;
        if effects.suppresses_exceptions {
            protected_errors.clear();
        } else if !effects.suppressed_exceptions.is_empty() {
            protected_errors.retain(|error| {
                !effects
                    .suppressed_exceptions
                    .iter()
                    .any(|suppressed| error.name().is_subclass_of(suppressed))
            });
        }
        self.errors.extend(protected_errors);
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

    fn expression_is_opaque(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Await(await_expression) => self.expression_is_opaque(&await_expression.value),
            Expr::Call(call) => self.call_is_opaque(call),
            Expr::Name(name) => self.opaque_values.contains(name.id.as_str()),
            _ => false,
        }
    }

    fn call_has_opaque_receiver(&self, call: &ExprCall) -> bool {
        call.func
            .as_attribute_expr()
            .is_some_and(|attribute| self.is_opaque_receiver(&attribute.value))
    }

    fn call_is_opaque(&self, call: &ExprCall) -> bool {
        is_opaque_call_target(self.db, self.file, &call.func) || self.call_has_opaque_receiver(call)
    }

    fn is_opaque_receiver(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => self.opaque_values.contains(name.id.as_str()),
            Expr::Attribute(attribute) => self.is_opaque_receiver(&attribute.value),
            Expr::Call(call) => self.call_is_opaque(call),
            _ => false,
        }
    }

    fn mark_opaque_targets(&mut self, targets: &[Expr]) {
        for target in targets {
            self.mark_opaque_target(target);
        }
    }

    fn mark_opaque_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => {
                self.opaque_values.insert(name.id.to_string());
            }
            Expr::Tuple(tuple) => self.mark_opaque_targets(&tuple.elts),
            Expr::List(list) => self.mark_opaque_targets(&list.elts),
            _ => {}
        }
    }
}

impl<'a> Visitor<'a> for FunctionTransitiveErrorVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(assign) = stmt
            && self.expression_is_opaque(&assign.value)
        {
            // Preserve the opacity of values returned by third-party constructors and calls so
            // subsequent method calls do not re-enter unsafe type inference.
            self.mark_opaque_targets(&assign.targets);
        }
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
            let opaque_receiver = self.call_has_opaque_receiver(call);
            // Calling a decorated generator creates a context manager without running its body.
            // The body is analyzed when that value is used by a matching `with` statement.
            if !opaque_receiver
                && (is_generator_context_manager(self.db, self.file, expr, false)
                    || is_generator_context_manager(self.db, self.file, expr, true))
            {
                self.visit_context_manager_arguments(expr);
                return;
            }
            if opaque_receiver {
                self.gaps.push(AnalysisGap::new(
                    AnalysisGapKind::OpaqueCall,
                    AnalysisGapImpact::MayMissErrors,
                    self.file,
                    call.range(),
                    call.func
                        .as_attribute_expr()
                        .map(|attribute| attribute.attr.to_string()),
                ));
                walk_expr(self, expr);
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
                let definitions_empty = defs.is_empty();
                let module_attribute = is_module_attribute(self.db, self.file, &call.func);
                let opaque_call_target = is_opaque_call_target(self.db, self.file, &call.func);
                if definitions_empty {
                    self.gaps.push(AnalysisGap::new(
                        if module_attribute {
                            AnalysisGapKind::OpaqueCall
                        } else {
                            AnalysisGapKind::DynamicCall
                        },
                        AnalysisGapImpact::MayMissErrors,
                        self.file,
                        call.range(),
                        None,
                    ));
                }
                let callable_analysis = if opaque_call_target {
                    Default::default()
                } else {
                    callable_analysis_for_call(
                        self.db,
                        self.file,
                        call,
                        self.target_exceptions,
                        self.call_stack.clone(),
                        &self.exception_capture_stack,
                        &self.callable_errors,
                        self.analysis_options,
                    )
                };
                self.gaps.extend(callable_analysis.gaps);
                self.errors.extend(
                    eager_stdlib_callback_errors(
                        self.db,
                        self.file,
                        call,
                        &callable_analysis.errors,
                    )
                    .into_iter()
                    .filter(|error| !self.exception_capture_stack.is_captured(error.name())),
                );
                let mut resolved_definition = false;
                for def in defs {
                    if let ResolvedDefinition::Definition(def) = def {
                        resolved_definition = true;
                        let definition_file = def.file(self.db);
                        let definition_path = match definition_file.path(self.db) {
                            ruff_db::files::FilePath::System(path) => path,
                            ruff_db::files::FilePath::SystemVirtual(_) => {
                                self.gaps.push(AnalysisGap::new(
                                    AnalysisGapKind::OpaqueCall,
                                    AnalysisGapImpact::MayMissErrors,
                                    self.file,
                                    call.range(),
                                    def.name(self.db).map(|name| name.to_string()),
                                ));
                                continue;
                            }
                            ruff_db::files::FilePath::Vendored(_) => continue,
                        };
                        if let Some(name) = def.name(self.db) {
                            let key = (definition_path.as_str().into(), name.clone());
                            if self.call_stack.contains(&key) {
                                self.gaps.push(AnalysisGap::new(
                                    AnalysisGapKind::AnalysisCutoff,
                                    AnalysisGapImpact::MayMissErrors,
                                    self.file,
                                    call.range(),
                                    Some(name.to_string()),
                                ));
                                continue;
                            }
                        }
                        let transitive = extract_analysis(
                            self.db,
                            self.file,
                            call.range(),
                            definition_file,
                            def,
                            self.target_exceptions.clone(),
                            self.call_stack.clone(),
                            self.exception_capture_stack.clone(),
                            self.analysis_options.clone(),
                            callable_analysis.errors.clone(),
                        )
                        .clone();
                        self.errors.extend(
                            transitive
                                .errors
                                .into_iter()
                                .filter(|e| !self.exception_capture_stack.is_captured(e.name())),
                        );
                        self.gaps.extend(transitive.gaps);
                    }
                }
                if !resolved_definition && !definitions_empty {
                    self.gaps.push(AnalysisGap::new(
                        AnalysisGapKind::DynamicCall,
                        AnalysisGapImpact::MayMissErrors,
                        self.file,
                        call.range(),
                        None,
                    ));
                }
            } else {
                self.gaps.push(AnalysisGap::new(
                    AnalysisGapKind::DynamicCall,
                    AnalysisGapImpact::MayMissErrors,
                    self.file,
                    call.range(),
                    None,
                ));
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
    if matches!(func, Expr::Name(_) | Expr::Attribute(_)) {
        return Some(definitions_for_expression(db, file, &func));
    }
    None
}
