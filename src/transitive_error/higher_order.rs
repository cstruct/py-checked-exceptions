use itertools::Itertools;
use ruff_db::files::{File, FilePath};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use ty_project::Db;
use ty_python_semantic::types::{call_signature_details, find_active_signature_from_details};
use ty_python_semantic::{
    ResolvedDefinition, SemanticModel, definitions_for_attribute, definitions_for_name,
};

use crate::{
    AnalysisOptions,
    transitive_error::{
        analysis::{AnalysisGap, AnalysisGapImpact, AnalysisGapKind, FunctionAnalysis},
        call_stack::CallStack,
        capture_stack::ExceptionCaptureStack,
        exception::Exception,
        extract::extract_analysis,
        raise::FunctionRaise,
        visitor::normalize_errors,
    },
};

pub(crate) type CallableErrors = Vec<(String, Vec<FunctionRaise>)>;

#[derive(Default)]
pub(crate) struct CallableAnalysis {
    pub(crate) errors: CallableErrors,
    pub(crate) gaps: Vec<AnalysisGap>,
}

pub(crate) fn eager_stdlib_callback_errors(
    db: &dyn Db,
    file: File,
    call: &ExprCall,
    callable_errors: &CallableErrors,
) -> Vec<FunctionRaise> {
    let callback_parameters = definitions_for_expression(db, file, &call.func)
        .into_iter()
        .filter_map(|definition| {
            let ResolvedDefinition::Definition(definition) = definition else {
                return None;
            };
            eager_stdlib_callback_parameter(db, definition)
        })
        .collect_vec();

    normalize_errors(
        callable_errors
            .iter()
            .filter(|(parameter, _)| callback_parameters.contains(parameter))
            .flat_map(|(_, errors)| {
                errors
                    .iter()
                    .map(|error| error.transitive(file, call.range))
            })
            .collect(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn callable_analysis_for_call(
    db: &dyn Db,
    file: File,
    call: &ExprCall,
    target_exceptions: &[Exception],
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    inherited_callable_errors: &CallableErrors,
    analysis_options: &AnalysisOptions,
) -> CallableAnalysis {
    if call.arguments.is_empty() {
        return CallableAnalysis::default();
    }
    let arguments = call.arguments.arguments_source_order().collect_vec();
    if !arguments.iter().any(|argument| {
        matches!(
            argument.value(),
            Expr::Name(_) | Expr::Attribute(_) | Expr::Lambda(_)
        )
    }) {
        return CallableAnalysis::default();
    }

    let model = SemanticModel::new(db, file);
    let signature_details = call_signature_details(db, &model, call);
    let Some(active_signature) = find_active_signature_from_details(&signature_details) else {
        return CallableAnalysis::default();
    };
    let Some(details) = signature_details.get(active_signature) else {
        return CallableAnalysis::default();
    };
    let mut callable_errors: CallableErrors = vec![];
    let mut gaps = vec![];

    for (argument_index, mapping) in details.argument_to_parameter_mapping.iter().enumerate() {
        if !mapping.matched {
            continue;
        }
        let Some(argument) = arguments
            .get(argument_index)
            .map(|argument| argument.value())
        else {
            continue;
        };
        let analysis = callable_expression_analysis(
            db,
            file,
            argument,
            target_exceptions,
            call_stack.clone(),
            exception_capture_stack,
            inherited_callable_errors,
            analysis_options,
        );
        gaps.extend(analysis.gaps);
        if analysis.errors.is_empty() {
            continue;
        }
        for parameter_index in &mapping.parameters {
            let Some(parameter_name) = details.parameter_names.get(*parameter_index) else {
                continue;
            };
            if parameter_name.is_empty() {
                continue;
            }
            if let Some((_, existing_errors)) = callable_errors
                .iter_mut()
                .find(|(name, _)| name == parameter_name)
            {
                existing_errors.extend(analysis.errors.clone());
                *existing_errors = normalize_errors(existing_errors.clone());
            } else {
                callable_errors.push((parameter_name.clone(), analysis.errors.clone()));
            }
        }
    }
    CallableAnalysis {
        errors: callable_errors,
        gaps,
    }
}

#[allow(clippy::too_many_arguments)]
fn callable_expression_analysis(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    target_exceptions: &[Exception],
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    inherited_callable_errors: &CallableErrors,
    analysis_options: &AnalysisOptions,
) -> FunctionAnalysis {
    if let Some(name) = expression.as_name_expr()
        && let Some((_, errors)) = inherited_callable_errors
            .iter()
            .find(|(callable, _)| callable == name.id.as_str())
    {
        return FunctionAnalysis {
            errors: errors.clone(),
            gaps: vec![],
        };
    }

    let definitions = match expression {
        Expr::Name(name) => definitions_for_name(db, file, name),
        Expr::Attribute(attribute) => definitions_for_attribute(db, file, attribute),
        Expr::Lambda(_) => {
            return FunctionAnalysis {
                errors: vec![],
                gaps: vec![AnalysisGap::new(
                    AnalysisGapKind::UnsupportedCallback,
                    AnalysisGapImpact::MayMissErrors,
                    file,
                    expression.range(),
                    None,
                )],
            };
        }
        _ => return FunctionAnalysis::default(),
    };
    let mut analysis = FunctionAnalysis::default();
    for definition in definitions {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        let definition_file = definition.file(db);
        let callback_analysis = extract_analysis(
            db,
            file,
            expression.range(),
            definition_file,
            definition,
            target_exceptions.to_vec(),
            call_stack.clone(),
            exception_capture_stack.clone(),
            analysis_options.clone(),
            vec![],
        );
        analysis.errors.extend(callback_analysis.errors);
        analysis.gaps.extend(callback_analysis.gaps);
    }
    analysis.errors = normalize_errors(analysis.errors);
    analysis
}

fn eager_stdlib_callback_parameter(
    db: &dyn Db,
    definition: ty_python_semantic::semantic_index::definition::Definition<'_>,
) -> Option<String> {
    let name = definition.name(db)?;
    let FilePath::Vendored(path) = definition.file(db).path(db) else {
        return None;
    };

    match (path.as_str(), name.as_str()) {
        (path, "sorted" | "min" | "max" | "sort") if path.ends_with("/builtins.pyi") => {
            Some("key".into())
        }
        (path, "reduce") if path.ends_with("/functools.pyi") => Some("function".into()),
        (path, "sub" | "subn")
            if path.ends_with("/re.pyi") || path.ends_with("/re/__init__.pyi") =>
        {
            Some("repl".into())
        }
        _ => None,
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
