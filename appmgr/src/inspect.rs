use std::path::Path;

use failure::ResultExt as _;
use futures::stream::StreamExt;
use tokio_tar as tar;

use crate::config::{ConfigRuleEntry, ConfigSpec};
use crate::manifest::{Manifest, ManifestLatest};
use crate::util::from_cbor_async_reader;
use crate::version::VersionT;
use crate::Error;
use crate::ResultExt as _;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppInfoFull {
    #[serde(flatten)]
    pub info: AppInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ManifestLatest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<AppConfig>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppInfo {
    pub title: String,
    pub version: emver::Version,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    pub spec: ConfigSpec,
    pub rules: Vec<ConfigRuleEntry>,
}

pub async fn info_full<P: AsRef<Path>>(
    path: P,
    with_manifest: bool,
    with_config: bool,
) -> Result<AppInfoFull, Error> {
    let p = path.as_ref();
    log::info!("Opening file.");
    let r = tokio::fs::File::open(p)
        .await
        .with_context(|e| format!("{}: {}", p.display(), e))
        .with_code(crate::error::FILESYSTEM_ERROR)?;
    log::info!("Extracting archive.");
    let mut pkg = tar::Archive::new(r);
    let mut entries = pkg.entries()?;
    log::info!("Opening manifest from archive.");
    let manifest = entries
        .next()
        .await
        .ok_or(crate::install::Error::CorruptedPkgFile("missing manifest"))
        .no_code()??;
    crate::ensure_code!(
        manifest.path()?.to_str() == Some("manifest.cbor"),
        crate::error::GENERAL_ERROR,
        "Package File Invalid or Corrupted"
    );
    log::trace!("Deserializing manifest.");
    let manifest: Manifest = from_cbor_async_reader(manifest).await?;
    let manifest = manifest.into_latest();
    crate::ensure_code!(
        crate::version::Current::new()
            .semver()
            .satisfies(&manifest.os_version_required),
        crate::error::VERSION_INCOMPATIBLE,
        "AppMgr Version Not Compatible: needs {}",
        manifest.os_version_required
    );
    Ok(AppInfoFull {
        info: AppInfo {
            title: manifest.title.clone(),
            version: manifest.version.clone(),
        },
        manifest: if with_manifest { Some(manifest) } else { None },
        config: if with_config {
            log::info!("Opening config spec from archive.");
            let spec = entries
                .next()
                .await
                .ok_or(crate::install::Error::CorruptedPkgFile(
                    "missing config spec",
                ))
                .no_code()??;
            crate::ensure_code!(
                spec.path()?.to_str() == Some("config_spec.cbor"),
                crate::error::GENERAL_ERROR,
                "Package File Invalid or Corrupted"
            );
            log::trace!("Deserializing config spec.");
            let spec = from_cbor_async_reader(spec).await?;
            log::info!("Opening config rules from archive.");
            let rules = entries
                .next()
                .await
                .ok_or(crate::install::Error::CorruptedPkgFile(
                    "missing config rules",
                ))
                .no_code()??;
            crate::ensure_code!(
                rules.path()?.to_str() == Some("config_rules.cbor"),
                crate::error::GENERAL_ERROR,
                "Package File Invalid or Corrupted"
            );
            log::trace!("Deserializing config rules.");
            let rules = from_cbor_async_reader(rules).await?;
            Some(AppConfig { spec, rules })
        } else {
            None
        },
    })
}

pub async fn print_instructions<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let p = path.as_ref();
    log::info!("Opening file.");
    let r = tokio::fs::File::open(p)
        .await
        .with_context(|e| format!("{}: {}", p.display(), e))
        .with_code(crate::error::FILESYSTEM_ERROR)?;
    log::info!("Extracting archive.");
    let mut pkg = tar::Archive::new(r);
    let mut entries = pkg.entries()?;
    log::info!("Opening manifest from archive.");
    let manifest = entries
        .next()
        .await
        .ok_or(crate::install::Error::CorruptedPkgFile("missing manifest"))
        .no_code()??;
    crate::ensure_code!(
        manifest.path()?.to_str() == Some("manifest.cbor"),
        crate::error::GENERAL_ERROR,
        "Package File Invalid or Corrupted"
    );
    log::trace!("Deserializing manifest.");
    let manifest: Manifest = from_cbor_async_reader(manifest).await?;
    let manifest = manifest.into_latest();
    crate::ensure_code!(
        crate::version::Current::new()
            .semver()
            .satisfies(&manifest.os_version_required),
        crate::error::VERSION_INCOMPATIBLE,
        "AppMgr Version Not Compatible: needs {}",
        manifest.os_version_required
    );
    entries
        .next()
        .await
        .ok_or(crate::install::Error::CorruptedPkgFile(
            "missing config spec",
        ))
        .no_code()??;
    entries
        .next()
        .await
        .ok_or(crate::install::Error::CorruptedPkgFile(
            "missing config rules",
        ))
        .no_code()??;

    if manifest.has_instructions {
        use tokio::io::AsyncWriteExt;

        let mut instructions = entries
            .next()
            .await
            .ok_or(crate::install::Error::CorruptedPkgFile(
                "missing instructions",
            ))
            .no_code()??;

        let mut stdout = tokio::io::stdout();
        tokio::io::copy(&mut instructions, &mut stdout)
            .await
            .with_code(crate::error::FILESYSTEM_ERROR)?;
        stdout
            .flush()
            .await
            .with_code(crate::error::FILESYSTEM_ERROR)?;
        stdout
            .shutdown()
            .await
            .with_code(crate::error::FILESYSTEM_ERROR)?;
    } else {
        return Err(failure::format_err!("No instructions for {}", p.display()))
            .with_code(crate::error::NOT_FOUND);
    }

    Ok(())
}

