use super::prelude::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct Portable;
impl Api for Portable {
    fn name(&self) -> &'static str {
        "Start9 Application Manager (portable)"
    }
    fn about(&self) -> Option<&'static str> {
        Some(clap::crate_description!())
    }
    fn version(&self) -> Option<&str> {
        Some(clap::crate_version!())
    }
    fn author(&self) -> Option<&'static str> {
        Some(clap::crate_authors!("\n"))
    }
    fn clap_pre_hook<'a>(&'a self, matches: &'a ArgMatches<'a>) -> ClapImpl<'a> {
        Some(
            async move {
                simple_logging::log_to_stderr(match matches.occurrences_of(Verbosity.name()) {
                    0 => log::LevelFilter::Error,
                    1 => log::LevelFilter::Warn,
                    2 => log::LevelFilter::Info,
                    3 => log::LevelFilter::Debug,
                    _ => log::LevelFilter::Trace,
                });
                Ok(())
            }
            .boxed(),
        )
    }
    fn args(&self) -> &'static [&'static dyn Argument] {
        &[&Verbosity]
    }
    fn commands(&self) -> &'static [&'static dyn Api] {
        &[
            &Semver,
            &GitInfo,
            &crate::pack::commands::Pack,
            &crate::pack::commands::Verify,
            &crate::inspect::commands::Inspect,
            &crate::index::commands::Index,
        ]
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Full;
impl Api for Full {
    fn name(&self) -> &'static str {
        "Start9 Application Manager"
    }
    fn clap_pre_hook<'a>(&'a self, matches: &'a ArgMatches<'a>) -> ClapImpl<'a> {
        Some(
            async move {
                simple_logging::log_to_stderr(match matches.occurrences_of(Verbosity.name()) {
                    0 => log::LevelFilter::Error,
                    1 => log::LevelFilter::Warn,
                    2 => log::LevelFilter::Info,
                    3 => log::LevelFilter::Debug,
                    _ => log::LevelFilter::Trace,
                });
                Ok(())
            }
            .boxed(),
        )
    }
    fn args(&self) -> &'static [&'static dyn Argument] {
        &[&Verbosity]
    }
    fn commands(&self) -> &'static [&'static dyn Api] {
        &[
            &Semver,
            &GitInfo,
            &crate::pack::commands::Pack,
            &crate::pack::commands::Verify,
            &crate::inspect::commands::Inspect,
            &crate::index::commands::Index,
            // &crate::install::commands::Install,
            // &crate::update::commands::Update,
            // &crate::control::commands::Start,
            // &crate::control::commands::Stop,
            // &crate::control::commands::Restart,
            // &crate::config::commands::Configure,
            // &crate::dependencies::commands::CheckDependencies,
            // &crate::dependencies::commands::AutoConfigureDependencies,
            // &crate::remove::commands::Remove,
            // &crate::tor::commands::Tor,
            // &crate::apps::commands::Info,
            // &crate::apps::commands::Instructions,
            // &crate::apps::commands::List,
            // &crate::version::commands::SelfUpdate,
            // &crate::logs::commands::Logs,
            // &crate::logs::commands::Notifications,
            // &crate::logs::commands::Properties,
            // &crate::disks::commands::Disks,
            // &crate::backup::commands::Backup,
        ]
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Semver;
impl Semver {
    fn version(&self) -> &'static emver::Version {
        use crate::version::VersionT;

        crate::version::Current::new().semver()
    }
}
impl Api for Semver {
    fn name(&self) -> &'static str {
        "semver"
    }
    fn clap_impl<'a>(
        &'a self,
        _full_command: &'a [&'a dyn Api],
        _matches: &'a ArgMatches,
    ) -> ClapImpl<'a> {
        Some(
            async move {
                println!("{}", self.version());
                Ok(())
            }
            .boxed(),
        )
    }
    fn hyper_impl<'a>(&'a self, request: &'a Parts, _query: &'a QueryMap<'a>) -> HyperImpl<'a> {
        Some(Box::new(move |_body| {
            async move { serde_res(request, &self.version()) }.boxed()
        }))
    }
    fn about(&self) -> Option<&'static str> {
        Some("Prints semantic version and exits")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GitInfo;
impl GitInfo {
    fn info(&self) -> String {
        let git_version =
            git_version::git_version!(args = ["--always", "--abbrev=40", "--dirty=-modified"]);
        #[cfg(not(feature = "production"))]
        let git_version = format!("{}-dev", git_version);
        git_version
    }
}
impl Api for GitInfo {
    fn name(&self) -> &'static str {
        "git-info"
    }
    fn clap_impl<'a>(
        &'a self,
        _full_command: &'a [&'a dyn Api],
        _matches: &'a ArgMatches,
    ) -> ClapImpl<'a> {
        Some(
            async move {
                println!("{}", self.info());
                Ok(())
            }
            .boxed(),
        )
    }
    fn hyper_impl<'a>(&'a self, request: &'a Parts, _query: &'a QueryMap<'a>) -> HyperImpl<'a> {
        Some(Box::new(move |_body| {
            async move { serde_res(&request, &self.info()) }.boxed()
        }))
    }
    fn about(&self) -> Option<&'static str> {
        Some("Prints git version info and exits")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Verbosity;
impl Argument for Verbosity {
    fn name(&self) -> &'static str {
        "verbosity"
    }
    fn short(&self) -> Option<&'static str> {
        Some("v")
    }
    fn help(&self) -> Option<&'static str> {
        Some("Sets verbosity level")
    }
    fn multiple(&self) -> bool {
        true
    }
}
