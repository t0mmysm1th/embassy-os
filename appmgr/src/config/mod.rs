use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use futures::future::{BoxFuture, FutureExt};
use hashlink::{LinkedHashMap as Map, LinkedHashSet as Set};
use itertools::Itertools;
use rand::SeedableRng;
use regex::Regex;

use crate::dependencies::{DependencyError, TaggedDependencyError};
use crate::util::PersistencePath;
use crate::util::{from_yaml_async_reader, to_yaml_async_writer};
use crate::ResultExt;

pub mod rules;
pub mod spec;
pub mod util;
pub mod value;

pub use rules::{ConfigRuleEntry, ConfigRuleEntryWithSuggestions};
pub use spec::{ConfigSpec, Defaultable};
use util::NumRange;
pub use value::Config;

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("Timeout Error")]
    TimeoutError(#[from] TimeoutError),
    #[error("No Match: {0}")]
    NoMatch(#[from] NoMatchWithPath),
    #[error("Invalid Variant: {0}")]
    InvalidVariant(String),
    #[error("System Error: {0}")]
    SystemError(#[from] crate::Error),
}

#[derive(Clone, Copy, Debug, Error)]
#[error("Timeout Error")]
pub struct TimeoutError;

#[derive(Clone, Debug, Error)]
pub struct NoMatchWithPath {
    pub path: Vec<String>,
    pub error: MatchError,
}
impl NoMatchWithPath {
    pub fn new(error: MatchError) -> Self {
        NoMatchWithPath {
            path: Vec::new(),
            error,
        }
    }
    pub fn prepend(mut self, seg: String) -> Self {
        self.path.push(seg);
        self
    }
}
impl std::fmt::Display for NoMatchWithPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.iter().rev().join("."), self.error)
    }
}

