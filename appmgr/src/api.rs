use std::future::Future;

use clap::ArgMatches;
use futures::{future::LocalBoxFuture, FutureExt};
use hyper::{body::HttpBody, http::request::Parts, Body, Response};
use linear_map::LinearMap;
use serde::{Deserialize, Serialize};

use crate::util::Apply;
use crate::version::VersionT;
use crate::{Error, ResultExt};

pub trait Api {
    fn name(&self) -> &'static str;
    fn clap_impl<'a>(
        &'a self,
        _matches: &'a ArgMatches<'a>,
    ) -> Option<LocalBoxFuture<'a, Result<(), Error>>> {
        None
    }
    fn hyper_impl<'a>(
        &'a self,
        _request: &'a Parts,
        _body: &'a mut Body,
        _query: &'a LinearMap<&'a str, &'a str>,
    ) -> Option<LocalBoxFuture<'a, Result<Response<Body>, Error>>> {
        None
    }
    fn about(&self) -> Option<&'static str> {
        None
    }
    fn args(&self) -> &'static [&'static dyn Argument] {
        &[]
    }
    fn commands(&self) -> &'static [&'static dyn Api] {
        &[]
    }
    fn takes_data(&self) -> bool {
        false
    }
    fn subscribe(&self) -> bool {
        false
    }
}

mod response {
    use std::borrow::Cow;

    use hyper::{Body, Response};

    pub fn bad_request<S: Into<Cow<'static, str>>>(msg: S) -> Response<Body> {
        let msg = msg.into();
        Response::builder()
            .header("content-type", "text/plain")
            .header("content-length", msg.len())
            .body(msg.into())
            .unwrap()
    }
}

pub trait Argument {
    fn name(&self) -> &'static str;
    fn hyper_validation<'a>(
        &self,
        _request: &'a Parts,
        query: &'a LinearMap<&'a str, &'a str>,
    ) -> Result<(), Response<Body>> {
        let val = query.get(self.name());
        if self.required() && val.is_none() {
            return Err(response::bad_request(format!("{}: required", self.name())));
        }
        if self.multiple() {
            if let Err(e) = val.map(|v| v.parse::<usize>()).unwrap_or(Ok(0)) {
                return Err(response::bad_request(format!(
                    "{}: expected usize: {}",
                    self.name(),
                    e,
                )));
            }
        }
        for arg in self.conflicts_with() {
            if query.contains_key(arg) {
                return Err(response::bad_request(format!(
                    "{}: conflicts with: {}",
                    self.name(),
                    arg,
                )));
            }
        }
        if let Some(arg) = self.required_unless() {
            if val.is_none() && !query.contains_key(arg) {
                return Err(response::bad_request(format!(
                    "{}: required unless: {}",
                    self.name(),
                    arg,
                )));
            }
        }
        if let Some(arg) = self.requires() {
            if val.is_some() && !query.contains_key(arg) {
                return Err(response::bad_request(format!(
                    "{}: requires: {}",
                    self.name(),
                    arg,
                )));
            }
        }
        Ok(())
    }
    fn help(&self) -> Option<&'static str> {
        None
    }
    fn short(&self) -> Option<&'static str> {
        None
    }
    fn long(&self) -> Option<&'static str> {
        None
    }
    fn default(&self) -> Option<&'static str> {
        None
    }
    fn required(&self) -> bool {
        false
    }
    fn takes_value(&self) -> bool {
        false
    }
    fn multiple(&self) -> bool {
        false
    }
    fn conflicts_with(&self) -> &'static [&'static str] {
        &[]
    }
    fn required_unless(&self) -> Option<&'static str> {
        None
    }
    fn requires(&self) -> Option<&'static str> {
        None
    }
}

pub fn serde_res<T: Serialize>(request: &Parts, val: &T) -> Result<Response<Body>, Error> {
    if match request.headers.get("accept") {
        Some(a)
            if a.to_str()
                .no_code()?
                .split(";")
                .next()
                .unwrap()
                .split(",")
                .map(|s| s.trim())
                .any(|t| t == "*/*" || t == "application/*" || t == "application/cbor") =>
        {
            true
        }
        _ => false,
    } {
        let res = serde_cbor::to_vec(val).with_code(crate::error::SERDE_ERROR)?;
        Ok(Response::builder()
            .header("content-type", "application/cbor")
            .header("content-length", res.len())
            .body(res.into())
            .no_code()?)
    } else {
        let res = serde_json::to_string(val).with_code(crate::error::SERDE_ERROR)?;
        Ok(Response::builder()
            .header("content-type", "application/json")
            .header("content-length", res.len())
            .body(res.into())
            .no_code()?)
    }
}

