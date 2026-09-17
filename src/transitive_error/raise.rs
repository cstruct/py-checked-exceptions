use crate::transitive_error::exception::Exception;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
use ruff_db::files::{File, FileRange};
use ruff_text_size::TextRange;

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct FunctionRaiseDirectTarget {
    file: File,
    exception: Exception,
    range: TextRange,
    documentation_optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct FunctionRaiseTransitiveTarget {
    target: Box<FunctionRaise>,
    file: File,
    exception: Exception,
    range: TextRange,
    depth: usize,
    documentation_optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum FunctionRaise {
    Direct(FunctionRaiseDirectTarget),
    Transitive(FunctionRaiseTransitiveTarget),
}

impl FunctionRaise {
    pub(crate) fn direct(file: File, exception: Exception, range: TextRange) -> Self {
        FunctionRaise::Direct(FunctionRaiseDirectTarget {
            file,
            exception,
            range,
            documentation_optional: false,
        })
    }
    pub(crate) fn sort_key(&self) -> (String, bool, usize, usize) {
        match self {
            FunctionRaise::Direct(e) => (e.exception.name.clone(), e.documentation_optional, 0, 0),
            FunctionRaise::Transitive(e) => (
                e.exception.name.clone(),
                e.documentation_optional,
                1,
                e.depth,
            ),
        }
    }
    pub(crate) fn group_key(&self) -> String {
        match self {
            FunctionRaise::Direct(e) => e.exception.name.clone(),
            FunctionRaise::Transitive(e) => e.exception.name.clone(),
        }
    }
    pub(crate) fn transitive(&self, file: File, range: TextRange) -> Self {
        match self {
            FunctionRaise::Direct(FunctionRaiseDirectTarget {
                exception,
                documentation_optional,
                ..
            }) => FunctionRaise::Transitive(FunctionRaiseTransitiveTarget {
                target: Box::new(self.clone()),
                file,
                exception: (*exception).clone(),
                range,
                depth: 1,
                documentation_optional: *documentation_optional,
            }),
            FunctionRaise::Transitive(FunctionRaiseTransitiveTarget {
                exception,
                depth,
                documentation_optional,
                ..
            }) => FunctionRaise::Transitive(FunctionRaiseTransitiveTarget {
                target: Box::new(self.clone()),
                file,
                exception: (*exception).clone(),
                range,
                depth: depth + 1,
                documentation_optional: *documentation_optional,
            }),
        }
    }

    pub(crate) fn documentation_optional(&self) -> bool {
        match self {
            FunctionRaise::Direct(error) => error.documentation_optional,
            FunctionRaise::Transitive(error) => error.documentation_optional,
        }
    }

    pub(crate) fn with_optional_documentation(mut self) -> Self {
        match &mut self {
            FunctionRaise::Direct(error) => error.documentation_optional = true,
            FunctionRaise::Transitive(error) => error.documentation_optional = true,
        }
        self
    }
    pub(crate) fn name(&self) -> &Exception {
        match self {
            FunctionRaise::Direct(r) => &r.exception,
            FunctionRaise::Transitive(r) => &r.exception,
        }
    }
}

impl From<&FunctionRaise> for Diagnostic {
    fn from(val: &FunctionRaise) -> Self {
        match val {
            FunctionRaise::Direct(direct) => {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("raise")),
                    Severity::Error,
                    format!("Raises undocumented error {}", direct.exception.name,),
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
                    format!("Raises undocumented error {}", transitive.exception.name),
                );
                diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
                    transitive.file,
                    transitive.range,
                ))));
                build_call_chain(&mut diagnostic, val);
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
