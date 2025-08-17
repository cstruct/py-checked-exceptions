use ruff_db::{
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_python_ast::{ExceptHandler, Expr, ExprTuple};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;
use ty_python_semantic::{
    ResolvedDefinition, definitions_for_attribute, definitions_for_name,
    semantic_index::definition::{Definition, DefinitionKind},
};

use crate::{
    module::ModuleCollector,
    transitive_error::{
        call_stack::CallStack, capture_stack::ExceptionCaptureStack, exception::Exception,
        raise::FunctionRaise, visitor::get_transitive_errors,
    },
};

#[allow(clippy::too_many_arguments)]
fn extract_errors_cycle_fn<'db>(
    _db: &'db dyn Db,
    _value: &[FunctionRaise],
    _count: u32,
    _expr_file: File,
    _expr_range: TextRange,
    _definition_file: File,
    _definition: Definition<'db>,
    _target_exceptions: Vec<Exception>,
    _call_stack: CallStack,
    _exception_capture_stack: ExceptionCaptureStack,
) -> salsa::CycleRecoveryAction<Vec<FunctionRaise>> {
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
) -> Vec<FunctionRaise> {
    vec![]
}

#[allow(clippy::too_many_arguments)]
#[salsa::tracked(returns(deref), cycle_fn=extract_errors_cycle_fn, cycle_initial=extract_errors_initial, heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn extract_errors<'db>(
    db: &'db dyn Db,
    expr_file: File,
    expr_range: TextRange,
    definition_file: File,
    definition: Definition<'db>,
    target_exceptions: Vec<Exception>,
    call_stack: CallStack,
    exception_capture_stack: ExceptionCaptureStack,
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
            &target_exceptions,
            new_stack,
            &exception_capture_stack,
        );
        let transitive_errors = transitive_errors
            .iter()
            .map(|e| e.transitive(expr_file, expr_range));
        errors.extend(transitive_errors);
    }
    errors
}

#[allow(clippy::too_many_arguments)]
#[salsa::tracked(returns(clone), no_eq, heap_size=ruff_memory_usage::heap_size)]
pub fn extract_exception<'db>(
    db: &'db dyn Db,
    definition_file: File,
    definition: Definition<'db>,
) -> Option<Exception> {
    let module = parsed_module(db, definition_file).load(db);
    let (definition_file, definition) = resolve_alias(db, &module, definition_file, definition)?;
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

pub(crate) fn try_extract_exception_from_expr(
    db: &dyn Db,
    file: File,
    expr: &Expr,
) -> Option<Exception> {
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
