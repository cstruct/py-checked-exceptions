use std::collections::HashSet;

use itertools::Itertools;
use ruff_db::{
    Db,
    diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span},
    files::{File, FileRange},
    source::source_text,
};
use ruff_linter::docstrings::extraction::docstring_from;
use ruff_python_ast::Stmt;
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::transitive_error::raise::FunctionRaise;

pub fn compare_documented_exceptions(
    db: &dyn Db,
    file: File,
    stmts: &[Stmt],
    errors: &[FunctionRaise],
) -> Vec<Diagnostic> {
    let source = source_text(db, file);
    let index = LineIndex::from_source_text(&source);
    let Some(docstring) = docstring_from(stmts) else {
        return errors.iter().map(|e| e.into()).collect();
    };
    let lines = docstring.value.to_str().split("\n").collect_vec();
    let Some((start_index, section_header)) = lines.iter().find_position(|l| l.contains("Raises:"))
    else {
        return errors.iter().map(|e| (e.into())).collect();
    };
    let indent = count_whitespace_bytes_at_start(section_header) + 4;
    let section_lines: Vec<_> = lines[start_index + 1..]
        .iter()
        .take_while(|l| l.starts_with(" ".repeat(indent).as_str()))
        .collect();
    let error_names: HashSet<_> = section_lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let parts: Vec<_> = l.split(":").collect();
            if parts.len() == 2 {
                let exc_name = parts[0].trim();
                return Some((start_index + i + 1, exc_name));
            }
            None
        })
        .collect();
    let errors: HashSet<_> = errors.iter().collect();

    let start = stmts[0].range().start();
    let lc = index.line_column(start, &source);
    let doc_start_line = lc.line.get() + 1;
    let doc_start_col = lc.column.get() + indent;

    let (undocumented_errors, extra_documented_errors) = difference_by_key(
        errors.into_iter(),
        error_names.into_iter(),
        |e| e.name().name.clone(),
        |(_, e)| e.to_string(),
    );

    let mut diagnostics: Vec<Diagnostic> =
        undocumented_errors.iter().map(|e| (*e).into()).collect();
    diagnostics.extend(extra_documented_errors.iter().map(|(i, e)| {
        let start_line = doc_start_line + i;
        let start_col = doc_start_col;
        let end_line = start_line;
        let end_col = start_col + e.len();
        let start = TextSize::new(
            index
                .line_start(OneIndexed::new(start_line).unwrap(), &source)
                .to_u32()
                + (start_col as u32),
        );
        let end = TextSize::new(
            index
                .line_start(OneIndexed::new(end_line).unwrap(), &source)
                .to_u32()
                + (end_col as u32),
        );
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(LintName::of("extra-documented-error")),
            Severity::Error,
            format!("Documents extra error that is never raised {e}"),
        );
        diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
            file,
            TextRange::new(start, end),
        ))));
        diagnostic
    }));
    diagnostics
}

fn count_whitespace_bytes_at_start(input: &str) -> usize {
    input
        .chars()
        .take_while(|ch| ch.is_whitespace() && *ch != '\n')
        .count()
}

fn difference_by_key<A, B, K, Fa, Fb>(
    iter_a: impl Iterator<Item = A>,
    iter_b: impl Iterator<Item = B>,
    key_fn_a: Fa,
    key_fn_b: Fb,
) -> (Vec<A>, Vec<B>)
where
    K: Eq + std::hash::Hash,
    Fa: Fn(&A) -> K + Copy,
    Fb: Fn(&B) -> K + Copy,
{
    let vec_a: Vec<A> = iter_a.collect();
    let vec_b: Vec<B> = iter_b.collect();

    let keys_a: HashSet<K> = vec_a.iter().map(key_fn_a).collect();
    let keys_b: HashSet<K> = vec_b.iter().map(key_fn_b).collect();

    let only_in_a = vec_a
        .into_iter()
        .filter(|item| !keys_b.contains(&key_fn_a(item)))
        .collect();

    let only_in_b = vec_b
        .into_iter()
        .filter(|item| !keys_a.contains(&key_fn_b(item)))
        .collect();

    (only_in_a, only_in_b)
}