pub mod commands {
    use clap::ArgMatches;
    use futures::{future::BoxFuture, FutureExt};

    use crate::api::{Api, Argument};
    use crate::{Error, ResultExt};

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Path;
    impl Argument for Path {
        fn name(&self) -> &'static str {
            "PATH"
        }
        fn help(&self) -> Option<&'static str> {
            Some("Path to the s9pk file to inspect")
        }
        fn required(&self) -> bool {
            true
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Json;
    impl Argument for Json {
        fn name(&self) -> &'static str {
            "json"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["yaml"]
        }
        fn required_unless(&self) -> Option<&'static str> {
            Some(Yaml.name())
        }
        fn long(&self) -> Option<&'static str> {
            Some("json")
        }
        fn short(&self) -> Option<&'static str> {
            Some("j")
        }
        fn help(&self) -> Option<&'static str> {
            Some("Output as JSON")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Pretty;
    impl Argument for Pretty {
        fn name(&self) -> &'static str {
            "pretty"
        }
        fn requires(&self) -> Option<&'static str> {
            Some(Json.name())
        }
        fn long(&self) -> Option<&'static str> {
            Some("pretty")
        }
        fn short(&self) -> Option<&'static str> {
            Some("p")
        }
        fn help(&self) -> Option<&'static str> {
            Some("Pretty print output")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Yaml;
    impl Argument for Yaml {
        fn name(&self) -> &'static str {
            "yaml"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["json"]
        }
        fn required_unless(&self) -> Option<&'static str> {
            Some(Json.name())
        }
        fn long(&self) -> Option<&'static str> {
            Some("yaml")
        }
        fn short(&self) -> Option<&'static str> {
            Some("y")
        }
        fn help(&self) -> Option<&'static str> {
            Some("Output as YAML")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct IncludeManifest;
    impl Argument for IncludeManifest {
        fn name(&self) -> &'static str {
            "include-manifest"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["only-manifest", "only-config"]
        }
        fn long(&self) -> Option<&'static str> {
            Some("include-manifest")
        }
        fn short(&self) -> Option<&'static str> {
            Some("m")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct IncludeConfig;
    impl Argument for IncludeConfig {
        fn name(&self) -> &'static str {
            "include-config"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["only-manifest", "only-config"]
        }
        fn long(&self) -> Option<&'static str> {
            Some("include-config")
        }
        fn short(&self) -> Option<&'static str> {
            Some("c")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct OnlyManifest;
    impl Argument for OnlyManifest {
        fn name(&self) -> &'static str {
            "only-manifest"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["include-manifest", "include-config", "only-config"]
        }
        fn long(&self) -> Option<&'static str> {
            Some("only-manifest")
        }
        fn short(&self) -> Option<&'static str> {
            Some("M")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct OnlyConfig;
    impl Argument for OnlyConfig {
        fn name(&self) -> &'static str {
            "only-config"
        }
        fn conflicts_with(&self) -> &'static [&'static str] {
            &["include-manifest", "include-config", "only-manifest"]
        }
        fn long(&self) -> Option<&'static str> {
            Some("only-config")
        }
        fn short(&self) -> Option<&'static str> {
            Some("C")
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Info;
    impl Info {
        async fn clap_impl<'a>(&'a self, matches: &'a ArgMatches<'a>) -> Result<(), Error> {
            let path = matches.value_of(Path.name()).unwrap();
            let info = crate::inspect::info_full(
                path,
                matches.is_present(IncludeManifest.name())
                    || matches.is_present(OnlyManifest.name()),
                matches.is_present(IncludeConfig.name()) || matches.is_present(OnlyConfig.name()),
            )
            .await?;

            if matches.is_present(Json.name()) {
                if matches.is_present(Pretty.name()) {
                    if matches.is_present(OnlyManifest.name()) {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&info.manifest)
                                .with_code(crate::error::SERDE_ERROR)?
                        );
                    } else if matches.is_present(OnlyConfig.name()) {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&info.config)
                                .with_code(crate::error::SERDE_ERROR)?
                        );
                    } else {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&info)
                                .with_code(crate::error::SERDE_ERROR)?
                        );
                    }
                } else {
                    if matches.is_present(OnlyManifest.name()) {
                        println!(
                            "{}",
                            serde_json::to_string(&info.manifest)
                                .with_code(crate::error::SERDE_ERROR)?
                        );
                    } else if matches.is_present(OnlyConfig.name()) {
                        println!(
                            "{}",
                            serde_json::to_string(&info.config)
                                .with_code(crate::error::SERDE_ERROR)?
                        );
                    } else {
                        println!(
                            "{}",
                            serde_json::to_string(&info).with_code(crate::error::SERDE_ERROR)?
                        );
                    }
                }
            } else if matches.is_present(Yaml.name()) {
                if matches.is_present(OnlyManifest.name()) {
                    println!(
                        "{}",
                        serde_yaml::to_string(&info.manifest)
                            .with_code(crate::error::SERDE_ERROR)?
                    );
                } else if matches.is_present(OnlyConfig.name()) {
                    println!(
                        "{}",
                        serde_yaml::to_string(&info.config).with_code(crate::error::SERDE_ERROR)?
                    );
                } else {
                    println!(
                        "{}",
                        serde_yaml::to_string(&info).with_code(crate::error::SERDE_ERROR)?
                    );
                }
            }
            Ok(())
        }
    }
    impl Api for Info {
        fn name(&self) -> &'static str {
            "info"
        }
        fn clap_impl<'a>(
            &'a self,
            matches: &'a ArgMatches,
        ) -> Option<BoxFuture<'a, Result<(), Error>>> {
            Some(self.clap_impl(matches).boxed())
        }
        fn about(&self) -> Option<&'static str> {
            Some("Prints information about an application package")
        }
        fn args(&self) -> &'static [&'static dyn Argument] {
            &[
                &Path,
                &Json,
                &Pretty,
                &Yaml,
                &IncludeManifest,
                &IncludeConfig,
                &OnlyManifest,
                &OnlyConfig,
            ]
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Instructions;
    impl Api for Instructions {
        fn name(&self) -> &'static str {
            "instructions"
        }
        fn clap_impl<'a>(
            &'a self,
            matches: &'a ArgMatches<'a>,
        ) -> Option<BoxFuture<'a, Result<(), Error>>> {
            Some(
                super::print_instructions(std::path::Path::new(
                    matches.value_of(Path.name()).unwrap(),
                ))
                .boxed(),
            )
        }
        fn about(&self) -> Option<&'static str> {
            Some("Prints instructions for an application package")
        }
        fn args(&self) -> &'static [&'static dyn Argument] {
            &[&Path]
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    pub struct Inspect;
    impl Api for Inspect {
        fn name(&self) -> &'static str {
            "inspect"
        }
        fn about(&self) -> Option<&'static str> {
            Some("Inspects an application package")
        }
        fn commands(&self) -> &'static [&'static dyn Api] {
            &[&Info, &Instructions]
        }
    }
}
