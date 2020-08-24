#![allow(deprecated)]

#[cfg(any(
feature = "native-tls",
feature = "rustls-tls",
))]
use std::any::Any;
use std::convert::TryInto;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use http::header::HeaderValue;
use tokio::sync::oneshot;

use runtime::ClientRuntime;

use crate::{async_impl, header, Proxy, redirect};
#[cfg(feature = "__tls")]
use crate::{Certificate, Identity};

use super::executor;
use super::response::Response;
use crate::Request;

mod runtime;

/// A `Client` to make Requests with.
///
/// The Client has various configuration values to tweak, but the defaults
/// are set to what is usually the most commonly desired value. To configure a
/// `Client`, use `Client::builder()`.
///
/// The `Client` holds a connection pool internally, so it is advised that
/// you create one and **reuse** it.
///
/// # Examples
///
/// ```rust
/// # use reqwest::RequestBuilder;
/// #
/// # fn run() -> Result<(), reqwest::Error> {
/// let client = reqwest::blocking::Client::new();
/// let resp = RequestBuilder::get("http://httpbin.org/").send(&client)?;
/// #   drop(resp);
/// #   Ok(())
/// # }
///
/// ```
#[derive(Clone)]
pub struct Client {
	timeout: Timeout,
	client_runtime: Arc<ClientRuntime>,
}

/// A `ClientBuilder` can be used to create a `Client` with  custom configuration.
///
/// # Example
///
/// ```
/// # fn run() -> Result<(), reqwest::Error> {
/// use std::time::Duration;
///
/// let client = reqwest::blocking::Client::builder()
///     .timeout(Duration::from_secs(10))
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct ClientBuilder {
	inner: async_impl::ClientBuilder,
	timeout: Timeout,
}

impl Default for ClientBuilder {
	fn default() -> Self {
		Self::new()
	}
}

impl ClientBuilder {
	/// Constructs a new `ClientBuilder`.
	///
	/// This is the same as `Client::builder()`.
	pub fn new() -> ClientBuilder {
		ClientBuilder {
			inner: async_impl::ClientBuilder::new(),
			timeout: Timeout::default(),
		}
	}

	/// Returns a `Client` that uses this `ClientBuilder` configuration.
	///
	/// # Errors
	///
	/// This method fails if TLS backend cannot be initialized, or the resolver
	/// cannot load the system configuration.
	pub fn build(self) -> crate::Result<Client> {
		Client::from_builder(self)
	}


	// Higher-level options


	/// Sets the `User-Agent` header to be used by this client.
	///
	/// # Example
	///
	/// ```rust
	/// # use reqwest::RequestBuilder;
	/// # fn doc() -> Result<(), reqwest::Error> {
	/// // Name your user agent after your app?
	/// static APP_USER_AGENT: &str = concat!(
	///     env!("CARGO_PKG_NAME"),
	///     "/",
	///     env!("CARGO_PKG_VERSION"),
	/// );
	///
	/// let client = reqwest::blocking::Client::builder()
	///     .user_agent(APP_USER_AGENT)
	///     .build()?;
	/// let res = RequestBuilder::get("https://www.rust-lang.org").send(&client)?;
	/// # Ok(())
	/// # }
	/// ```
	pub fn user_agent<V>(self, value: V) -> ClientBuilder
		where
			V: TryInto<HeaderValue>,
			V::Error: Into<http::Error>,
	{
		self.with_inner(move |inner| inner.user_agent(value))
	}

	/// Sets the default headers for every request.
	///
	/// # Example
	///
	/// ```rust
	/// # use reqwest::RequestBuilder;
	/// use reqwest::header;
	/// # fn build_client() -> Result<(), reqwest::Error> {
	/// let mut headers = header::HeaderMap::new();
	/// headers.insert(header::AUTHORIZATION, header::HeaderValue::from_static("secret"));
	///
	/// // get a client builder
	/// let client = reqwest::blocking::Client::builder()
	///     .default_headers(headers)
	///     .build()?;
	/// let res = RequestBuilder::get("https://www.rust-lang.org").send(&client)?;
	/// # Ok(())
	/// # }
	/// ```
	///
	/// Override the default headers:
	///
	/// ```rust
	/// # use reqwest::RequestBuilder;
	/// use reqwest::header;
	/// # fn build_client() -> Result<(), reqwest::Error> {
	/// let mut headers = header::HeaderMap::new();
	/// headers.insert(header::AUTHORIZATION, header::HeaderValue::from_static("secret"));
	///
	/// // get a client builder
	/// let client = reqwest::blocking::Client::builder()
	///     .default_headers(headers)
	///     .build()?;
	/// let res = RequestBuilder::get("https://www.rust-lang.org")
	///     .header(header::AUTHORIZATION, "token")
	///     .send(&client)?;
	/// # Ok(())
	/// # }
	/// ```
	pub fn default_headers(self, headers: header::HeaderMap) -> ClientBuilder {
		self.with_inner(move |inner| inner.default_headers(headers))
	}

