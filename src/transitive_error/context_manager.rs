use ruff_db::{
    files::{File, FilePath},
    parsed::parsed_module,
};
use ruff_python_ast::{
    Expr, Stmt, StmtFunctionDef,
    statement_visitor::{StatementVisitor, walk_stmt},
};
use ruff_text_size::Ranged;
use ty_project::Db;
use ty_python_semantic::{
    ResolvedDefinition, definitions_for_attribute, definitions_for_name,
    semantic_index::definition::DefinitionKind,
};

use crate::{
    module::ModuleCollector,
    transitive_error::{
        call_stack::CallStack,
        capture_stack::ExceptionCaptureStack,
        exception::Exception,
        extract::{resolve_alias, try_extract_exception_from_expr},
        raise::FunctionRaise,
        visitor::{FunctionTransitiveErrorVisitor, get_transitive_errors},
    },
};

#[derive(Default)]
pub(crate) struct ContextManagerEffects {
    pub(crate) enter_errors: Vec<FunctionRaise>,
    pub(crate) exit_errors: Vec<FunctionRaise>,
    pub(crate) suppresses_exceptions: bool,
    pub(crate) suppressed_exceptions: Vec<Exception>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn context_manager_effects(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    is_async: bool,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
) -> ContextManagerEffects {
    let resolution_expression = expression
        .as_call_expr()
        .map(|call| call.func.as_ref())
        .unwrap_or(expression);
    let enter_name = if is_async { "__aenter__" } else { "__enter__" };
    let exit_name = if is_async { "__aexit__" } else { "__exit__" };
    let mut effects = ContextManagerEffects::default();
    let mut exit_methods = 0;
    let mut all_exit_methods_suppress = true;

    collect_effects(
        db,
        file,
        resolution_expression,
        file,
        expression,
        enter_name,
        exit_name,
        target_exceptions,
        call_stack,
        exception_capture_stack,
        &mut effects,
        &mut exit_methods,
        &mut all_exit_methods_suppress,
        true,
        true,
        0,
    );

    effects.suppresses_exceptions = exit_methods > 0 && all_exit_methods_suppress;
    if let Some(suppressed_exceptions) = contextlib_suppressed_exceptions(db, file, expression) {
        effects.suppressed_exceptions = suppressed_exceptions;
    }
    effects
}

pub(crate) fn is_generator_context_manager(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    is_async: bool,
) -> bool {
    generator_context_manager_functions(db, file, expression, is_async, |_, _| {})
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_generator_context_manager(
    db: &dyn Db,
    call_file: File,
    expression: &Expr,
    is_async: bool,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    body_errors: Vec<FunctionRaise>,
) -> Option<Vec<FunctionRaise>> {
    let mut errors = Vec::new();
    let found = generator_context_manager_functions(
        db,
        call_file,
        expression,
        is_async,
        |definition_file, function| {
            let path = match definition_file.path(db) {
                FilePath::System(path) => path,
                FilePath::SystemVirtual(_) | FilePath::Vendored(_) => return,
            };
            let key = (path.as_str().into(), function.name.as_str().into());
            if call_stack.contains(&key) {
                return;
            }
            let function_errors = FunctionTransitiveErrorVisitor::new(
                db,
                definition_file,
                function,
                target_exceptions,
                call_stack.push(key),
                exception_capture_stack,
            )
            .with_yield_errors(body_errors.clone())
            .transitive_errors();
            errors.extend(function_errors.into_iter().map(|error| {
                if body_errors.contains(&error) {
                    error
                } else {
                    error.transitive(call_file, expression.range())
                }
            }));
        },
    );
    found.then_some(errors)
}

fn generator_context_manager_functions(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    is_async: bool,
    mut visit: impl FnMut(File, &StmtFunctionDef),
) -> bool {
    collect_generator_context_manager_functions(db, file, expression, is_async, &mut visit, 0)
}

fn collect_generator_context_manager_functions(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    is_async: bool,
    visit: &mut impl FnMut(File, &StmtFunctionDef),
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let resolution_expression = expression
        .as_call_expr()
        .map(|call| call.func.as_ref())
        .unwrap_or(expression);
    let expected_decorator = if is_async {
        "asynccontextmanager"
    } else {
        "contextmanager"
    };
    let mut found = false;
    for definition in definitions_for_expression(db, file, resolution_expression) {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        let definition_file = definition.file(db);
        let module = parsed_module(db, definition_file).load(db);
        if let DefinitionKind::Assignment(assignment) = definition.kind(db) {
            let value = assignment.value(&module);
            if matches!(value, Expr::Call(_) | Expr::Name(_) | Expr::Attribute(_)) {
                found |= collect_generator_context_manager_functions(
                    db,
                    definition_file,
                    value,
                    is_async,
                    visit,
                    depth + 1,
                );
            }
            continue;
        }
        let Some((definition_file, definition)) =
            resolve_alias(db, &module, definition_file, definition)
        else {
            continue;
        };
        let module = parsed_module(db, definition_file).load(db);
        let mut collector = ModuleCollector::new();
        collector.init(&module);
        let full_range = definition.full_range(db, &module).range();
        for function in collector.find_functions(&full_range) {
            if function.decorator_list.iter().any(|decorator| {
                is_contextlib_member(
                    db,
                    definition_file,
                    &decorator.expression,
                    expected_decorator,
                )
            }) {
                found = true;
                visit(definition_file, function);
            }
        }
    }
    found
}

#[allow(clippy::too_many_arguments)]
fn collect_effects(
    db: &dyn Db,
    resolution_file: File,
    resolution_expression: &Expr,
    call_file: File,
    call_expression: &Expr,
    enter_name: &str,
    exit_name: &str,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    effects: &mut ContextManagerEffects,
    exit_methods: &mut usize,
    all_exit_methods_suppress: &mut bool,
    needs_enter: bool,
    needs_exit: bool,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    let definitions = definitions_for_expression(db, resolution_file, resolution_expression);
    for definition in definitions {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        let definition_file = definition.file(db);
        let module = parsed_module(db, definition_file).load(db);
        if let DefinitionKind::Assignment(assignment) = definition.kind(db) {
            let value = assignment.value(&module);
            let nested_expression = value
                .as_call_expr()
                .map(|call| call.func.as_ref())
                .unwrap_or(value);
            if matches!(nested_expression, Expr::Name(_) | Expr::Attribute(_)) {
                collect_effects(
                    db,
                    definition_file,
                    nested_expression,
                    call_file,
                    call_expression,
                    enter_name,
                    exit_name,
                    target_exceptions,
                    call_stack.clone(),
                    exception_capture_stack,
                    effects,
                    exit_methods,
                    all_exit_methods_suppress,
                    needs_enter,
                    needs_exit,
                    depth + 1,
                );
            }
            continue;
        }
        let Some((definition_file, definition)) =
            resolve_alias(db, &module, definition_file, definition)
        else {
            continue;
        };
        let module = parsed_module(db, definition_file).load(db);
        let mut collector = ModuleCollector::new();
        collector.init(&module);
        let full_range = definition.full_range(db, &module).range();
        let Some(class) = collector.find_class(&full_range) else {
            continue;
        };

        let mut found_enter = false;
        let mut found_exit = false;
        for statement in &class.body {
            let Stmt::FunctionDef(method) = statement else {
                continue;
            };
            if needs_enter && method.name.as_str() == enter_name {
                found_enter = true;
                effects.enter_errors.extend(method_errors(
                    db,
                    call_file,
                    definition_file,
                    call_expression,
                    method,
                    target_exceptions,
                    call_stack.clone(),
                    exception_capture_stack,
                ));
            } else if needs_exit && method.name.as_str() == exit_name {
                found_exit = true;
                *exit_methods += 1;
                *all_exit_methods_suppress &= always_returns_true(method);
                effects.exit_errors.extend(method_errors(
                    db,
                    call_file,
                    definition_file,
                    call_expression,
                    method,
                    target_exceptions,
                    call_stack.clone(),
                    exception_capture_stack,
                ));
            }
        }

        if (needs_enter && !found_enter) || (needs_exit && !found_exit) {
            for base in class.bases() {
                collect_effects(
                    db,
                    definition_file,
                    base,
                    call_file,
                    call_expression,
                    enter_name,
                    exit_name,
                    target_exceptions,
                    call_stack.clone(),
                    exception_capture_stack,
                    effects,
                    exit_methods,
                    all_exit_methods_suppress,
                    needs_enter && !found_enter,
                    needs_exit && !found_exit,
                    depth + 1,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn method_errors(
    db: &dyn Db,
    call_file: File,
    definition_file: File,
    expression: &Expr,
    method: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
) -> Vec<FunctionRaise> {
    let path = match definition_file.path(db) {
        FilePath::System(path) => path,
        FilePath::SystemVirtual(_) | FilePath::Vendored(_) => return Vec::new(),
    };
    let key = (path.as_str().into(), method.name.as_str().into());
    if call_stack.contains(&key) {
        return Vec::new();
    }
    get_transitive_errors(
        db,
        definition_file,
        method,
        target_exceptions,
        call_stack.push(key),
        exception_capture_stack,
    )
    .iter()
    .map(|error| error.transitive(call_file, expression.range()))
    .collect()
}

fn definitions_for_expression<'a>(
    db: &'a dyn Db,
    file: File,
    expression: &Expr,
) -> Vec<ResolvedDefinition<'a>> {
    match expression {
        Expr::Name(name) => definitions_for_name(db, file, name),
        Expr::Attribute(attribute) => definitions_for_attribute(db, file, attribute),
        _ => Vec::new(),
    }
}

fn contextlib_suppressed_exceptions(
    db: &dyn Db,
    file: File,
    expression: &Expr,
) -> Option<Vec<Exception>> {
    let call = expression.as_call_expr()?;
    if !is_contextlib_member(db, file, &call.func, "suppress") {
        return None;
    }

    Some(
        call.arguments
            .args
            .iter()
            .flat_map(|argument| suppressed_exception_types(db, file, argument))
            .collect(),
    )
}

fn is_contextlib_member(db: &dyn Db, file: File, expression: &Expr, member: &str) -> bool {
    definitions_for_expression(db, file, expression)
        .into_iter()
        .any(|definition| {
            let ResolvedDefinition::Definition(definition) = definition else {
                return false;
            };
            definition.name(db).is_some_and(|name| name == member)
                && matches!(
                    definition.file(db).path(db),
                    FilePath::Vendored(path) if path.file_name() == Some("contextlib.pyi")
                )
        })
}

fn suppressed_exception_types(db: &dyn Db, file: File, expression: &Expr) -> Vec<Exception> {
    if let Expr::Tuple(tuple) = expression {
        return tuple
            .elts
            .iter()
            .flat_map(|element| suppressed_exception_types(db, file, element))
            .collect();
    }
    try_extract_exception_from_expr(db, file, expression)
        .into_iter()
        .collect()
}

fn always_returns_true(function: &StmtFunctionDef) -> bool {
    let Some(Stmt::Return(last_return)) = function.body.last() else {
        return false;
    };
    if !last_return.value.as_deref().is_some_and(is_true_literal) {
        return false;
    }

    let mut returns = ReturnValueCollector::default();
    returns.visit_body(&function.body);
    !returns.values.is_empty() && returns.values.into_iter().all(is_true_literal)
}

fn is_true_literal(expression: &Expr) -> bool {
    expression
        .as_boolean_literal_expr()
        .is_some_and(|literal| literal.value)
}

#[derive(Default)]
struct ReturnValueCollector<'a> {
    values: Vec<&'a Expr>,
}

impl<'a> StatementVisitor<'a> for ReturnValueCollector<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) => {}
            Stmt::Return(return_statement) => {
                if let Some(value) = return_statement.value.as_deref() {
                    self.values.push(value);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}
