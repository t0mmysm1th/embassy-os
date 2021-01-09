pub mod api;
pub mod arg_value;
pub mod clap_helpers;
pub mod hyper_helpers;
pub mod prelude;

use clap::ArgMatches;
use futures::future::BoxFuture;
use hyper::{http::request::Parts, Body, Method, Response};

use crate::Error;

pub type ClapImpl<'a> = Option<BoxFuture<'a, Result<(), Error>>>;
pub type HyperImpl<'a> = Option<
    Box<
        dyn FnOnce(&'a mut Body) -> BoxFuture<'a, Result<Response<Body>, Error>> + Send + Sync + 'a,
    >,
>;

pub use api::{Full, Portable};
pub use arg_value::{ArgValue, QueryMap};
pub use clap_helpers::{forward_to_hyper_impl, run_cli};

pub trait Api: Send + Sync {
    fn name(&self) -> &'static str;
    fn clap_impl<'a>(
        &'a self,
        _full_command: &'a [&'a dyn Api],
        _matches: &'a ArgMatches<'a>,
    ) -> ClapImpl<'a> {
        None
    }
    fn clap_pre_hook<'a>(&'a self, _matches: &'a ArgMatches<'a>) -> ClapImpl<'a> {
        None
    }
    fn hyper_impl<'a, 'b>(
        &'a self,
        _request: &'a Parts,
        _query: &'a QueryMap<'a>,
    ) -> HyperImpl<'a> {
        HyperImpl::None
    }
    fn allow_methods(&self) -> &'static [Method] {
        &[]
    }
    fn about(&self) -> Option<&'static str> {
        None
    }
    fn version(&self) -> Option<&str> {
        None
    }
    fn author(&self) -> Option<&'static str> {
        None
    }
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn args(&self) -> &'static [&'static dyn Argument] {
        &[]
    }
    fn commands(&self) -> &'static [&'static dyn Api] {
        &[]
    }
}

pub trait Argument: Send + Sync {
    fn name(&self) -> &'static str;
    fn hyper_validation<'a>(
        &self,
        _request: &'a Parts,
        query: &'a QueryMap<'a>,
    ) -> Result<(), Response<Body>> {
        hyper_helpers::default_hyper_validation(self, query)
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
    fn default_value(&self) -> Option<&'static str> {
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
