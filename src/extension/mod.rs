pub(crate) mod fastapi;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum, serde::Deserialize, get_size2::GetSize,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisExtension {
    /// Model FastAPI response documentation and dependency injection.
    Fastapi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, get_size2::GetSize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextManagerEffect {
    /// Prevent matching exceptions from propagating out of the context manager.
    Suppress,
    /// Allow matching exceptions to propagate without requiring documentation.
    Optional,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, get_size2::GetSize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ContextManagerEffectRule {
    pub function: String,
    pub exception_parameter: String,
    pub effect: ContextManagerEffect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct AnalysisOptions {
    extensions: Vec<AnalysisExtension>,
    context_manager_effects: Vec<ContextManagerEffectRule>,
}

impl AnalysisOptions {
    pub fn with_extension(mut self, extension: AnalysisExtension) -> Self {
        if !self.extensions.contains(&extension) {
            self.extensions.push(extension);
        }
        self
    }

    pub fn with_extensions(
        mut self,
        extensions: impl IntoIterator<Item = AnalysisExtension>,
    ) -> Self {
        for extension in extensions {
            self = self.with_extension(extension);
        }
        self
    }

    pub fn extension_enabled(&self, extension: AnalysisExtension) -> bool {
        self.extensions.contains(&extension)
    }

    pub fn with_context_manager_effects(
        mut self,
        effects: impl IntoIterator<Item = ContextManagerEffectRule>,
    ) -> Self {
        self.context_manager_effects.extend(effects);
        self
    }

    pub(crate) fn context_manager_effects(
        &self,
    ) -> impl Iterator<Item = &ContextManagerEffectRule> {
        self.context_manager_effects.iter()
    }
}
