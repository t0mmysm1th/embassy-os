use std::convert::Infallible;
use std::future::Future;

use futures::{future::BoxFuture, FutureExt};
use hyper::{body::HttpBody, http::request::Parts, Body, Request, Response};
use serde::{Deserialize, Serialize};

use super::{Api, Argument, QueryMap};
use crate::util::Apply;
use crate::{Error, ResultExt};

pub async fn create_service_fn<A: Api + Default>(
    request: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    let (request, mut body) = request.into_parts();
    let query: QueryMap = if let Some(qs) = request.uri.query() {
        match serde_qs::from_str(qs) {
            Ok(a) => a,
            Err(e) => return Ok(response::bad_request(format!("{}", e))),
        }
    } else {
        QueryMap::new()
    };
    let api = A::default();
    Ok(
        match handle_request(&api, Some(request.uri.path()), &request, &mut body, &query).await {
            Ok(res) => res,
            Err(e) => e.to_response(accepts_cbor(&request)),
        },
    )
}

async fn handle_request<'a, A: Api + ?Sized>(
    api: &'a A,
    path: Option<&'a str>,
    request: &'a Parts,
    body: &'a mut Body,
    query: &'a QueryMap<'a>,
) -> Result<Response<Body>, Error> {
    for arg in api.args() {
        if let Err(res) = arg.hyper_validation(request, query) {
            return Ok(res);
        }
    }

    if let Some(sub_action) = handle_subrequest(api, path, request, body, query) {
        return sub_action.await;
    }
    if let Some(action) = api.hyper_impl(request, query) {
        action(body).await
    } else {
        Ok(response::not_found())
    }
}

fn handle_subrequest<'a, A: Api + ?Sized>(
    api: &'a A,
    path: Option<&'a str>,
    request: &'a Parts,
    body: &'a mut Body,
    query: &'a QueryMap<'a>,
) -> Option<BoxFuture<'a, Result<Response<Body>, Error>>> {
    if let Some(path) = path {
        let mut path_iter = path.split('/');
        let cmd_str = path_iter.next().unwrap();
        let cmds = api.commands();
        if let Some(cmd) = cmds.iter().filter(|cmd| cmd.name() == cmd_str).next() {
            Some(handle_request(*cmd, path_iter.next(), request, body, query).boxed())
        } else {
            None
        }
    } else {
        None
    }
}

pub fn is_cbor(request: &Parts) -> bool {
    if let Some(content_type) = request
        .headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
    {
        content_type
            .split(";")
            .next()
            .unwrap()
            .apply(|t| t.trim())
            .apply(|t| t == "application/cbor")
    } else {
        false
    }
}

pub fn accepts_cbor(request: &Parts) -> bool {
    if let Some(accept) = request.headers.get("accept").and_then(|h| h.to_str().ok()) {
        accept
            .split(";")
            .next()
            .unwrap()
            .split(",")
            .map(|s| s.trim())
            .any(|t| t == "*/*" || t == "application/*" || t == "application/cbor")
    } else {
        is_cbor(request)
    }
}

pub fn serde_res<T: Serialize>(request: &Parts, val: &T) -> Result<Response<Body>, Error> {
    if accepts_cbor(request) {
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
    if is_cbor(request) {
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

pub fn default_hyper_validation<'a, A: Argument + ?Sized>(
    arg: &'a A,
    query: &'a QueryMap<'a>,
) -> Result<(), Response<Body>> {
    let val = query.get(arg.name());
    if arg.required() && val.is_none() {
        return Err(response::bad_request(format!("{}: required", arg.name())));
    }
    if arg.multiple() {
        if let Err(e) = val.map(|v| v.parse::<usize>()).unwrap_or(Ok(0)) {
            return Err(response::bad_request(format!(
                "{}: expected usize: {}",
                arg.name(),
                e,
            )));
        }
    }
    for other in arg.conflicts_with() {
        if query.contains_key(*other) {
            return Err(response::bad_request(format!(
                "{}: conflicts with: {}",
                arg.name(),
                other,
            )));
        }
    }
    if let Some(other) = arg.required_unless() {
        if val.is_none() && !query.contains_key(other) {
            return Err(response::bad_request(format!(
                "{}: required unless: {}",
                arg.name(),
                other,
            )));
        }
    }
    if let Some(other) = arg.requires() {
        if val.is_some() && !query.contains_key(other) {
            return Err(response::bad_request(format!(
                "{}: requires: {}",
                arg.name(),
                other,
            )));
        }
    }
    Ok(())
}

pub mod response {
    use std::borrow::Cow;

    use hyper::{Body, Response, StatusCode};

    pub fn bad_request<S: Into<Cow<'static, str>>>(msg: S) -> Response<Body> {
        let msg = msg.into();
        Response::builder()
            .header("content-type", "text/plain")
            .header("content-length", msg.len())
            .status(StatusCode::BAD_REQUEST)
            .body(msg.into())
            .unwrap()
    }

    pub fn not_found() -> Response<Body> {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap()
    }

    pub fn no_content() -> Response<Body> {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap()
    }
}
