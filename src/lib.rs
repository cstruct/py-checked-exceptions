#![feature(extend_one)]
use anyhow::Result;
use crossbeam::channel::Sender;
use crossbeam::channel::bounded;
use indicatif::ProgressBar;
use itertools::Itertools;
use rayon::prelude::*;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ty_project::{Db, ProjectDatabase};
use ty_python_semantic::ModuleName;
use ty_python_semantic::resolve_module;
use ty_python_semantic::semantic_index::global_scope;
use ty_python_semantic::types::resolve_definition::find_symbol_in_scope;

use crate::docstring::compare_documented_exceptions;
use crate::module::ModuleCollector;
use crate::transitive_error::call_stack::CallStack;
use crate::transitive_error::capture_stack::ExceptionCaptureStack;
use crate::transitive_error::visitor::get_transitive_errors;

mod docstring;
mod module;
mod transitive_error;

pub use transitive_error::exception::Exception;
pub use transitive_error::extract::extract_exception;

pub fn analyze_project(
    db: ProjectDatabase,
    target_exceptions: Vec<crate::Exception>,
    progress_bar: Option<&'static ProgressBar>,
) -> Result<impl Iterator<Item = Diagnostic>> {
    let (sender, receiver) = bounded(10);
    let files = db.project().files(&db).clone();
    if let Some(pb) = &progress_bar {
        pb.set_length(files.len() as u64);
    }

    rayon::spawn(move || {
        if let Some(pb) = &progress_bar {
            pb.set_length(files.len() as u64);
        }

        files.into_par_iter().for_each_with(
            (db, target_exceptions),
            |(db, target_exceptions), file| {
                let db2 = db.clone();
                analyze_file(db, &sender, file, target_exceptions);
                if let Some(pb) = &progress_bar {
                    pb.set_message(file.path(&db2).as_str().to_string());
                    pb.inc(1);
                    pb.force_draw();
                }
            },
        );
        drop(sender);
    });

    Ok(receiver.into_iter())
}

pub fn analyze_file(
    db: &mut ProjectDatabase,
    sender: &Sender<Diagnostic>,
    file: File,
    target_exceptions: &Vec<crate::Exception>,
) {
    let module = parsed_module(db, file);
    let module_ref = module.load(db);
    module_ref.clone().errors().iter().for_each(|error| {
        sender
            .send(Diagnostic::invalid_syntax(file, &error.error, error))
            .unwrap()
    });

    let mut module_collector = ModuleCollector::new();
    module_collector.init(&module_ref);

    for func_def in module_collector.list_functions() {
        let errors = get_transitive_errors(
            db,
            file,
            func_def,
            target_exceptions,
            CallStack::new(),
            &ExceptionCaptureStack::new(),
        );
        let diagnostics = compare_documented_exceptions(db, file, &func_def.body, &errors);
        for diagnostic in diagnostics {
            sender.send(diagnostic).unwrap();
        }
    }
}

pub fn resolve_absolute_module_path(db: &dyn Db, path: &str) -> Exception {
    let parts = path.split(".").collect_vec();
    let exception_name = parts
        .last()
        .expect("target exception has to be in 'full.module.path.Exception' format.");
    let module_components = parts[..parts.len() - 1].to_vec();
    let module_name = ModuleName::from_components(module_components)
        .expect("target exception has to be a valid module path.");
    let module = resolve_module(db, &module_name)
        .expect("target exception has to resolve to an existing module.");
    let module_file = module.file(db).unwrap();
    let global_scope = global_scope(db, module_file);
    let definitions_in_module = find_symbol_in_scope(db, global_scope, exception_name);
    for def in definitions_in_module {
        let file = def.file(db);
        let Some(exc) = extract_exception(db, file, def) else {
            continue;
        };
        return exc;
    }
    panic!("Failed to resolve exception from module path");
}
