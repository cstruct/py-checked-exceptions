use std::collections::HashSet;

use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{
    Expr, Stmt, StmtFunctionDef,
    statement_visitor::{StatementVisitor, walk_stmt},
};
use ruff_text_size::Ranged;
use ty_project::Db;
use ty_python_semantic::{ResolvedDefinition, definitions_for_attribute, definitions_for_name};

use crate::{
    module::ModuleCollector,
    transitive_error::{
        call_stack::CallStack, capture_stack::ExceptionCaptureStack, exception::Exception,
        extract::resolve_alias, raise::FunctionRaise, visitor::FunctionTransitiveErrorVisitor,
    },
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_decorators(
    db: &dyn Db,
    file: File,
    function: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    mut errors: Vec<FunctionRaise>,
) -> Vec<FunctionRaise> {
    for decorator in function.decorator_list.iter().rev() {
        let (expression, is_factory) = match &decorator.expression {
            Expr::Call(call) => (call.func.as_ref(), true),
            expression => (expression, false),
        };
        let definitions = definitions_for_expression(db, file, expression);
        let mut transformed = Vec::new();
        let mut recognized = false;

        for definition in definitions {
            let ResolvedDefinition::Definition(definition) = definition else {
                continue;
            };
            let definition_file = definition.file(db);
            let module = parsed_module(db, definition_file).load(db);
            let Some((definition_file, definition)) =
                resolve_alias(db, &module, definition_file, definition)
            else {
                continue;
            };
            let module = parsed_module(db, definition_file).load(db);
            let mut collector = ModuleCollector::new();
            collector.init(&module);
            let full_range = definition.full_range(db, &module).range();

            for resolved_function in collector.find_functions(&full_range) {
                if is_factory {
                    for decorator_function in returned_functions(resolved_function) {
                        recognized |= transform_with_decorator(
                            db,
                            definition_file,
                            decorator_function,
                            target_exceptions,
                            call_stack.clone(),
                            exception_capture_stack,
                            &errors,
                            &mut transformed,
                        );
                    }
                } else {
                    recognized |= transform_with_decorator(
                        db,
                        definition_file,
                        resolved_function,
                        target_exceptions,
                        call_stack.clone(),
                        exception_capture_stack,
                        &errors,
                        &mut transformed,
                    );
                }
            }
        }

        if recognized {
            errors = transformed;
        }
    }
    errors
}

#[allow(clippy::too_many_arguments)]
fn transform_with_decorator(
    db: &dyn Db,
    file: File,
    decorator: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    errors: &[FunctionRaise],
    transformed: &mut Vec<FunctionRaise>,
) -> bool {
    let Some(parameter) = first_parameter_name(decorator) else {
        return false;
    };
    let wrappers = returned_functions(decorator);
    if wrappers.is_empty() {
        return false;
    }

    for wrapper in wrappers {
        let wrapper_errors = FunctionTransitiveErrorVisitor::new(
            db,
            file,
            wrapper,
            target_exceptions,
            call_stack.clone(),
            exception_capture_stack,
        )
        .with_callable_errors(vec![(parameter.to_string(), errors.to_vec())])
        .transitive_errors();
        transformed.extend(wrapper_errors);
    }
    true
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

fn first_parameter_name(function: &StmtFunctionDef) -> Option<&str> {
    function
        .parameters
        .posonlyargs
        .first()
        .or_else(|| function.parameters.args.first())
        .map(|parameter| parameter.name().as_str())
}

fn returned_functions(function: &StmtFunctionDef) -> Vec<&StmtFunctionDef> {
    let mut returns = ReturnNameCollector::default();
    returns.visit_body(&function.body);
    let mut functions = ReturnedFunctionCollector {
        names: &returns.names,
        functions: Vec::new(),
    };
    functions.visit_body(&function.body);
    functions.functions
}

#[derive(Default)]
struct ReturnNameCollector {
    names: HashSet<String>,
}

impl<'a> StatementVisitor<'a> for ReturnNameCollector {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) => {}
            Stmt::Return(return_statement) => {
                if let Some(name) = return_statement
                    .value
                    .as_deref()
                    .and_then(Expr::as_name_expr)
                {
                    self.names.insert(name.id.to_string());
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}

struct ReturnedFunctionCollector<'names, 'ast> {
    names: &'names HashSet<String>,
    functions: Vec<&'ast StmtFunctionDef>,
}

impl<'names, 'ast> StatementVisitor<'ast> for ReturnedFunctionCollector<'names, 'ast> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if let Stmt::FunctionDef(function) = statement {
            if self.names.contains(function.name.as_str()) {
                self.functions.push(function);
            }
        } else {
            walk_stmt(self, statement);
        }
    }
}
