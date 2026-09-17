use std::{io::ErrorKind, sync::Arc};

use anyhow::{Context, Result};
use py_checked_exceptions::{AnalysisExtension, ContextManagerEffectRule};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use serde::Deserialize;
use ty_project::metadata::{
    Options,
    options::{EnvironmentOptions, SrcOptions, TerminalOptions},
    value::{RangedValue, RelativeGlobPattern, RelativePathBuf, ValueSource},
};

use crate::args::{AnalysisGapOutput, OutputFormat, TerminalColor};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct ProjectConfig {
    pub(crate) target_exceptions: Option<Vec<String>>,
    pub(crate) extensions: Option<Vec<AnalysisExtension>>,
    pub(crate) context_manager_effects: Option<Vec<ContextManagerEffectRule>>,
    pub(crate) python: Option<String>,
    pub(crate) typeshed: Option<String>,
    pub(crate) extra_search_paths: Option<Vec<String>>,
    pub(crate) output_format: Option<OutputFormat>,
    pub(crate) color: Option<TerminalColor>,
    pub(crate) show_analysis_gaps: Option<AnalysisGapOutput>,
    pub(crate) respect_ignore_files: Option<bool>,
    pub(crate) exclude: Option<Vec<String>>,
}

impl ProjectConfig {
    pub(crate) fn as_ty_options(&self, config_path: &SystemPath) -> Options {
        let source = ValueSource::File(Arc::new(config_path.to_path_buf()));
        Options {
            environment: Some(EnvironmentOptions {
                python_version: None,
                python_platform: None,
                python: self
                    .python
                    .as_ref()
                    .map(|path| RelativePathBuf::new(path, source.clone())),
                typeshed: self
                    .typeshed
                    .as_ref()
                    .map(|path| RelativePathBuf::new(path, source.clone())),
                extra_paths: self.extra_search_paths.as_ref().map(|paths| {
                    paths
                        .iter()
                        .map(|path| RelativePathBuf::new(path, source.clone()))
                        .collect()
                }),
                ..EnvironmentOptions::default()
            }),
            terminal: Some(TerminalOptions {
                output_format: self
                    .output_format
                    .map(|format| RangedValue::new(format.into(), source.clone())),
                error_on_warning: None,
            }),
            src: Some(SrcOptions {
                respect_ignore_files: self.respect_ignore_files,
                exclude: self.exclude.as_ref().map(|patterns| {
                    RangedValue::new(
                        patterns
                            .iter()
                            .map(|pattern| RelativeGlobPattern::new(pattern, source.clone()))
                            .collect(),
                        source,
                    )
                }),
                ..SrcOptions::default()
            }),
            rules: None,
            ..Options::default()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct PyProject {
    tool: Option<Tool>,
}

#[derive(Debug, Default, Deserialize)]
struct Tool {
    #[serde(rename = "py-checked-exceptions")]
    py_checked_exceptions: Option<ProjectConfig>,
}

pub(crate) struct LoadedProjectConfig {
    pub(crate) config: ProjectConfig,
    pub(crate) path: SystemPathBuf,
}

pub(crate) fn load_project_config(
    system: &dyn System,
    project_root: &SystemPath,
) -> Result<LoadedProjectConfig> {
    let path = project_root.join("pyproject.toml");
    let content = match system.read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(LoadedProjectConfig {
                config: ProjectConfig::default(),
                path,
            });
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to read configuration from `{path}`"));
        }
    };
    let pyproject: PyProject = toml::from_str(&content)
        .with_context(|| format!("Failed to parse configuration from `{path}`"))?;
    let config = pyproject
        .tool
        .and_then(|tool| tool.py_checked_exceptions)
        .unwrap_or_default();
    Ok(LoadedProjectConfig { config, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use py_checked_exceptions::ContextManagerEffect;

    #[test]
    fn parses_pyproject_configuration() {
        let pyproject: PyProject = toml::from_str(
            r#"
                [project]
                name = "example"

                [tool.other]
                value = true

                [tool.py-checked-exceptions]
                target-exceptions = ["example.BaseError"]
                extensions = ["fastapi"]
                python = ".venv"
                typeshed = "typings/typeshed"
                extra-search-paths = ["packages/one", "packages/two"]
                output-format = "concise"
                color = "never"
                show-analysis-gaps = "full"
                respect-ignore-files = false
                exclude = ["generated", "tests/fixtures/**"]

                [[tool.py-checked-exceptions.context-manager-effects]]
                function = "example.suppress_error"
                exception-parameter = "error_type"
                effect = "optional"

                [[tool.py-checked-exceptions.context-manager-effects]]
                function = "example.suppress_error"
                exception-parameter = "error_type"
                effect = "suppress"
            "#,
        )
        .unwrap();
        let config = pyproject.tool.unwrap().py_checked_exceptions.unwrap();

        assert_eq!(
            config.target_exceptions,
            Some(vec!["example.BaseError".into()])
        );
        assert_eq!(config.extensions, Some(vec![AnalysisExtension::Fastapi]));
        let effects = config.context_manager_effects.unwrap();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0].function, "example.suppress_error");
        assert_eq!(effects[0].exception_parameter, "error_type");
        assert_eq!(effects[0].effect, ContextManagerEffect::Optional);
        assert_eq!(effects[1].effect, ContextManagerEffect::Suppress);
        assert_eq!(config.python.as_deref(), Some(".venv"));
        assert_eq!(config.typeshed.as_deref(), Some("typings/typeshed"));
        assert_eq!(
            config.extra_search_paths,
            Some(vec!["packages/one".into(), "packages/two".into()])
        );
        assert_eq!(config.output_format, Some(OutputFormat::Concise));
        assert_eq!(config.color, Some(TerminalColor::Never));
        assert_eq!(config.show_analysis_gaps, Some(AnalysisGapOutput::Full));
        assert_eq!(config.respect_ignore_files, Some(false));
        assert_eq!(
            config.exclude,
            Some(vec!["generated".into(), "tests/fixtures/**".into()])
        );
    }

    #[test]
    fn rejects_unknown_configuration_fields() {
        let error = toml::from_str::<PyProject>(
            r#"
                [tool.py-checked-exceptions]
                target-exception = ["example.BaseError"]
            "#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unknown field `target-exception`")
        );
    }

    #[test]
    fn configuration_paths_are_relative_to_pyproject() {
        let config = ProjectConfig {
            python: Some(".venv".into()),
            typeshed: Some("typings/typeshed".into()),
            extra_search_paths: Some(vec!["packages/shared".into()]),
            ..ProjectConfig::default()
        };
        let config_path = SystemPath::new("/project/pyproject.toml");
        let options = config.as_ty_options(config_path);
        let environment = options.environment.unwrap();

        assert_eq!(
            environment.python.unwrap().source().file(),
            Some(config_path)
        );
        assert_eq!(
            environment.typeshed.unwrap().source().file(),
            Some(config_path)
        );
        assert_eq!(
            environment.extra_paths.unwrap()[0].source().file(),
            Some(config_path)
        );
    }
}