	/// Enable a persistent cookie store for the client.
	///
	/// Cookies received in responses will be preserved and included in
	/// additional requests.
	///
	/// By default, no cookie store is used.
	///
	/// # Optional
	///
	/// This requires the optional `cookies` feature to be enabled.
	#[cfg(feature = "cookies")]
	pub fn cookie_store(self, enable: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.cookie_store(enable))
	}

	/// Enable auto gzip decompression by checking the `Content-Encoding` response header.
	///
	/// If auto gzip decompresson is turned on:
	///
	/// - When sending a request and if the request's headers do not already contain
	///   an `Accept-Encoding` **and** `Range` values, the `Accept-Encoding` header is set to `gzip`.
	///   The request body is **not** automatically compressed.
	/// - When receiving a response, if it's headers contain a `Content-Encoding` value that
	///   equals to `gzip`, both values `Content-Encoding` and `Content-Length` are removed from the
	///   headers' set. The response body is automatically decompressed.
	///
	/// If the `gzip` feature is turned on, the default option is enabled.
	///
	/// # Optional
	///
	/// This requires the optional `gzip` feature to be enabled
	#[cfg(feature = "gzip")]
	pub fn gzip(self, enable: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.gzip(enable))
	}

	/// Disable auto response body gzip decompression.
	///
	/// This method exists even if the optional `gzip` feature is not enabled.
	/// This can be used to ensure a `Client` doesn't use gzip decompression
	/// even if another dependency were to enable the optional `gzip` feature.
	pub fn no_gzip(self) -> ClientBuilder {
		self.with_inner(|inner| inner.no_gzip())
	}

	// Redirect options

	/// Set a `redirect::Policy` for this client.
	///
	/// Default will follow redirects up to a maximum of 10.
	pub fn redirect(self, policy: redirect::Policy) -> ClientBuilder {
		self.with_inner(move |inner| inner.redirect(policy))
	}

	/// Enable or disable automatic setting of the `Referer` header.
	///
	/// Default is `true`.
	pub fn referer(self, enable: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.referer(enable))
	}

	// Proxy options

	/// Add a `Proxy` to the list of proxies the `Client` will use.
	///
	/// # Note
	///
	/// Adding a proxy will disable the automatic usage of the "system" proxy.
	pub fn proxy(self, proxy: Proxy) -> ClientBuilder {
		self.with_inner(move |inner| inner.proxy(proxy))
	}

	/// Clear all `Proxies`, so `Client` will use no proxy anymore.
	///
	/// This also disables the automatic usage of the "system" proxy.
	pub fn no_proxy(self) -> ClientBuilder {
		self.with_inner(move |inner| inner.no_proxy())
	}

	#[doc(hidden)]
	#[deprecated(note = "the system proxy is used automatically")]
	pub fn use_sys_proxy(self) -> ClientBuilder {
		self
	}

	// Timeout options

	/// Set a timeout for connect, read and write operations of a `Client`.
	///
	/// Default is 30 seconds.
	///
	/// Pass `None` to disable timeout.
	pub fn timeout<T>(mut self, timeout: T) -> ClientBuilder
		where
			T: Into<Option<Duration>>,
	{
		self.timeout = Timeout(timeout.into());
		self
	}

	/// Set a timeout for only the connect phase of a `Client`.
	///
	/// Default is `None`.
	pub fn connect_timeout<T>(self, timeout: T) -> ClientBuilder
		where
			T: Into<Option<Duration>>,
	{
		let timeout = timeout.into();
		if let Some(dur) = timeout {
			self.with_inner(|inner| inner.connect_timeout(dur))
		} else {
			self
		}
	}

	/// Set whether connections should emit verbose logs.
	///
	/// Enabling this option will emit [log][] messages at the `TRACE` level
	/// for read and write operations on connections.
	///
	/// [log]: https://crates.io/crates/log
	pub fn connection_verbose(self, verbose: bool) -> ClientBuilder {
		self.with_inner(move |inner| inner.connection_verbose(verbose))
	}

	// HTTP options

	/// Set an optional timeout for idle sockets being kept-alive.
	///
	/// Pass `None` to disable timeout.
	///
	/// Default is 90 seconds.
	pub fn pool_idle_timeout<D>(self, val: D) -> ClientBuilder
		where
			D: Into<Option<Duration>>,
	{
		self.with_inner(|inner| inner.pool_idle_timeout(val))
	}

	/// Sets the maximum idle connection per host allowed in the pool.
	pub fn pool_max_idle_per_host(self, max: usize) -> ClientBuilder {
		self.with_inner(move |inner| inner.pool_max_idle_per_host(max))
	}

	#[doc(hidden)]
	#[deprecated(note = "use pool_max_idle_per_host instead")]
	pub fn max_idle_per_host(self, max: usize) -> ClientBuilder {
		self.pool_max_idle_per_host(max)
	}

	/// Enable case sensitive headers.
	pub fn http1_title_case_headers(self) -> ClientBuilder {
		self.with_inner(|inner| inner.http1_title_case_headers())
	}

	/// Only use HTTP/2.
	pub fn http2_prior_knowledge(self) -> ClientBuilder {
		self.with_inner(|inner| inner.http2_prior_knowledge())
	}

	/// Sets the `SETTINGS_INITIAL_WINDOW_SIZE` option for HTTP2 stream-level flow control.
	///
	/// Default is currently 65,535 but may change internally to optimize for common uses.
	pub fn http2_initial_stream_window_size(self, sz: impl Into<Option<u32>>) -> ClientBuilder {
		self.with_inner(|inner| inner.http2_initial_stream_window_size(sz))
	}

	/// Sets the max connection-level flow control for HTTP2
	///
	/// Default is currently 65,535 but may change internally to optimize for common uses.
	pub fn http2_initial_connection_window_size(self, sz: impl Into<Option<u32>>) -> ClientBuilder {
		self.with_inner(|inner| inner.http2_initial_connection_window_size(sz))
	}

	// TCP options

	#[doc(hidden)]
	#[deprecated(note = "tcp_nodelay is enabled by default, use `tcp_nodelay_` to disable")]
	pub fn tcp_nodelay(self) -> ClientBuilder {
		self.tcp_nodelay_(true)
	}

	/// Set whether sockets have `SO_NODELAY` enabled.
	///
	/// Default is `true`.
	// NOTE: Regarding naming (trailing underscore):
	//
	// Due to the original `tcp_nodelay()` not taking an argument, changing
	// the default means a user has no way of *disabling* this feature.
	//
	// TODO(v0.11.x): Remove trailing underscore.
	pub fn tcp_nodelay_(self, enabled: bool) -> ClientBuilder {
		self.with_inner(move |inner| inner.tcp_nodelay_(enabled))
	}

	/// Bind to a local IP Address.
	///
	/// # Example
	///
	/// ```
	/// use std::net::IpAddr;
	/// let local_addr = IpAddr::from([12, 4, 1, 8]);
	/// let client = reqwest::blocking::Client::builder()
	///     .local_address(local_addr)
	///     .build().unwrap();
	/// ```
	pub fn local_address<T>(self, addr: T) -> ClientBuilder
		where
			T: Into<Option<IpAddr>>,
	{
		self.with_inner(move |inner| inner.local_address(addr))
	}

	// TLS options

	/// Add a custom root certificate.
	///
	/// This allows connecting to a server that has a self-signed
	/// certificate for example. This **does not** replace the existing
	/// trusted store.
	///
	/// # Example
	///
	/// ```
	/// # use std::fs::File;
	/// # use std::io::Read;
	/// # fn build_client() -> Result<(), Box<dyn std::error::Error>> {
	/// // read a local binary DER encoded certificate
	/// let der = std::fs::read("my-cert.der")?;
	///
	/// // create a certificate
	/// let cert = reqwest::Certificate::from_der(&der)?;
	///
	/// // get a client builder
	/// let client = reqwest::blocking::Client::builder()
	///     .add_root_certificate(cert)
	///     .build()?;
	/// # drop(client);
	/// # Ok(())
	/// # }
	/// ```
	///
	/// # Optional
	///
	/// This requires the optional `default-tls`, `native-tls`, or `rustls-tls`
	/// feature to be enabled.
	#[cfg(feature = "__tls")]
	pub fn add_root_certificate(self, cert: Certificate) -> ClientBuilder {
		self.with_inner(move |inner| inner.add_root_certificate(cert))
	}

	/// Sets the identity to be used for client certificate authentication.
	#[cfg(feature = "__tls")]
	pub fn identity(self, identity: Identity) -> ClientBuilder {
		self.with_inner(move |inner| inner.identity(identity))
	}

	/// Controls the use of hostname verification.
	///
	/// Defaults to `false`.
	///
	/// # Warning
	///
	/// You should think very carefully before you use this method. If
	/// hostname verification is not used, any valid certificate for any
	/// site will be trusted for use from any other. This introduces a
	/// significant vulnerability to man-in-the-middle attacks.
	///
	/// # Optional
	///
	/// This requires the optional `native-tls` feature to be enabled.
	#[cfg(feature = "native-tls")]
	pub fn danger_accept_invalid_hostnames(self, accept_invalid_hostname: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.danger_accept_invalid_hostnames(accept_invalid_hostname))
	}

	/// Controls the use of certificate validation.
	///
	/// Defaults to `false`.
	///
	/// # Warning
	///
	/// You should think very carefully before using this method. If
	/// invalid certificates are trusted, *any* certificate for *any* site
	/// will be trusted for use. This includes expired certificates. This
	/// introduces significant vulnerabilities, and should only be used
	/// as a last resort.
	#[cfg(feature = "__tls")]
	pub fn danger_accept_invalid_certs(self, accept_invalid_certs: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.danger_accept_invalid_certs(accept_invalid_certs))
	}

	/// Force using the native TLS backend.
	///
	/// Since multiple TLS backends can be optionally enabled, this option will
	/// force the `native-tls` backend to be used for this `Client`.
	///
	/// # Optional
	///
	/// This requires the optional `native-tls` feature to be enabled.
	#[cfg(feature = "native-tls")]
	pub fn use_native_tls(self) -> ClientBuilder {
		self.with_inner(move |inner| inner.use_native_tls())
	}

	/// Force using the Rustls TLS backend.
	///
	/// Since multiple TLS backends can be optionally enabled, this option will
	/// force the `rustls` backend to be used for this `Client`.
	///
	/// # Optional
	///
	/// This requires the optional `rustls-tls` feature to be enabled.
	#[cfg(feature = "rustls-tls")]
	pub fn use_rustls_tls(self) -> ClientBuilder {
		self.with_inner(move |inner| inner.use_rustls_tls())
	}

	/// Use a preconfigured TLS backend.
	///
	/// If the passed `Any` argument is not a TLS backend that reqwest
	/// understands, the `ClientBuilder` will error when calling `build`.
	///
	/// # Advanced
	///
	/// This is an advanced option, and can be somewhat brittle. Usage requires
	/// keeping the preconfigured TLS argument version in sync with reqwest,
	/// since version mismatches will result in an "unknown" TLS backend.
	///
	/// If possible, it's preferable to use the methods on `ClientBuilder`
	/// to configure reqwest's TLS.
	///
	/// # Optional
	///
	/// This requires one of the optional features `native-tls` or
	/// `rustls-tls` to be enabled.
	#[cfg(any(
	feature = "native-tls",
	feature = "rustls-tls",
	))]
	pub fn use_preconfigured_tls(self, tls: impl Any) -> ClientBuilder {
		self.with_inner(move |inner| inner.use_preconfigured_tls(tls))
	}

	/// Enables the [trust-dns](trust_dns_resolver) async resolver instead of a default threadpool using `getaddrinfo`.
	///
	/// If the `trust-dns` feature is turned on, the default option is enabled.
	///
	/// # Optional
	///
	/// This requires the optional `trust-dns` feature to be enabled
	#[cfg(feature = "trust-dns")]
	pub fn trust_dns(self, enable: bool) -> ClientBuilder {
		self.with_inner(|inner| inner.trust_dns(enable))
	}

	/// Disables the trust-dns async resolver.
	///
	/// This method exists even if the optional `trust-dns` feature is not enabled.
	/// This can be used to ensure a `Client` doesn't use the trust-dns async resolver
	/// even if another dependency were to enable the optional `trust-dns` feature.
	pub fn no_trust_dns(self) -> ClientBuilder {
		self.with_inner(|inner| inner.no_trust_dns())
	}

	// private

	fn with_inner<F>(mut self, func: F) -> ClientBuilder
		where
			F: FnOnce(async_impl::ClientBuilder) -> async_impl::ClientBuilder,
	{
		self.inner = func(self.inner);
		self
	}
}

