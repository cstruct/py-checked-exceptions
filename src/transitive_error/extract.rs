use ruff_db::{
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;
use ty_python_semantic::{
    ResolvedDefinition, definitions_for_name,
    semantic_index::definition::{Definition, DefinitionKind},
};

use crate::{
    module::ModuleCollector,
    transitive_error::{raise::FunctionRaise, stack::CallStack, visitor::get_transitive_errors},
};

pub(crate) fn extract_errors<'db>(
    db: &'db dyn Db,
    expr_file: File,
    expr_range: TextRange,
    definition_file: File,
    definition: Definition<'db>,
    target_exceptions: &'db Vec<String>,
    stack: CallStack,
) -> Vec<FunctionRaise> {
    let module = parsed_module(db, definition_file).load(db);
    let (definition_file, definition) = resolve_alias(db, &module, definition_file, definition);
    let module = parsed_module(db, definition_file).load(db);
    let mut module_collector = ModuleCollector::new();
    module_collector.init(&module);
    let full_range = definition.full_range(db, &module);

    let mut errors = vec![];

    for func_def in module_collector.find_functions(&full_range.range()) {
        let new_stack = stack.push((
            definition_file.path(db).as_str().into(),
            func_def.name.as_str().into(),
        ));
        let transitive_errors =
            get_transitive_errors(db, definition_file, func_def, target_exceptions, new_stack);
        let transitive_errors = transitive_errors
            .iter()
            .map(|e| e.transitive(expr_file, expr_range));
        errors.extend(transitive_errors);
    }
    errors
}

fn resolve_alias<'a>(
    db: &'a dyn Db,
    module: &ParsedModuleRef,
    def_file: File,
    def: Definition<'a>,
) -> (File, Definition<'a>) {
    let mut file = def_file;
    let mut def = def;
    while let DefinitionKind::Assignment(ass) = def.kind(db) {
        let value = ass.value(module).as_name_expr().unwrap();
        for resolved in definitions_for_name(db, def_file, value) {
            if let ResolvedDefinition::Definition(inner_def) = resolved {
                file = inner_def.file(db);
                def = inner_def;
                break;
            }
        }
    }
    (file, def)
}