#[derive(Clone, Debug, Error)]
pub enum MatchError {
    #[error("String {0:?} Does Not Match Pattern {1}")]
    Pattern(String, Regex),
    #[error("String {0:?} Is Not In Enum {1:?}")]
    Enum(String, Set<String>),
    #[error("Field Is Not Nullable")]
    NotNullable,
    #[error("Length Mismatch: expected {0}, actual: {1}")]
    LengthMismatch(NumRange<usize>, usize),
    #[error("Invalid Type: expected {0}, actual: {1}")]
    InvalidType(&'static str, &'static str),
    #[error("Number Out Of Range: expected {0}, actual: {1}")]
    OutOfRange(NumRange<f64>, f64),
    #[error("Number Is Not Integral: {0}")]
    NonIntegral(f64),
    #[error("Variant {0:?} Is Not In Union {1:?}")]
    Union(String, Set<String>),
    #[error("Variant Is Missing Tag {0:?}")]
    MissingTag(String),
    #[error("Property {0:?} Of Variant {1:?} Conflicts With Union Tag")]
    PropertyMatchesUnionTag(String, String),
    #[error("Name of Property {0:?} Conflicts With Map Tag Name")]
    PropertyNameMatchesMapTag(String),
    #[error("Pointer Is Invalid: {0}")]
    InvalidPointer(spec::ValueSpecPointer),
    #[error("Object Key Is Invalid: {0}")]
    InvalidKey(String),
    #[error("Value In List Is Not Unique")]
    ListUniquenessViolation,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigurationRes {
    pub changed: Map<String, Config>,
    pub needs_restart: Set<String>,
    pub stopped: Map<String, TaggedDependencyError>,
}

// returns apps with changed configurations
pub async fn configure(
    name: &str,
    config: Option<Config>,
    timeout: Option<Duration>,
    dry_run: bool,
) -> Result<ConfigurationRes, crate::Error> {
    async fn handle_broken_dependent(
        name: &str,
        dependent: String,
        dry_run: bool,
        res: &mut ConfigurationRes,
        error: DependencyError,
    ) -> Result<(), crate::Error> {
        crate::control::stop_dependents(
            &dependent,
            dry_run,
            DependencyError::NotRunning,
            &mut res.stopped,
        )
        .await?;
        if crate::apps::status(&dependent, false).await?.status
            != crate::apps::DockerStatus::Stopped
        {
            crate::control::stop_app(&dependent, false, dry_run).await?;
            res.stopped.insert(
                // TODO: maybe don't do this if its not running
                dependent,
                TaggedDependencyError {
                    dependency: name.to_owned(),
                    error,
                },
            );
        }
        Ok(())
    }
    fn configure_rec<'a>(
        name: &'a str,
        config: Option<Config>,
        timeout: Option<Duration>,
        dry_run: bool,
        res: &'a mut ConfigurationRes,
    ) -> BoxFuture<'a, Result<Config, crate::Error>> {
        async move {
            let info = crate::apps::list_info()
                .await?
                .remove(name)
                .ok_or_else(|| anyhow!("{} is not installed", name))
                .with_code(crate::error::NOT_FOUND)?;
            let mut rng = rand::rngs::StdRng::from_entropy();
            let spec_path = PersistencePath::from_ref("apps")
                .join(name)
                .join("config_spec.yaml");
            let rules_path = PersistencePath::from_ref("apps")
                .join(name)
                .join("config_rules.yaml");
            let config_path = PersistencePath::from_ref("apps")
                .join(name)
                .join("config.yaml");
            let spec: ConfigSpec =
                from_yaml_async_reader(&mut *spec_path.read(false).await?).await?;
            let rules: Vec<ConfigRuleEntry> =
                from_yaml_async_reader(&mut *rules_path.read(false).await?).await?;
            let old_config: Option<Config> =
                if let Some(mut f) = config_path.maybe_read(false).await.transpose()? {
                    Some(from_yaml_async_reader(&mut *f).await?)
                } else {
                    None
                };
            let mut config = if let Some(cfg) = config {
                cfg
            } else {
                if let Some(old) = &old_config {
                    old.clone()
                } else {
                    spec.gen(&mut rng, &timeout)
                        .with_code(crate::error::CFG_SPEC_VIOLATION)?
                }
            };
            spec.matches(&config)
                .with_code(crate::error::CFG_SPEC_VIOLATION)?;
            spec.update(&mut config)
                .await
                .with_code(crate::error::CFG_SPEC_VIOLATION)?;
            let mut cfgs = Map::new();
            cfgs.insert(name, Cow::Borrowed(&config));
            for rule in rules {
                rule.check(&config, &cfgs)
                    .with_code(crate::error::CFG_RULES_VIOLATION)?;
            }
            match old_config {
                Some(old) if &old == &config && info.configured && !info.recoverable => {
                    drop(cfgs);
                    return Ok(config);
                }
                _ => (),
            };
            res.changed.insert(name.to_owned(), config.clone());
            for dependent in crate::apps::dependents(name, false).await? {
                match configure_rec(&dependent, None, timeout, dry_run, res).await {
                    Ok(dependent_config) => {
                        let man = crate::apps::manifest(&dependent).await?;
                        if let Some(dep_info) = man.dependencies.0.get(name) {
                            match dep_info
                                .satisfied(
                                    name,
                                    Some(config.clone()),
                                    &dependent,
                                    &dependent_config,
                                )
                                .await?
                            {
                                Ok(_) => (),
                                Err(e) => {
                                    handle_broken_dependent(name, dependent, dry_run, res, e)
                                        .await?;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if e.code == crate::error::CFG_RULES_VIOLATION
                            || e.code == crate::error::CFG_SPEC_VIOLATION
                        {
                            if !dry_run {
                                crate::apps::set_configured(&dependent, false).await?;
                            }
                            handle_broken_dependent(
                                name,
                                dependent,
                                dry_run,
                                res,
                                DependencyError::PointerUpdateError(format!("{}", e)),
                            )
                            .await?;
                        } else {
                            handle_broken_dependent(
                                name,
                                dependent,
                                dry_run,
                                res,
                                DependencyError::Other(format!("{}", e)),
                            )
                            .await?;
                        }
                    }
                }
            }
            if !dry_run {
                let mut file = config_path.write(None).await?;
                to_yaml_async_writer(file.as_mut(), &config).await?;
                file.commit().await?;
                let volume_config = Path::new(crate::VOLUMES)
                    .join(name)
                    .join("start9")
                    .join("config.yaml");
                tokio::fs::copy(config_path.path(), &volume_config)
                    .await
                    .with_context(|| {
                        format!(
                            "{} -> {}",
                            config_path.path().display(),
                            volume_config.display()
                        )
                    })
                    .with_code(crate::error::FILESYSTEM_ERROR)?;
                crate::apps::set_configured(name, true).await?;
                crate::apps::set_recoverable(name, false).await?;
            }
            if crate::apps::status(name, false).await?.status != crate::apps::DockerStatus::Stopped
            {
                if !dry_run {
                    crate::apps::set_needs_restart(name, true).await?;
                }
                res.needs_restart.insert(name.to_string());
            }
            drop(cfgs);
            Ok(config)
        }
        .boxed()
    }
    let mut res = ConfigurationRes::default();
    configure_rec(name, config, timeout, dry_run, &mut res).await?;
    Ok(res)
}

pub async fn remove(name: &str) -> Result<(), crate::Error> {
    let config_path = PersistencePath::from_ref("apps")
        .join(name)
        .join("config.yaml")
        .path();
    if config_path.exists() {
        tokio::fs::remove_file(&config_path)
            .await
            .with_context(|| format!("{}", config_path.display()))
            .with_code(crate::error::FILESYSTEM_ERROR)?;
    }
    let volume_config = Path::new(crate::VOLUMES)
        .join(name)
        .join("start9")
        .join("config.yaml");
    if volume_config.exists() {
        tokio::fs::remove_file(&volume_config)
            .await
            .with_context(|| format!("{}", volume_config.display()))
            .with_code(crate::error::FILESYSTEM_ERROR)?;
    }
    crate::apps::set_configured(name, false).await?;
    Ok(())
}
