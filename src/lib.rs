#![feature(extend_one)]
use anyhow::Result;
use anyhow::bail;
use crossbeam::channel::Sender;
use crossbeam::channel::bounded;
use indicatif::ProgressBar;
use rayon::prelude::*;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::system::SystemPathBuf;
use ty_project::{Db, ProjectDatabase};

use crate::module::ModuleCollector;
use crate::transitive_error::call_stack::CallStack;
use crate::transitive_error::capture_stack::ExceptionCaptureStack;
use crate::transitive_error::visitor::get_transitive_errors;

mod module;
mod transitive_error;

pub use transitive_error::exception::Exception;

pub fn analyze_project(
    project_path: SystemPathBuf,
    db: ProjectDatabase,
    filter_path: Option<SystemPathBuf>,
    target_exceptions: Vec<crate::Exception>,
    progress_bar: Option<&'static ProgressBar>,
) -> Result<impl Iterator<Item = Diagnostic>> {
    let (sender, receiver) = bounded(10);
    let files = db.project().files(&db).clone();
    if let Some(pb) = &progress_bar {
        pb.set_length(files.len() as u64);
    }

    if let Some(ref filter_path) = filter_path {
        let project_path = project_path.as_str();
        if !filter_path.starts_with(project_path) {
            bail!(
                "Filter path must be a subpath of the project path. filter_path: '{filter_path}', project_path: '{project_path}'"
            )
        }
    };
    rayon::spawn(move || {
        let filtered_files: Vec<File> = files
            .into_par_iter()
            .map_with(db.clone(), move |db, file| {
                if filter_path.as_ref().is_none_or(|filter_path| {
                    file.path(db)
                        .as_system_path()
                        .unwrap()
                        .starts_with(filter_path)
                }) {
                    return Some(file);
                }
                None
            })
            .filter_map(|x| x)
            .collect();

        if let Some(pb) = &progress_bar {
            pb.set_length(filtered_files.len() as u64);
        }

        filtered_files.into_par_iter().for_each_with(
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
        for error in errors {
            sender.send(error.into()).unwrap();
        }
    }
}
