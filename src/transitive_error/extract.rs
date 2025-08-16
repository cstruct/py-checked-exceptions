use ruff_db::{
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_python_ast::{ExceptHandler, Expr, ExprName, ExprTuple};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;
use ty_python_semantic::{
    ResolvedDefinition, definitions_for_name,
    semantic_index::definition::{Definition, DefinitionKind},
};

use crate::{
    module::ModuleCollector,
    transitive_error::{
        call_stack::CallStack, capture_stack::ExceptionCaptureStack, raise::FunctionRaise,
        visitor::get_transitive_errors,
    },
};

#[allow(clippy::too_many_arguments)]
#[salsa::tracked(returns(clone), no_eq, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn extract_errors<'db>(
    db: &'db dyn Db,
    expr_file: File,
    expr_range: TextRange,
    definition_file: File,
    definition: Definition<'db>,
    target_exceptions: &'db Vec<String>,
    call_stack: CallStack,
    exception_capture_stack: &'db ExceptionCaptureStack,
) -> Vec<FunctionRaise> {
    let module = parsed_module(db, definition_file).load(db);
    let Some((definition_file, definition)) =
        resolve_alias(db, &module, definition_file, definition)
    else {
        return vec![];
    };
    let module = parsed_module(db, definition_file).load(db);
    let mut module_collector = ModuleCollector::new();
    module_collector.init(&module);
    let full_range = definition.full_range(db, &module);

    let mut errors = vec![];

    for func_def in module_collector.find_functions(&full_range.range()) {
        let new_stack = call_stack.push((
            definition_file.path(db).as_str().into(),
            func_def.name.as_str().into(),
        ));
        let transitive_errors = get_transitive_errors(
            db,
            definition_file,
            func_def,
            target_exceptions,
            new_stack,
            exception_capture_stack,
        );
        let transitive_errors = transitive_errors
            .iter()
            .map(|e| e.transitive(expr_file, expr_range));
        errors.extend(transitive_errors);
    }
    errors
}

pub(crate) fn extract_caught_exceptions(handler: &ExceptHandler) -> Vec<String> {
    let Some(handler) = handler.as_except_handler() else {
        return vec![];
    };
    let Some(ref type_) = handler.type_ else {
        return vec!["*ALL*".to_string()];
    };
    if let Expr::Name(ExprName { id, .. }) = &**type_ {
        return vec![id.to_string()];
    } else if let Expr::Tuple(ExprTuple { elts, .. }) = &**type_ {
        return elts
            .iter()
            .flat_map(|e| e.as_name_expr())
            .map(|n| n.id.to_string())
            .collect();
    }
    vec![]
}

fn resolve_alias<'a>(
    db: &'a dyn Db,
    module: &ParsedModuleRef,
    def_file: File,
    def: Definition<'a>,
) -> Option<(File, Definition<'a>)> {
    let mut file = def_file;
    let mut def = def;
    while let DefinitionKind::Assignment(ass) = def.kind(db) {
        let value = ass.value(module).as_name_expr()?;
        for resolved in definitions_for_name(db, def_file, value) {
            if let ResolvedDefinition::Definition(inner_def) = resolved {
                file = inner_def.file(db);
                def = inner_def;
                break;
            }
        }
    }
    Some((file, def))
}
