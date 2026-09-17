pub(crate) mod fastapi;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum AnalysisExtension {
    /// Model FastAPI response documentation and dependency injection.
    Fastapi,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisOptions {
    extensions: Vec<AnalysisExtension>,
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
}
