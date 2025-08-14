use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FileRange};
use ruff_text_size::TextRange;

#[derive(Clone, Debug, PartialEq, get_size2::GetSize)]
pub(crate) struct FunctionRaiseDirectTarget {
    file: File,
    name: String,
    range: TextRange,
}

#[derive(Clone, Debug, PartialEq, get_size2::GetSize)]
pub(crate) struct FunctionRaiseTransitiveTarget {
    target: Box<FunctionRaise>,
    file: File,
    name: String,
    range: TextRange,
    depth: usize,
}

#[derive(Clone, Debug, PartialEq, get_size2::GetSize)]
pub(crate) enum FunctionRaise {
    Direct(FunctionRaiseDirectTarget),
    Transitive(FunctionRaiseTransitiveTarget),
}

impl FunctionRaise {
    pub(crate) fn direct(file: File, name: String, range: TextRange) -> Self {
        FunctionRaise::Direct(FunctionRaiseDirectTarget { file, name, range })
    }
    pub(crate) fn sort_key(&self) -> (String, usize, usize) {
        match self {
            FunctionRaise::Direct(e) => (e.name.clone(), 0, 0),
            FunctionRaise::Transitive(e) => (e.name.clone(), 1, e.depth),
        }
    }
    pub(crate) fn group_key(&self) -> String {
        match self {
            FunctionRaise::Direct(e) => e.name.clone(),
            FunctionRaise::Transitive(e) => e.name.clone(),
        }
    }
    pub(crate) fn transitive(&self, file: File, range: TextRange) -> Self {
        match self {
            FunctionRaise::Direct(FunctionRaiseDirectTarget { name, .. }) => {
                FunctionRaise::Transitive(FunctionRaiseTransitiveTarget {
                    target: Box::new(self.clone()),
                    file,
                    name: (*name).clone(),
                    range,
                    depth: 1,
                })
            }
            FunctionRaise::Transitive(FunctionRaiseTransitiveTarget { name, depth, .. }) => {
                FunctionRaise::Transitive(FunctionRaiseTransitiveTarget {
                    target: Box::new(self.clone()),
                    file,
                    name: (*name).clone(),
                    range,
                    depth: depth + 1,
                })
            }
        }
    }
}

impl From<FunctionRaise> for Diagnostic {
    fn from(val: FunctionRaise) -> Self {
        match &val {
            FunctionRaise::Direct(direct) => {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("raise")),
                    Severity::Error,
                    format!("Raises {}", direct.name,),
                );
                diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
                    direct.file,
                    direct.range,
                ))));
                diagnostic
            }
            FunctionRaise::Transitive(transitive) => {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("raise")),
                    Severity::Error,
                    format!("Raises {}", transitive.name),
                );
                diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
                    transitive.file,
                    transitive.range,
                ))));
                build_call_chain(&mut diagnostic, &val);
                diagnostic
            }
        }
    }
}

fn build_call_chain(diagnostic: &mut Diagnostic, e: &FunctionRaise) {
    match e {
        FunctionRaise::Direct(t) => {
            diagnostic.annotate(Annotation::secondary(Span::from(FileRange::new(
                t.file, t.range,
            ))));
        }
        FunctionRaise::Transitive(t) => {
            diagnostic.annotate(Annotation::secondary(Span::from(FileRange::new(
                t.file, t.range,
            ))));
            build_call_chain(diagnostic, &t.target);
        }
    }
}
