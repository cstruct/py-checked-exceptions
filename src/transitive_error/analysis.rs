use ruff_db::{
    diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span},
    files::{File, FileRange},
};
use ruff_text_size::TextRange;

use crate::transitive_error::raise::FunctionRaise;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum AnalysisGapKind {
    OpaqueCall,
    DynamicCall,
    UnmodeledDecorator,
    UnmodeledContextManager,
    UnsupportedCallback,
    AnalysisCutoff,
}

impl AnalysisGapKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::OpaqueCall => "analysis-gap/opaque-call",
            Self::DynamicCall => "analysis-gap/dynamic-call",
            Self::UnmodeledDecorator => "analysis-gap/unmodeled-decorator",
            Self::UnmodeledContextManager => "analysis-gap/unmodeled-context-manager",
            Self::UnsupportedCallback => "analysis-gap/unsupported-callback",
            Self::AnalysisCutoff => "analysis-gap/analysis-cutoff",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::OpaqueCall => "opaque or native calls",
            Self::DynamicCall => "dynamic call targets",
            Self::UnmodeledDecorator => "unmodeled decorators",
            Self::UnmodeledContextManager => "unmodeled context managers",
            Self::UnsupportedCallback => "unsupported callbacks",
            Self::AnalysisCutoff => "analysis cutoffs",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum AnalysisGapImpact {
    MayMissErrors,
    MayOverReport,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct AnalysisGap {
    kind: AnalysisGapKind,
    impact: AnalysisGapImpact,
    file: File,
    range: TextRange,
    target: Option<String>,
}

impl AnalysisGap {
    pub(crate) fn new(
        kind: AnalysisGapKind,
        impact: AnalysisGapImpact,
        file: File,
        range: TextRange,
        target: Option<String>,
    ) -> Self {
        Self {
            kind,
            impact,
            file,
            range,
            target,
        }
    }

    pub fn kind(&self) -> AnalysisGapKind {
        self.kind
    }

    pub fn impact(&self) -> AnalysisGapImpact {
        self.impact
    }

    pub fn file(&self) -> File {
        self.file
    }

    pub fn range(&self) -> TextRange {
        self.range
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn message(&self) -> String {
        let target = self
            .target
            .as_deref()
            .map(|target| format!(" `{target}`"))
            .unwrap_or_default();
        match self.kind {
            AnalysisGapKind::OpaqueCall => {
                format!("Cannot inspect exception effects of opaque or native call{target}")
            }
            AnalysisGapKind::DynamicCall => {
                "Cannot determine the exception effects of this dynamic call".into()
            }
            AnalysisGapKind::UnmodeledDecorator => {
                format!("Cannot determine how decorator{target} changes exception effects")
            }
            AnalysisGapKind::UnmodeledContextManager => format!(
                "Cannot determine the enter, exit, or suppression effects of context manager{target}"
            ),
            AnalysisGapKind::UnsupportedCallback => {
                "Cannot follow this callback through the higher-order call".into()
            }
            AnalysisGapKind::AnalysisCutoff => {
                format!("Stopped exception analysis to avoid a cycle{target}")
            }
        }
    }
}

impl From<&AnalysisGap> for Diagnostic {
    fn from(gap: &AnalysisGap) -> Self {
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(LintName::of(gap.kind.code())),
            Severity::Info,
            gap.message(),
        );
        diagnostic.annotate(Annotation::primary(Span::from(FileRange::new(
            gap.file, gap.range,
        ))));
        diagnostic
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) struct FunctionAnalysis {
    pub(crate) errors: Vec<FunctionRaise>,
    pub(crate) gaps: Vec<AnalysisGap>,
}
