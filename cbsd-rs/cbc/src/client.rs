// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use reqwest::Method;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::error::Error;

/// Client-construction flags, bundled so the CLI threads a single value
/// through every command rather than a growing list of positional `bool`s
/// (which are trivially transposable at call sites).
///
/// Every field is a plain `bool` and carries no secret material — the bearer
/// token is passed separately to [`CbcClient::new`] as a `&SecretString`, so
/// deriving `Debug` here cannot leak credentials.
#[derive(Clone, Copy, Debug)]
pub struct ClientOpts {
    /// Print HTTP requests/responses to stderr.
    pub debug: bool,
    /// Disable TLS certificate verification (development with self-signed
    /// certificates). Orthogonal to `insecure_http`: this only relaxes cert
    /// checking for `https`, it does not permit the `http` scheme.
    pub no_tls_verify: bool,
    /// Permit plain `http://` hosts. Without this, non-`https` hosts are
    /// rejected at URL-parse time. Bearer tokens are sent in cleartext when
    /// this is set.
    pub insecure_http: bool,
}

impl ClientOpts {
    /// Operator-facing warnings implied by these options. Returned rather than
    /// printed so the decision is unit-testable; the CLI entry point emits
    /// each once per invocation.
    pub fn warnings(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.no_tls_verify {
            out.push("warning: TLS certificate verification is disabled");
        }
        if self.insecure_http {
            out.push("warning: --insecure-http is set; bearer tokens are sent in cleartext");
        }
        out
    }
}

pub struct CbcClient {
    inner: reqwest::Client,
    base_url: Url,
    debug: bool,
}

impl CbcClient {
    /// Create an authenticated client.
    pub fn new(host: &str, token: &SecretString, opts: ClientOpts) -> Result<Self, Error> {
        let base_url = parse_base_url(host, opts.insecure_http)?;

        let mut headers = HeaderMap::new();
        let auth_value = format!("Bearer {}", token.expose_secret());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value)
                .map_err(|e| Error::Config(format!("invalid token: {e}")))?,
        );

        let inner = reqwest::Client::builder()
            .danger_accept_invalid_certs(opts.no_tls_verify)
            .default_headers(headers)
            .build()
            .map_err(|e| Error::Connection(format!("cannot build HTTP client: {e}")))?;

        Ok(Self {
            inner,
            base_url,
            debug: opts.debug,
        })
    }

    /// Create an unauthenticated client (for pre-login health checks).
    pub fn unauthenticated(host: &str, opts: ClientOpts) -> Result<Self, Error> {
        let base_url = parse_base_url(host, opts.insecure_http)?;

        let inner = reqwest::Client::builder()
            .danger_accept_invalid_certs(opts.no_tls_verify)
            .build()
            .map_err(|e| Error::Connection(format!("cannot build HTTP client: {e}")))?;

        Ok(Self {
            inner,
            base_url,
            debug: opts.debug,
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.request::<T>(Method::GET, path, Option::<&()>::None)
            .await
    }

    /// Return a raw `RequestBuilder` for the given method and API path.
    ///
    /// Useful when the caller needs to customise the request (e.g. SSE
    /// streaming) instead of going through the generic JSON helpers.
    pub fn request_builder(
        &self,
        method: Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| Error::Connection(format!("invalid path '{path}': {e}")))?;

        if self.debug {
            eprintln!("{method} {url}");
        }

        Ok(self.inner.request(method, url))
    }

    /// Send a GET request and return the raw response for streaming.
    pub async fn get_stream(&self, path: &str) -> Result<reqwest::Response, Error> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| Error::Connection(format!("invalid path '{path}': {e}")))?;

        if self.debug {
            eprintln!("GET {url}");
        }

        let resp = self
            .inner
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Connection(e.to_string()))?;

        let status = resp.status();

        if self.debug {
            eprintln!("  -> {status}");
        }

        if status.is_success() {
            Ok(resp)
        } else {
            let status_code = status.as_u16();
            let text = resp.text().await.unwrap_or_default();

            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("detail")?.as_str().map(String::from))
                .unwrap_or(text);

            Err(Error::Api {
                status: status_code,
                message,
            })
        }
    }

    pub async fn post<B: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Error> {
        self.request::<T>(Method::POST, path, Some(body)).await
    }

    pub async fn put_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &(impl Serialize + Sync),
    ) -> Result<T, Error> {
        self.request::<T>(Method::PUT, path, Some(body)).await
    }

    pub async fn put_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.request::<T>(Method::PUT, path, Option::<&()>::None)
            .await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        self.request::<T>(Method::DELETE, path, Option::<&()>::None)
            .await
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + Sync)>,
    ) -> Result<T, Error> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| Error::Connection(format!("invalid path '{path}': {e}")))?;

        if self.debug {
            eprintln!("{method} {url}");
        }

        let mut req = self.inner.request(method.clone(), url);
        if let Some(b) = body {
            req = req.json(b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| Error::Connection(e.to_string()))?;

        let status = resp.status();

        if self.debug {
            eprintln!("  -> {status}");
        }

        if status.is_success() {
            resp.json::<T>()
                .await
                .map_err(|e| Error::Other(format!("cannot decode response: {e}")))
        } else {
            let status_code = status.as_u16();
            let text = resp.text().await.unwrap_or_default();

            // Try to extract a structured error message.
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("detail")?.as_str().map(String::from))
                .unwrap_or(text);

            Err(Error::Api {
                status: status_code,
                message,
            })
        }
    }
}