pub async fn serde_req_res<
    F: FnOnce(U) -> Fut,
    Fut: Future<Output = Result<T, Error>>,
    T: Serialize,
    U: for<'de> Deserialize<'de>,
>(
    request: &Parts,
    body: &mut Body,
    f: F,
) -> Result<Response<Body>, Error> {
    let mut data = Vec::new();
    while let Some(chunk) = body
        .data()
        .await
        .transpose()
        .with_code(crate::error::NETWORK_ERROR)?
    {
        data.extend_from_slice(&*chunk);
    }
    if match request.headers.get("content-type") {
        Some(a)
            if a.to_str()
                .no_code()?
                .split(";")
                .next()
                .unwrap()
                .apply(|t| t.trim())
                .apply(|t| t == "*/*" || t == "application/*" || t == "application/cbor") =>
        {
            true
        }
        _ => false,
    } {
        serde_res(
            request,
            &f(serde_cbor::from_slice(&data).with_code(crate::error::SERDE_ERROR)?).await?,
        )
    } else {
        serde_res(
            request,
            &f(serde_json::from_slice(&data).with_code(crate::error::SERDE_ERROR)?).await?,
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Portable;
impl Api for Portable {
    fn name(&self) -> &'static str {
        "Start9 Application Manager (portable)"
    }
    fn clap_impl<'a>(
        &'a self,
        matches: &'a ArgMatches,
    ) -> Option<LocalBoxFuture<'a, Result<(), Error>>> {
        Some(
            async move {
                log::set_max_level(match matches.occurrences_of(Verbosity.name()) {
                    0 => log::LevelFilter::Error,
                    1 => log::LevelFilter::Warn,
                    2 => log::LevelFilter::Info,
                    3 => log::LevelFilter::Debug,
                    _ => log::LevelFilter::Trace,
                });
                Ok(())
            }
            .boxed_local(),
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
pub struct Semver;
impl Semver {
    fn version(&self) -> &'static emver::Version {
        crate::version::Current::new().semver()
    }
}
impl Api for Semver {
    fn name(&self) -> &'static str {
        "semver"
    }
    fn clap_impl<'a>(
        &'a self,
        _matches: &'a ArgMatches,
    ) -> Option<LocalBoxFuture<'a, Result<(), Error>>> {
        Some(
            async move {
                println!("{}", self.version());
                Ok(())
            }
            .boxed_local(),
        )
    }
    fn hyper_impl<'a>(
        &'a self,
        request: &'a Parts,
        _body: &'a mut Body,
        _query: &'a LinearMap<&'a str, &'a str>,
    ) -> Option<LocalBoxFuture<'a, Result<Response<Body>, Error>>> {
        Some(async move { serde_res(request, &self.version()) }.boxed_local())
    }
    fn about(&self) -> Option<&'static str> {
        Some("Prints semantic version and exits")
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GitInfo;
impl GitInfo {
    fn info(&self) -> &str {
        git_version::git_version!(args = ["--always", "--abbrev=40", "--dirty=-modified"])
    }
}
impl Api for GitInfo {
    fn name(&self) -> &'static str {
        "git-info"
    }
    fn clap_impl<'a>(
        &'a self,
        _matches: &'a ArgMatches,
    ) -> Option<LocalBoxFuture<'a, Result<(), Error>>> {
        Some(
            async move {
                println!("{}", self.info());
                Ok(())
            }
            .boxed_local(),
        )
    }
    fn hyper_impl<'a>(
        &'a self,
        request: &'a Parts,
        _body: &'a mut Body,
        _query: &'a LinearMap<&'a str, &'a str>,
    ) -> Option<LocalBoxFuture<Result<Response<Body>, Error>>> {
        Some(async move { serde_res(&request, &self.info()) }.boxed_local())
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
