use ruff_db::{files::File, parsed::parsed_module};
use ruff_python_ast::{ExceptHandler, Expr, ExprTuple};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;
use ty_python_semantic::{
    ResolvedDefinition, definitions_for_attribute, definitions_for_name,
    semantic_index::definition::{Definition, DefinitionKind},
};

use crate::{
    AnalysisOptions,
    module::ModuleCollector,
    transitive_error::{
        analysis::{AnalysisGap, AnalysisGapImpact, AnalysisGapKind, FunctionAnalysis},
        call_stack::CallStack,
        capture_stack::ExceptionCaptureStack,
        exception::{Exception, canonical_exception_expression},
        higher_order::CallableErrors,
        visitor::get_transitive_analysis_with_callable_errors,
    },
};

#[allow(clippy::too_many_arguments)]
fn extract_errors_cycle_fn<'db>(
    _db: &'db dyn Db,
    _value: &FunctionAnalysis,
    _count: u32,
    _expr_file: File,
    _expr_range: TextRange,
    _definition_file: File,
    _definition: Definition<'db>,
    _target_exceptions: Vec<Exception>,
    _call_stack: CallStack,
    _exception_capture_stack: ExceptionCaptureStack,
    _analysis_options: AnalysisOptions,
    _callable_errors: CallableErrors,
) -> salsa::CycleRecoveryAction<FunctionAnalysis> {
    salsa::CycleRecoveryAction::Iterate
}

#[allow(clippy::too_many_arguments)]
fn extract_errors_initial<'db>(
    _db: &'db dyn Db,
    _expr_file: File,
    _expr_range: TextRange,
    _definition_file: File,
    _definition: Definition<'db>,
    _target_exceptions: Vec<Exception>,
    _call_stack: CallStack,
    _exception_capture_stack: ExceptionCaptureStack,
    _analysis_options: AnalysisOptions,
    _callable_errors: CallableErrors,
) -> FunctionAnalysis {
    FunctionAnalysis::default()
}

#[allow(clippy::too_many_arguments)]
#[salsa::tracked(returns(clone), cycle_fn=extract_errors_cycle_fn, cycle_initial=extract_errors_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn extract_analysis<'db>(
    db: &'db dyn Db,
    expr_file: File,
    expr_range: TextRange,
    definition_file: File,
    definition: Definition<'db>,
    target_exceptions: Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: ExceptionCaptureStack,
    analysis_options: AnalysisOptions,
    callable_errors: CallableErrors,
) -> FunctionAnalysis {
    let Some((definition_file, definition)) = resolve_alias(db, definition_file, definition) else {
        return FunctionAnalysis {
            errors: vec![],
            gaps: vec![AnalysisGap::new(
                AnalysisGapKind::DynamicCall,
                AnalysisGapImpact::MayMissErrors,
                expr_file,
                expr_range,
                definition.name(db).map(|name| name.to_string()),
            )],
        };
    };
    if matches!(
        definition_file.path(db),
        ruff_db::files::FilePath::System(path) if path.extension() == Some("pyi")
    ) {
        return FunctionAnalysis {
            errors: vec![],
            gaps: vec![AnalysisGap::new(
                AnalysisGapKind::OpaqueCall,
                AnalysisGapImpact::MayMissErrors,
                expr_file,
                expr_range,
                definition.name(db).map(|name| name.to_string()),
            )],
        };
    }
    let module = parsed_module(db, definition_file).load(db);
    let mut module_collector = ModuleCollector::new();
    module_collector.init(&module);
    let full_range = definition.full_range(db, &module);

    let mut analysis = FunctionAnalysis::default();
    let mut found_function = false;

    for func_def in module_collector.find_functions(&full_range.range()) {
        found_function = true;
        let new_stack = call_stack.push((
            definition_file.path(db).as_str().into(),
            func_def.name.as_str().into(),
        ));
        let transitive = get_transitive_analysis_with_callable_errors(
            db,
            definition_file,
            func_def,
            &target_exceptions,
            new_stack,
            &exception_capture_stack,
            &analysis_options,
            callable_errors.clone(),
        );
        let transitive_errors = transitive
            .errors
            .iter()
            .map(|e| e.transitive(expr_file, expr_range));
        analysis.errors.extend(transitive_errors);
        analysis.gaps.extend(transitive.gaps);
    }
    if !found_function
        && matches!(
            definition_file.path(db),
            ruff_db::files::FilePath::SystemVirtual(_)
        )
    {
        analysis.gaps.push(AnalysisGap::new(
            AnalysisGapKind::OpaqueCall,
            AnalysisGapImpact::MayMissErrors,
            expr_file,
            expr_range,
            definition.name(db).map(|name| name.to_string()),
        ));
    }
    analysis
}

