use itertools::Itertools;
use ruff_db::files::File;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
use ty_project::Db;
use ty_python_semantic::types::{call_signature_details, find_active_signature_from_details};
use ty_python_semantic::{
    ResolvedDefinition, SemanticModel, definitions_for_attribute, definitions_for_name,
};

use crate::transitive_error::{
    call_stack::CallStack, capture_stack::ExceptionCaptureStack, exception::Exception,
    extract::extract_errors, raise::FunctionRaise, visitor::normalize_errors,
};

pub(crate) type CallableErrors = Vec<(String, Vec<FunctionRaise>)>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn callable_errors_for_call(
    db: &dyn Db,
    file: File,
    call: &ExprCall,
    target_exceptions: &[Exception],
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    inherited_callable_errors: &CallableErrors,
) -> CallableErrors {
    if call.arguments.is_empty() {
        return vec![];
    }
    let arguments = call.arguments.arguments_source_order().collect_vec();
    if !arguments
        .iter()
        .any(|argument| matches!(argument.value(), Expr::Name(_) | Expr::Attribute(_)))
    {
        return vec![];
    }

    let model = SemanticModel::new(db, file);
    let signature_details = call_signature_details(db, &model, call);
    let Some(active_signature) = find_active_signature_from_details(&signature_details) else {
        return vec![];
    };
    let Some(details) = signature_details.get(active_signature) else {
        return vec![];
    };
    let mut callable_errors: CallableErrors = vec![];

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
        let errors = callable_expression_errors(
            db,
            file,
            argument,
            target_exceptions,
            call_stack.clone(),
            exception_capture_stack,
            inherited_callable_errors,
        );
        if errors.is_empty() {
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
                existing_errors.extend(errors.clone());
                *existing_errors = normalize_errors(existing_errors.clone());
            } else {
                callable_errors.push((parameter_name.clone(), errors.clone()));
            }
        }
    }
    callable_errors
}

#[allow(clippy::too_many_arguments)]
fn callable_expression_errors(
    db: &dyn Db,
    file: File,
    expression: &Expr,
    target_exceptions: &[Exception],
    call_stack: CallStack,
    exception_capture_stack: &ExceptionCaptureStack,
    inherited_callable_errors: &CallableErrors,
) -> Vec<FunctionRaise> {
    if let Some(name) = expression.as_name_expr()
        && let Some((_, errors)) = inherited_callable_errors
            .iter()
            .find(|(callable, _)| callable == name.id.as_str())
    {
        return errors.clone();
    }

    let definitions = match expression {
        Expr::Name(name) => definitions_for_name(db, file, name),
        Expr::Attribute(attribute) => definitions_for_attribute(db, file, attribute),
        _ => return vec![],
    };
    let mut errors = vec![];
    for definition in definitions {
        let ResolvedDefinition::Definition(definition) = definition else {
            continue;
        };
        let definition_file = definition.file(db);
        errors.extend(
            extract_errors(
                db,
                file,
                expression.range(),
                definition_file,
                definition,
                target_exceptions.to_vec(),
                call_stack.clone(),
                exception_capture_stack.clone(),
                vec![],
            )
            .iter()
            .cloned(),
        );
    }
    normalize_errors(errors)
}
