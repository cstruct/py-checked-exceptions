pub(crate) mod fastapi;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum, serde::Deserialize, get_size2::GetSize,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisExtension {
    /// Model FastAPI response documentation and dependency injection.
    Fastapi,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum, serde::Deserialize, get_size2::GetSize,
)]
pub enum Entrypoint {
    /// Check FastAPI route handlers while still following their transitive calls.
    #[value(name = "fastapi:route")]
    #[serde(rename = "fastapi:route")]
    FastapiRoute,
}

impl Entrypoint {
    fn required_extension(self) -> AnalysisExtension {
        match self {
            Self::FastapiRoute => AnalysisExtension::Fastapi,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::FastapiRoute => "fastapi:route",
        }
    }
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
    entrypoints: Vec<Entrypoint>,
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

    pub fn with_entrypoint(mut self, entrypoint: Entrypoint) -> Self {
        if !self.entrypoints.contains(&entrypoint) {
            self.entrypoints.push(entrypoint);
        }
        self
    }

    pub fn with_entrypoints(mut self, entrypoints: impl IntoIterator<Item = Entrypoint>) -> Self {
        for entrypoint in entrypoints {
            self = self.with_entrypoint(entrypoint);
        }
        self
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for entrypoint in &self.entrypoints {
            let required_extension = entrypoint.required_extension();
            anyhow::ensure!(
                self.extension_enabled(required_extension),
                "entrypoint `{}` requires the `fastapi` extension",
                entrypoint.name()
            );
        }
        Ok(())
    }

    pub(crate) fn entrypoints(&self) -> &[Entrypoint] {
        &self.entrypoints
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

#[cfg(test)]
mod tests {
    use super::{AnalysisOptions, Entrypoint};

    #[test]
    fn fastapi_route_entrypoint_requires_fastapi_extension() {
        let error = AnalysisOptions::default()
            .with_entrypoint(Entrypoint::FastapiRoute)
            .validate()
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "entrypoint `fastapi:route` requires the `fastapi` extension"
        );
    }
}