#[allow(clippy::too_many_arguments)]
#[salsa::tracked(returns(clone), no_eq, heap_size=ruff_memory_usage::heap_size)]
pub fn extract_exception<'db>(
    db: &'db dyn Db,
    definition_file: File,
    definition: Definition<'db>,
) -> Option<Exception> {
    let (definition_file, definition) = resolve_alias(db, definition_file, definition)?;
    let module = parsed_module(db, definition_file).load(db);
    let mut module_collector = ModuleCollector::new();
    module_collector.init(&module);
    let full_range = definition.full_range(db, &module);

    let cls = module_collector.find_class(&full_range.range())?;
    let bases = cls.bases();

    let bases = bases
        .iter()
        .filter_map(|b| b.as_name_expr())
        .flat_map(|b| {
            let defs = definitions_for_name(db, definition_file, b);
            defs.iter()
                .filter_map(|def| {
                    if let ResolvedDefinition::Definition(def) = def {
                        let inner_definition_file = def.file(db);
                        extract_exception(db, inner_definition_file, *def)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    Some(Exception::new(cls.name.to_string(), bases))
}

pub(crate) fn extract_caught_exceptions(
    db: &dyn Db,
    file: File,
    handler: &ExceptHandler,
) -> Vec<Exception> {
    let Some(handler) = handler.as_except_handler() else {
        return vec![];
    };
    let Some(ref type_) = handler.type_ else {
        return vec![Exception::base_exception()];
    };
    if let Expr::Name(_) = &**type_ {
        let Some(exception) = try_extract_exception_from_expr(db, file, type_) else {
            return vec![];
        };
        return vec![exception];
    } else if let Expr::Tuple(ExprTuple { elts, .. }) = &**type_ {
        return elts
            .iter()
            .filter(|e| e.is_name_expr())
            .filter_map(|e| try_extract_exception_from_expr(db, file, e))
            .collect();
    }
    vec![]
}

pub(crate) fn resolve_alias<'a>(
    db: &'a dyn Db,
    def_file: File,
    def: Definition<'a>,
) -> Option<(File, Definition<'a>)> {
    let mut file = def_file;
    let mut def = def;
    let mut seen = std::collections::HashSet::new();
    while let DefinitionKind::Assignment(assignment) = def.kind(db) {
        if !seen.insert(def) {
            return None;
        }
        let module = parsed_module(db, file).load(db);
        let value = assignment.value(&module).as_name_expr()?;
        let inner_def = definitions_for_name(db, file, value)
            .into_iter()
            .find_map(|resolved| match resolved {
                ResolvedDefinition::Definition(definition) => Some(definition),
                _ => None,
            })?;
        file = inner_def.file(db);
        def = inner_def;
    }
    Some((file, def))
}

pub(crate) fn try_extract_exception_from_expr(
    db: &dyn Db,
    file: File,
    expr: &Expr,
) -> Option<Exception> {
    if let Expr::Subscript(subscript) = expr {
        let exception = try_extract_exception_from_expr(db, file, &subscript.value)?;
        let arguments = canonical_exception_expression(db, file, &subscript.slice);
        return Some(exception.with_type_arguments(arguments));
    }

    let defs = match *expr {
        Expr::Name(ref name) => definitions_for_name(db, file, name),
        Expr::Attribute(ref attr) => definitions_for_attribute(db, file, attr),
        _ => return None,
    };

    for def in defs {
        if let ResolvedDefinition::Definition(def) = def {
            let definition_file = def.file(db);

            let Some(exception) = extract_exception(db, definition_file, def) else {
                continue;
            };
            return Some(exception);
        }
    }
    None
}
