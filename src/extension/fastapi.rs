use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{Expr, ExprCall, Operator, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;
use ty_python_semantic::semantic_index::definition::DefinitionKind;
use ty_python_semantic::{ResolvedDefinition, definitions_for_attribute, definitions_for_name};

use crate::{
    module::ModuleCollector,
    transitive_error::{
        analysis::{AnalysisGap, AnalysisGapImpact, AnalysisGapKind, FunctionAnalysis},
        call_stack::CallStack,
        capture_stack::ExceptionCaptureStack,
        exception::{Exception, canonical_exception_expression},
        extract::{extract_analysis, resolve_alias},
        visitor::{get_transitive_analysis, normalize_errors},
    },
};

pub(crate) fn documented_exceptions(
    db: &dyn Db,
    file: File,
    function: &StmtFunctionDef,
) -> Vec<(TextRange, String)> {
    function
        .decorator_list
        .iter()
        .filter_map(|decorator| {
            let Expr::Call(call) = &decorator.expression else {
                return None;
            };
            is_route_decorator(&call.func)
                .then(|| call.arguments.find_keyword("responses"))
                .flatten()
        })
        .filter_map(|responses| responses.value.as_dict_expr())
        .flat_map(|responses| responses.iter_values())
        .filter_map(Expr::as_dict_expr)
        .flat_map(|response| response.iter())
        .filter(|item| {
            item.key.as_ref().is_some_and(|key| {
                key.as_string_literal_expr()
                    .is_some_and(|key| key.value.to_str() == "model")
            })
        })
        .flat_map(|item| response_model_names(db, file, &item.value))
        .collect()
}

pub(crate) fn apply_dependency_injection(
    db: &dyn Db,
    file: File,
    function: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    mut analysis: FunctionAnalysis,
) -> FunctionAnalysis {
    let route_decorator_ranges = function
        .decorator_list
        .iter()
        .filter(|decorator| {
            decorator
                .expression
                .as_call_expr()
                .is_some_and(|call| is_route_decorator(&call.func))
        })
        .map(|decorator| decorator.expression.range())
        .collect::<Vec<_>>();
    // Core analysis treats decorators as arbitrary Python. Once a route decorator is owned by
    // this extension, discard those provisional gaps (including calls in `dependencies=[...]`)
    // and replace unresolved dependency shapes with extension-specific gaps below.
    analysis.gaps.retain(|gap| {
        !route_decorator_ranges.iter().any(|route_range| {
            route_range.start() <= gap.range().start() && route_range.end() >= gap.range().end()
        })
    });

    let mut dependencies = Vec::new();
    collect_parameter_dependencies(db, file, function, &mut dependencies);
    collect_route_dependencies(function, &mut dependencies);

    for dependency in dependencies {
        let Some(callable) = dependency.callable else {
            analysis.gaps.push(AnalysisGap::new(
                AnalysisGapKind::UnsupportedCallback,
                AnalysisGapImpact::MayMissErrors,
                file,
                dependency.range,
                None,
            ));
            continue;
        };
        let dependency_analysis =
            dependency_expression_analysis(db, file, callable, target_exceptions, CallStack::new());
        analysis.errors.extend(dependency_analysis.errors);
        analysis.gaps.extend(dependency_analysis.gaps);
    }
    let alias_analysis = aliased_parameter_dependency_analysis(
        db,
        file,
        function,
        target_exceptions,
        CallStack::new(),
    );
    analysis.errors.extend(alias_analysis.errors);
    analysis.gaps.extend(alias_analysis.gaps);

    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

struct Dependency<'a> {
    callable: Option<&'a Expr>,
    range: TextRange,
}

fn collect_parameter_dependencies<'a>(
    db: &dyn Db,
    file: File,
    function: &'a StmtFunctionDef,
    dependencies: &mut Vec<Dependency<'a>>,
) {
    for parameter in function.parameters.iter_non_variadic_params() {
        if let Some(default) = parameter.default()
            && let Some(call) = dependency_marker_call(default)
        {
            let annotation = parameter.annotation();
            let handled_by_annotation_alias = dependency_call_has_no_callable(call)
                && annotation.is_some_and(|annotation| {
                    aliased_annotation_has_dependency(db, file, annotation, 0)
                });
            if !handled_by_annotation_alias {
                dependencies.push(dependency_from_call(call, annotation, default.range()));
            }
        }
        let Some(annotation) = parameter.annotation() else {
            continue;
        };
        collect_annotated_dependencies(annotation, dependencies);
    }
}