/// Validate the host URL's scheme and ensure its path ends with `/api/`.
///
/// `https` is always accepted. `http` is accepted only when `insecure_http`
/// is set — bearer tokens then travel in cleartext. Any other scheme, and
/// `http` without the opt-in, is rejected.
fn parse_base_url(host: &str, insecure_http: bool) -> Result<Url, Error> {
    let mut s = host.to_string();
    if !s.ends_with('/') {
        s.push('/');
    }
    let mut url =
        Url::parse(&s).map_err(|e| Error::Config(format!("invalid host URL '{host}': {e}")))?;

    let scheme = url.scheme();
    let allowed = scheme == "https" || (scheme == "http" && insecure_http);
    if !allowed {
        return Err(Error::Config(format!("host must be https; got: {scheme}")));
    }

    // Append "api/" to the path so all relative joins resolve under /api/.
    url.set_path(&format!("{}api/", url.path()));
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_host_is_accepted() {
        let url = parse_base_url("https://cbs.example", false).expect("https accepted");
        assert_eq!(url.scheme(), "https");
        assert!(url.path().ends_with("/api/"));
    }

    #[test]
    fn http_host_is_rejected_without_opt_in() {
        let err =
            parse_base_url("http://cbs.example", false).expect_err("http rejected without opt-in");
        match err {
            Error::Config(msg) => assert_eq!(msg, "host must be https; got: http"),
            other => panic!("expected Error::Config, got {other:?}"),
        }
    }

    #[test]
    fn http_host_is_accepted_with_opt_in() {
        let url = parse_base_url("http://cbs.example", true).expect("opt-in permits http");
        assert_eq!(url.scheme(), "http");
        assert!(url.path().ends_with("/api/"));
    }

    #[test]
    fn other_schemes_are_rejected_even_with_opt_in() {
        let err = parse_base_url("ftp://cbs.example", true)
            .expect_err("opt-in widens only to http, not arbitrary schemes");
        match err {
            Error::Config(msg) => assert_eq!(msg, "host must be https; got: ftp"),
            other => panic!("expected Error::Config, got {other:?}"),
        }
    }

    #[test]
    fn warnings_reflect_set_flags() {
        let none = ClientOpts {
            debug: false,
            no_tls_verify: false,
            insecure_http: false,
        };
        assert!(none.warnings().is_empty());

        let tls = ClientOpts {
            debug: false,
            no_tls_verify: true,
            insecure_http: false,
        };
        assert_eq!(
            tls.warnings(),
            vec!["warning: TLS certificate verification is disabled"]
        );

        let http = ClientOpts {
            debug: false,
            no_tls_verify: false,
            insecure_http: true,
        };
        assert_eq!(
            http.warnings(),
            vec!["warning: --insecure-http is set; bearer tokens are sent in cleartext"]
        );

        let both = ClientOpts {
            debug: true,
            no_tls_verify: true,
            insecure_http: true,
        };
        assert_eq!(both.warnings().len(), 2);
    }
}
