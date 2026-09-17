use std::collections::HashSet;

use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span},
    files::{File, FileRange},
};
use ruff_python_ast::{Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_python_core::definition::docstring_from_body;

use crate::transitive_error::exception::canonical_exception_name;
use crate::transitive_error::raise::FunctionRaise;

pub fn compare_documented_exceptions(
    file: File,
    function: &StmtFunctionDef,
    errors: &[FunctionRaise],
    extension_documented_errors: Vec<(TextRange, String)>,
) -> Vec<Diagnostic> {
    let documented_docstring_errors = documented_docstring_exceptions(&function.body);
    let documented_errors = documented_docstring_errors
        .iter()
        .chain(&extension_documented_errors)
        .cloned()
        .collect_vec();

    let errors = errors.iter().collect_vec();

    let (undocumented_errors, _) = difference_by_key(
        errors
            .iter()
            .copied()
            .filter(|error| !error.documentation_optional()),
        documented_errors.into_iter(),
        |e| canonical_exception_name(&e.name().name),
        |(_, e)| canonical_exception_name(e),
    );
    // Extension-provided errors can be emitted by framework or middleware code that isn't visible
    // from the function body. Only explicitly documented docstring errors are therefore checked
    // for extras.
    let (_, extra_documented_errors) = difference_by_key(
        errors.into_iter(),
        documented_docstring_errors.into_iter(),
        |e| canonical_exception_name(&e.name().name),
        |(_, e)| canonical_exception_name(e),
    );

    let mut diagnostics: Vec<Diagnostic> =
        undocumented_errors.iter().map(|e| (*e).into()).collect();
    diagnostics.extend(extra_documented_errors.iter().map(|(range, e)| {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(LintName::of("extra-documented-error")),
            Severity::Error,
            format!("Documents extra error that is never raised {e}"),
        );
        diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
            file, *range,
        ))));
        diagnostic
    }));
    diagnostics
}

fn documented_docstring_exceptions(stmts: &[Stmt]) -> Vec<(TextRange, String)> {
    let Some(docstring) = docstring_from_body(stmts) else {
        return Vec::new();
    };
    let lines = docstring
        .value
        .to_str()
        .split("\n")
        .map(|l| format!("{l}\n"))
        .collect_vec();
    let Some((start_index, section_header)) = lines.iter().find_position(|l| l.contains("Raises:"))
    else {
        return Vec::new();
    };
    let docstring_start = stmts[0].range().start();

    let preceding_lines_offset = 4 + lines[1..=start_index]
        .iter()
        .fold(0, |acc, l| acc + l.len());
    let docstring_indent = count_whitespace_chars_at_start(section_header);
    let raises_list_line_start = docstring_start.to_usize() + preceding_lines_offset;

    let section_lines: Vec<_> = lines[start_index + 1..]
        .iter()
        .take_while(|l| l.starts_with(" ".repeat(docstring_indent + 4).as_str()))
        .collect();
    let (_, error_names) = section_lines
        .iter()
        .filter_map(|l| {
            let parts: Vec<_> = l.split(":").collect();
            if parts.len() == 2 {
                let exc_name = parts[0].trim();
                return Some((l.len(), exc_name));
            }
            None
        })
        .fold(
            (raises_list_line_start, HashSet::new()),
            |(offset, mut es), (line_length, e)| {
                let start = offset + docstring_indent + 4;
                let end = start + e.len();
                let range = TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32));
                es.insert((range, e.to_string()));
                (offset + docstring_indent + line_length, es)
            },
        );
    error_names.into_iter().collect()
}

fn count_whitespace_chars_at_start(input: &str) -> usize {
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
