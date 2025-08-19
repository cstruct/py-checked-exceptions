use std::collections::HashSet;

use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span},
    files::{File, FileRange},
};
use ruff_linter::docstrings::extraction::docstring_from;
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::transitive_error::raise::FunctionRaise;

pub fn compare_documented_exceptions(
    file: File,
    stmts: &[Stmt],
    errors: &[FunctionRaise],
) -> Vec<Diagnostic> {
    let Some(docstring) = docstring_from(stmts) else {
        return errors.iter().map(|e| e.into()).collect();
    };
    let lines = docstring
        .value
        .to_str()
        .split("\n")
        .map(|l| format!("{l}\n"))
        .collect_vec();
    let Some((start_index, section_header)) = lines.iter().find_position(|l| l.contains("Raises:"))
    else {
        return errors.iter().map(|e| (e.into())).collect();
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
                es.insert((range, e));
                (offset + docstring_indent + line_length, es)
            },
        );
    let errors: HashSet<_> = errors.iter().collect();

    let (undocumented_errors, extra_documented_errors) = difference_by_key(
        errors.into_iter(),
        error_names.into_iter(),
        |e| e.name().name.clone(),
        |(_, e)| e.to_string(),
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