fn dependency_call_has_no_callable(call: &ExprCall) -> bool {
    call.arguments.args.is_empty() && call.arguments.find_keyword("dependency").is_none()
}

fn aliased_annotation_has_dependency(
    db: &dyn Db,
    resolution_file: File,
    annotation: &Expr,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    definitions_for_expression(db, resolution_file, annotation)
        .into_iter()
        .any(|definition| {
            let ResolvedDefinition::Definition(definition) = definition else {
                return false;
            };
            let definition_file = definition.file(db);
            let module = parsed_module(db, definition_file).load(db);
            let value = match definition.kind(db) {
                DefinitionKind::Assignment(assignment) => assignment.value(&module),
                DefinitionKind::TypeAlias(type_alias) => &type_alias.node(&module).value,
                _ => return false,
            };
            let mut dependencies = Vec::new();
            collect_annotated_dependencies(value, &mut dependencies);
            !dependencies.is_empty()
                || matches!(value, Expr::Name(_) | Expr::Attribute(_))
                    && aliased_annotation_has_dependency(db, definition_file, value, depth + 1)
        })
}

fn collect_annotated_dependencies<'a>(
    annotation: &'a Expr,
    dependencies: &mut Vec<Dependency<'a>>,
) {
    let Expr::Subscript(annotated) = annotation else {
        return;
    };
    if type_name(&annotated.value) != Some("Annotated") {
        return;
    }
    let Expr::Tuple(elements) = annotated.slice.as_ref() else {
        return;
    };
    let Some(inferred_callable) = elements.elts.first() else {
        return;
    };
    for metadata in elements.elts.iter().skip(1) {
        if let Some(call) = dependency_marker_call(metadata) {
            dependencies.push(dependency_from_call(
                call,
                Some(inferred_callable),
                metadata.range(),
            ));
        }
    }
}

fn collect_route_dependencies<'a>(
    function: &'a StmtFunctionDef,
    dependencies: &mut Vec<Dependency<'a>>,
) {
    for decorator in &function.decorator_list {
        let Expr::Call(call) = &decorator.expression else {
            continue;
        };
        if !is_route_decorator(&call.func) {
            continue;
        }
        let Some(keyword) = call.arguments.find_keyword("dependencies") else {
            continue;
        };
        collect_dependency_markers(&keyword.value, dependencies);
    }
}

fn collect_dependency_markers<'a>(expression: &'a Expr, dependencies: &mut Vec<Dependency<'a>>) {
    if let Some(call) = dependency_marker_call(expression) {
        dependencies.push(dependency_from_call(call, None, expression.range()));
        return;
    }
    let elements = match expression {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::Set(set) => &set.elts,
        Expr::Call(_) => {
            dependencies.push(Dependency {
                callable: Some(expression),
                range: expression.range(),
            });
            return;
        }
        _ => {
            dependencies.push(Dependency {
                callable: None,
                range: expression.range(),
            });
            return;
        }
    };
    for element in elements {
        collect_dependency_markers(element, dependencies);
    }
}

fn dependency_from_call<'a>(
    call: &'a ExprCall,
    inferred_callable: Option<&'a Expr>,
    range: TextRange,
) -> Dependency<'a> {
    let callable = call
        .arguments
        .args
        .first()
        .or_else(|| {
            call.arguments
                .find_keyword("dependency")
                .map(|keyword| &keyword.value)
        })
        .or(inferred_callable);
    Dependency { callable, range }
}

fn dependency_marker_call(expression: &Expr) -> Option<&ExprCall> {
    let call = expression.as_call_expr()?;
    matches!(type_name(&call.func), Some("Depends" | "Security")).then_some(call)
}

fn dependency_expression_analysis(
    db: &dyn Db,
    call_file: File,
    expression: &Expr,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    dependency_expression_analysis_at(
        db,
        call_file,
        expression,
        call_file,
        expression.range(),
        target_exceptions,
        call_stack,
    )
}