impl From<async_impl::ClientBuilder> for ClientBuilder {
	fn from(builder: async_impl::ClientBuilder) -> Self {
		Self {
			inner: builder,
			timeout: Timeout::default(),
		}
	}
}

impl Default for Client {
	fn default() -> Self {
		Self::new()
	}
}

impl Client {
	fn from_builder(builder: ClientBuilder) -> crate::Result<Client> {
		Ok(Client {
			timeout: builder.timeout,
			client_runtime: ClientRuntime::new(builder.inner.build()?).map(Arc::new)?,
		})
	}

	/// Constructs a new `Client`.
	///
	/// # Panic
	///
	/// This method panics if TLS backend cannot initialized, or the resolver
	/// cannot load the system configuration.
	///
	/// Use `Client::builder()` if you wish to handle the failure as an `Error`
	/// instead of panicking.
	pub fn new() -> Client {
		ClientBuilder::new().build().expect("Client::new()")
	}

	/// Creates a `ClientBuilder` to configure a `Client`.
	///
	/// This is the same as `ClientBuilder::new()`.
	pub fn builder() -> ClientBuilder {
		ClientBuilder::new()
	}

	/// Executes a `Request`.
	///
	/// A `Request` can be built manually with `Request::new()` or obtained
	/// from a RequestBuilder with `RequestBuilder::build()`.
	///
	/// You should prefer to use the `RequestBuilder` and
	/// `RequestBuilder::send()`.
	///
	/// # Errors
	///
	/// This method fails if there was an error while sending request,
	/// or redirect limit was exhausted.
	pub fn send(&self, request: Request) -> crate::Result<Response> {
		let (tx, rx) = oneshot::channel();
		let url = request.url().clone();
		let timeout = request.timeout().copied().or(self.timeout.0);

		self.client_runtime
			.task_queue_sender
			.as_ref()
			.expect("core thread exited early")
			.send((request, tx))
			.expect("core thread panicked");

		let send_future = async move {
			rx.await.map_err(|_canceled| event_loop_panicked()).unwrap()
		};

		executor::execute_blocking_flatten_timeout(
			send_future,
			timeout,
			|timeout_err| crate::error::request(timeout_err).with_url(url.clone())
		)
			.map(|response| Response::new(
				response,
				self.timeout.0,
				KeepCoreThreadAlive(Some(self.client_runtime.clone())),
			))
			.map_err(|response_error| response_error.with_url(url.clone()))
	}
}

impl crate::core::Client for Client {
	type Response = crate::Result<Response>;

	fn send(&self, request: crate::Result<Request>) -> Self::Response {
		self.send(request?)
	}
}

impl fmt::Debug for Client {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("Client").finish()
	}
}

impl fmt::Debug for ClientBuilder {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.inner.fmt(f)
	}
}

#[derive(Clone, Copy)]
struct Timeout(Option<Duration>);

impl Default for Timeout {
	fn default() -> Timeout {
		// default mentioned in ClientBuilder::timeout() doc comment
		Timeout(Some(Duration::from_secs(30)))
	}
}

pub(crate) struct KeepCoreThreadAlive(Option<Arc<ClientRuntime>>);

impl KeepCoreThreadAlive {
	pub(crate) fn empty() -> KeepCoreThreadAlive {
		KeepCoreThreadAlive(None)
	}
}

#[cold]
#[inline(never)]
fn event_loop_panicked() -> ! {
	// The only possible reason there would be a Canceled error
	// is if the thread running the event loop panicked. We could return
	// an Err here, like a BrokenPipe, but the Client is not
	// recoverable. Additionally, the panic in the other thread
	// is not normal, and should likely be propagated.
	panic!("event loop thread panicked");
}
