use std::borrow::Cow;

use clap::ArgMatches;
use futures::{future::BoxFuture, FutureExt};
use hyper::Method;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use tokio_compat_02::FutureExt as _;

use super::{Api, Argument};
use crate::{Error, ResultExt};

const QS_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b' ')
    .remove(b'*')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_');

pub async fn run_cli<A: Api>(api: &A) {
    let matches = create_app(api).clone().get_matches();

    let mut full_command = Vec::new();
    match handle_command(api, &mut full_command, &matches).await {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e.message);
            log::warn!("{:?}", e.message);
            std::process::exit(e.code);
        }
    }
}

fn create_app<A: Api + ?Sized>(api: &A) -> clap::App {
    use clap::App;

    let mut app = App::new(api.name());

    if let Some(about) = api.about() {
        app = App::about(app, about);
    }
    if let Some(version) = api.version() {
        app = App::version(app, version);
    }
    if let Some(author) = api.author() {
        app = App::author(app, author);
    }
    for arg in api.args() {
        app = App::arg(app, create_arg(*arg));
    }
    for cmd in api.commands() {
        app = App::subcommand(app, create_app(*cmd));
    }
    for alias in api.aliases() {
        app = App::alias(app, *alias);
    }

    app
}

fn create_arg<A: Argument + ?Sized>(arg: &A) -> clap::Arg<'static, 'static> {
    use clap::Arg;

    let mut clap_arg = Arg::with_name(arg.name());
    if let Some(short) = arg.short() {
        clap_arg = Arg::short(clap_arg, short);
    }
    if let Some(long) = arg.long() {
        clap_arg = Arg::long(clap_arg, long);
    }
    if let Some(default) = arg.default_value() {
        clap_arg = Arg::default_value(clap_arg, default);
    }
    if arg.required() {
        clap_arg = Arg::required(clap_arg, true);
    }
    if arg.takes_value() {
        clap_arg = Arg::takes_value(clap_arg, true);
    }
    if arg.multiple() {
        clap_arg = Arg::multiple(clap_arg, true);
    }
    let conflicts_with = arg.conflicts_with();
    if !conflicts_with.is_empty() {
        clap_arg = Arg::conflicts_with_all(clap_arg, conflicts_with);
    }
    if let Some(required_unless) = arg.required_unless() {
        clap_arg = Arg::required_unless(clap_arg, required_unless);
    }
    if let Some(requires) = arg.requires() {
        clap_arg = Arg::requires(clap_arg, requires);
    }

    clap_arg
}

async fn handle_command<'a, A: Api + ?Sized>(
    api: &'a A,
    full_command: &'a mut Vec<&'a dyn Api>,
    matches: &'a ArgMatches<'a>,
) -> Result<(), Error> {
    fn handle_command_rec<'a>(
        api: &'a dyn Api,
        full_command: &'a mut Vec<&'a dyn Api>,
        matches: &'a ArgMatches<'a>,
    ) -> BoxFuture<'a, Result<(), Error>> {
        full_command.push(&*api);
        handle_command(api, full_command, matches).boxed()
    }
    async fn handle_command_base<'a, A: Api + ?Sized>(
        api: &'a A,
        full_command: &'a [&'a dyn Api],
        matches: &'a ArgMatches<'a>,
    ) -> Result<(), Error> {
        if let Some(action) = api.clap_impl(&*full_command, matches) {
            action.await
        } else {
            eprintln!("{}", matches.usage());
            Err(Error::new(
                anyhow!("unrecognized command"),
                crate::error::UNRECOGNIZED_COMMAND,
            ))
        }
    }
    let cmds = api.commands();
    match matches.subcommand() {
        (command, Some(sub_m)) => {
            if let Some(sub_cmd) = cmds.iter().filter(|c| c.name() == command).next() {
                if let Some(pre_hook) = api.clap_pre_hook(matches) {
                    pre_hook.await?;
                }
                handle_command_rec(*sub_cmd, full_command, sub_m).await
            } else {
                handle_command_base(api, &*full_command, matches).await
            }
        }
        (_, None) => handle_command_base(api, &*full_command, matches).await,
    }
}

pub async fn forward_to_hyper_impl<
    'a,
    A: Api + ?Sized,
    B: Serialize,
    T: for<'de> Deserialize<'de>,
>(
    api: &A,
    full_command: &[&dyn Api],
    method: Method,
    matches: &'a ArgMatches<'a>,
    body: Option<&B>,
) -> Result<T, Error> {
    use hyper::StatusCode;
    use std::fmt::Write;

    let mut url = format!("http://localhost:{}", crate::PORT);
    for cmd in full_command {
        write!(&mut url, "/{}", cmd.name()).unwrap();
    }
    let mut delim = std::iter::once("?").chain(std::iter::repeat("&"));
    for arg in api.args() {
        let name: Cow<str> =
            percent_encoding::utf8_percent_encode(arg.name(), QS_ENCODE_SET).into();
        if arg.takes_value() && arg.multiple() {
            if let Some(values) = matches.values_of(arg.name()) {
                for (idx, value) in values.enumerate() {
                    write!(
                        &mut url,
                        "{}{}[{}]={}",
                        delim.next().unwrap(),
                        name,
                        idx,
                        Cow::from(percent_encoding::utf8_percent_encode(value, QS_ENCODE_SET))
                    )
                    .unwrap();
                }
            }
        } else if arg.takes_value() {
            if let Some(value) = matches.value_of(arg.name()) {
                write!(
                    &mut url,
                    "{}{}={}",
                    delim.next().unwrap(),
                    name,
                    Cow::from(percent_encoding::utf8_percent_encode(value, QS_ENCODE_SET))
                )
                .unwrap();
            }
        } else if arg.multiple() {
            write!(
                &mut url,
                "{}{}={}",
                delim.next().unwrap(),
                name,
                matches.occurrences_of(arg.name()),
            )
            .unwrap();
        } else {
            write!(
                &mut url,
                "{}{}={}",
                delim.next().unwrap(),
                name,
                matches.is_present(arg.name()),
            )
            .unwrap();
        }
    }

    let body = body
        .map(|b| serde_cbor::to_vec(b))
        .transpose()
        .with_code(crate::error::SERDE_ERROR)?
        .unwrap_or_default();

    let response = reqwest::Client::new()
        .request(method, &url)
        .header("content-type", "application/cbor")
        .header("content-length", body.len())
        .body(body)
        .send()
        .compat()
        .await
        .with_code(crate::error::NETWORK_ERROR)?;

    if response.status() == StatusCode::NO_CONTENT {
        serde_json::from_value(serde_json::Value::Null).with_code(crate::error::SERDE_ERROR)
    } else if response.status().is_success() {
        serde_cbor::from_slice(
            &*response
                .bytes()
                .await
                .with_code(crate::error::NETWORK_ERROR)?,
        )
        .with_code(crate::error::SERDE_ERROR)
    } else {
        Err(serde_cbor::from_slice(
            &*response
                .bytes()
                .await
                .with_code(crate::error::NETWORK_ERROR)?,
        )
        .with_code(crate::error::SERDE_ERROR)?)
    }
}