#[allow(clippy::too_many_arguments)]
fn dependency_expression_analysis_at(
    db: &dyn Db,
    resolution_file: File,
    expression: &Expr,
    call_file: File,
    call_range: TextRange,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    if let Expr::Call(call) = expression {
        return dependency_factory_or_object_analysis(
            db,
            resolution_file,
            call,
            call_file,
            call_range,
            target_exceptions,
            call_stack,
        );
    }

    let definitions = definitions_for_expression(db, resolution_file, expression);
    if definitions.is_empty() {
        return unsupported_dependency(call_file, call_range);
    }

    let mut analysis = FunctionAnalysis::default();
    let mut resolved = false;
    for definition in definitions {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        resolved = true;
        let definition_file = definition.file(db);
        let core_analysis = extract_analysis(
            db,
            call_file,
            call_range,
            definition_file,
            definition,
            target_exceptions.clone(),
            call_stack.clone(),
            ExceptionCaptureStack::new(),
            vec![],
        );
        analysis.errors.extend(core_analysis.errors);
        analysis.gaps.extend(core_analysis.gaps);
        let injected_analysis = nested_dependency_analysis(
            db,
            call_file,
            call_range,
            definition_file,
            definition,
            target_exceptions,
            call_stack.clone(),
        );
        analysis.errors.extend(injected_analysis.errors);
        analysis.gaps.extend(injected_analysis.gaps);
    }
    if !resolved {
        analysis
            .gaps
            .extend(unsupported_dependency(call_file, call_range).gaps);
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

#[allow(clippy::too_many_arguments)]
fn dependency_factory_or_object_analysis(
    db: &dyn Db,
    resolution_file: File,
    call: &ExprCall,
    call_file: File,
    call_range: TextRange,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    let definitions = definitions_for_expression(db, resolution_file, &call.func);
    let mut analysis = FunctionAnalysis::default();
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

        if let Some(class) = collector.find_class(&full_range) {
            for statement in &class.body {
                let Stmt::FunctionDef(method) = statement else {
                    continue;
                };
                if method.name.as_str() != "__call__" {
                    continue;
                }
                recognized = true;
                let key = (
                    definition_file.path(db).as_str().into(),
                    method.name.as_str().into(),
                );
                if call_stack.contains(&key) {
                    analysis.gaps.push(AnalysisGap::new(
                        AnalysisGapKind::AnalysisCutoff,
                        AnalysisGapImpact::MayMissErrors,
                        call_file,
                        call_range,
                        Some(class.name.to_string()),
                    ));
                    continue;
                }
                let method_stack = call_stack.push(key);
                let method_analysis = get_transitive_analysis(
                    db,
                    definition_file,
                    method,
                    target_exceptions,
                    method_stack.clone(),
                    &ExceptionCaptureStack::new(),
                );
                analysis.errors.extend(
                    method_analysis
                        .errors
                        .into_iter()
                        .map(|error| error.transitive(call_file, call_range)),
                );
                analysis.gaps.extend(method_analysis.gaps);
                let injected = injected_dependencies_for_function(
                    db,
                    definition_file,
                    method,
                    target_exceptions,
                    method_stack,
                );
                analysis.errors.extend(
                    injected
                        .errors
                        .into_iter()
                        .map(|error| error.transitive(call_file, call_range)),
                );
                analysis.gaps.extend(injected.gaps);
            }
        }

        for factory in collector.find_functions(&full_range) {
            let mut returns = ReturnExpressionCollector::default();
            returns.visit_body(&factory.body);
            for returned in returns.expressions {
                let Some(marker) = dependency_marker_call(returned) else {
                    continue;
                };
                recognized = true;
                let dependency = dependency_from_call(marker, None, returned.range());
                let Some(callable) = dependency.callable else {
                    analysis.gaps.push(AnalysisGap::new(
                        AnalysisGapKind::UnsupportedCallback,
                        AnalysisGapImpact::MayMissErrors,
                        call_file,
                        call_range,
                        None,
                    ));
                    continue;
                };
                let returned_analysis = dependency_expression_analysis_at(
                    db,
                    definition_file,
                    callable,
                    call_file,
                    call_range,
                    target_exceptions,
                    call_stack.clone(),
                );
                analysis.errors.extend(returned_analysis.errors);
                analysis.gaps.extend(returned_analysis.gaps);
            }
        }
    }

    if !recognized {
        analysis
            .gaps
            .extend(unsupported_dependency(call_file, call_range).gaps);
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

fn aliased_parameter_dependency_analysis(
    db: &dyn Db,
    file: File,
    function: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    let mut analysis = FunctionAnalysis::default();
    for parameter in function.parameters.iter_non_variadic_params() {
        let Some(annotation) = parameter.annotation() else {
            continue;
        };
        if matches!(annotation, Expr::Subscript(_)) {
            continue;
        }
        let nested = aliased_annotation_dependency_analysis(
            db,
            file,
            annotation,
            file,
            annotation.range(),
            target_exceptions,
            call_stack.clone(),
            0,
        );
        analysis.errors.extend(nested.errors);
        analysis.gaps.extend(nested.gaps);
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

#[allow(clippy::too_many_arguments)]
fn aliased_annotation_dependency_analysis(
    db: &dyn Db,
    resolution_file: File,
    annotation: &Expr,
    call_file: File,
    call_range: TextRange,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
    depth: usize,
) -> FunctionAnalysis {
    if depth > 8 {
        return FunctionAnalysis {
            errors: vec![],
            gaps: vec![AnalysisGap::new(
                AnalysisGapKind::AnalysisCutoff,
                AnalysisGapImpact::MayMissErrors,
                call_file,
                call_range,
                None,
            )],
        };
    }
    let mut analysis = FunctionAnalysis::default();
    for definition in definitions_for_expression(db, resolution_file, annotation) {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        let definition_file = definition.file(db);
        let module = parsed_module(db, definition_file).load(db);
        let value = match definition.kind(db) {
            DefinitionKind::Assignment(assignment) => assignment.value(&module),
            DefinitionKind::TypeAlias(type_alias) => &type_alias.node(&module).value,
            _ => continue,
        };
        let mut dependencies = Vec::new();
        collect_annotated_dependencies(value, &mut dependencies);
        for dependency in dependencies {
            let Some(callable) = dependency.callable else {
                analysis.gaps.push(AnalysisGap::new(
                    AnalysisGapKind::UnsupportedCallback,
                    AnalysisGapImpact::MayMissErrors,
                    call_file,
                    call_range,
                    None,
                ));
                continue;
            };
            let nested = dependency_expression_analysis_at(
                db,
                definition_file,
                callable,
                call_file,
                call_range,
                target_exceptions,
                call_stack.clone(),
            );
            analysis.errors.extend(nested.errors);
            analysis.gaps.extend(nested.gaps);
        }
        if matches!(value, Expr::Name(_) | Expr::Attribute(_)) {
            let nested = aliased_annotation_dependency_analysis(
                db,
                definition_file,
                value,
                call_file,
                call_range,
                target_exceptions,
                call_stack.clone(),
                depth + 1,
            );
            analysis.errors.extend(nested.errors);
            analysis.gaps.extend(nested.gaps);
        }
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

#[derive(Default)]
struct ReturnExpressionCollector<'a> {
    expressions: Vec<&'a Expr>,
}

impl<'a> StatementVisitor<'a> for ReturnExpressionCollector<'a> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(_) => {}
            Stmt::Return(return_statement) => {
                if let Some(expression) = return_statement.value.as_deref() {
                    self.expressions.push(expression);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn nested_dependency_analysis<'db>(
    db: &'db dyn Db,
    call_file: File,
    call_range: TextRange,
    definition_file: File,
    definition: ty_python_semantic::semantic_index::definition::Definition<'db>,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    let module = parsed_module(db, definition_file).load(db);
    let Some((definition_file, definition)) =
        resolve_alias(db, &module, definition_file, definition)
    else {
        return FunctionAnalysis::default();
    };
    let module = parsed_module(db, definition_file).load(db);
    let mut collector = ModuleCollector::new();
    collector.init(&module);
    let full_range = definition.full_range(db, &module).range();
    let mut analysis = FunctionAnalysis::default();

    for function in collector.find_functions(&full_range) {
        let key = (
            definition_file.path(db).as_str().into(),
            function.name.as_str().into(),
        );
        if call_stack.contains(&key) {
            analysis.gaps.push(AnalysisGap::new(
                AnalysisGapKind::AnalysisCutoff,
                AnalysisGapImpact::MayMissErrors,
                call_file,
                call_range,
                Some(function.name.to_string()),
            ));
            continue;
        }
        let nested = injected_dependencies_for_function(
            db,
            definition_file,
            function,
            target_exceptions,
            call_stack.push(key),
        );
        analysis.errors.extend(
            nested
                .errors
                .into_iter()
                .map(|error| error.transitive(call_file, call_range)),
        );
        analysis.gaps.extend(nested.gaps);
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

fn injected_dependencies_for_function(
    db: &dyn Db,
    file: File,
    function: &StmtFunctionDef,
    target_exceptions: &Vec<Exception>,
    call_stack: CallStack,
) -> FunctionAnalysis {
    let mut dependencies = Vec::new();
    collect_parameter_dependencies(db, file, function, &mut dependencies);
    let mut analysis = FunctionAnalysis::default();
    for dependency in dependencies {
        let Some(callable) = dependency.callable else {
            analysis.gaps.push(AnalysisGap::new(
                AnalysisGapKind::UnsupportedCallback,
                AnalysisGapImpact::MayMissErrors,
                file,
                dependency.range,
                None,
            ));
            continue;
        };
        let nested = dependency_expression_analysis(
            db,
            file,
            callable,
            target_exceptions,
            call_stack.clone(),
        );
        analysis.errors.extend(nested.errors);
        analysis.gaps.extend(nested.gaps);
    }
    let aliases =
        aliased_parameter_dependency_analysis(db, file, function, target_exceptions, call_stack);
    analysis.errors.extend(aliases.errors);
    analysis.gaps.extend(aliases.gaps);
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

fn unsupported_dependency(file: File, range: TextRange) -> FunctionAnalysis {
    FunctionAnalysis {
        errors: vec![],
        gaps: vec![AnalysisGap::new(
            AnalysisGapKind::UnsupportedCallback,
            AnalysisGapImpact::MayMissErrors,
            file,
            range,
            None,
        )],
    }
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

fn is_route_decorator(expression: &Expr) -> bool {
    const ROUTE_METHODS: [&str; 9] = [
        "api_route",
        "delete",
        "get",
        "head",
        "options",
        "patch",
        "post",
        "put",
        "trace",
    ];

    expression
        .as_attribute_expr()
        .is_some_and(|attribute| ROUTE_METHODS.contains(&attribute.attr.as_str()))
}

fn response_model_names(db: &dyn Db, file: File, expression: &Expr) -> Vec<(TextRange, String)> {
    match expression {
        Expr::BinOp(binary) if binary.op == Operator::BitOr => {
            let mut names = response_model_names(db, file, &binary.left);
            names.extend(response_model_names(db, file, &binary.right));
            names
        }
        Expr::Subscript(subscript) if type_name(&subscript.value) == Some("Union") => {
            match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple
                    .elts
                    .iter()
                    .flat_map(|element| response_model_names(db, file, element))
                    .collect(),
                slice => response_model_names(db, file, slice),
            }
        }
        Expr::Subscript(subscript) if type_name(&subscript.value) == Some("Annotated") => {
            match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple
                    .elts
                    .first()
                    .map(|element| response_model_names(db, file, element))
                    .unwrap_or_default(),
                slice => response_model_names(db, file, slice),
            }
        }
        Expr::Subscript(subscript) => response_model_name(&subscript.value)
            .map(|(base_range, base)| {
                vec![
                    (base_range, base.clone()),
                    (
                        subscript.range,
                        format!(
                            "{base}[{}]",
                            canonical_exception_expression(db, file, &subscript.slice)
                        ),
                    ),
                ]
            })
            .unwrap_or_default(),
        _ => response_model_name(expression).into_iter().collect(),
    }
}

fn response_model_name(expression: &Expr) -> Option<(TextRange, String)> {
    match expression {
        Expr::Name(name) => Some((name.range, name.id.to_string())),
        Expr::Attribute(attribute) => Some((attribute.attr.range, attribute.attr.to_string())),
        _ => None,
    }
}

fn type_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}
