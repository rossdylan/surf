use std::fmt;
use std::sync::Arc;

use crate::http::Method;
use crate::middleware::{Middleware, Next};
use crate::{HttpClient, Request, RequestBuilder, Response, Result};

use futures_util::future::BoxFuture;

#[cfg(all(feature = "native-client", not(feature = "h1-client")))]
use http_client::native::NativeClient;

#[cfg(feature = "h1-client")]
use http_client::h1::H1Client;

/// An HTTP client, capable of sending `Request`s and running a middleware stack.
///
/// # Examples
///
/// ```no_run
/// # #[async_std::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
/// let client = surf::Client::new();
/// let res1 = client.recv_string(surf::get("https://httpbin.org/get"));
/// let res2 = client.recv_string(surf::get("https://httpbin.org/get"));
/// let (str1, str2) = futures_util::future::try_join(res1, res2).await?;
/// # Ok(()) }
/// ```
pub struct Client {
    http_client: Arc<dyn HttpClient>,
    /// Holds the middleware stack.
    middleware: Arc<Vec<Arc<dyn Middleware>>>,
}

impl Clone for Client {
    /// Clones the Client.
    ///
    /// This copies the middleware stack from the original, but shares
    /// the `HttpClient` of the original.
    /// Note that individual middleware in the middleware stack are
    /// still shared by reference.
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            middleware: Arc::new(self.middleware.iter().cloned().collect()),
        }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Client {{}}")
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Create a new instance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::Client::new();
    /// # Ok(()) }
    /// ```
    pub fn new() -> Self {
        #[cfg(all(feature = "native-client", not(feature = "h1-client")))]
        let client = NativeClient::new();
        #[cfg(feature = "h1-client")]
        let client = H1Client::new();
        Self::with_http_client(Arc::new(client))
    }

    /// Create a new instance with an `http_client::HttpClient` instance.
    // TODO(yw): hidden from docs until we make the traits public.
    #[doc(hidden)]
    #[allow(missing_doc_code_examples)]
    pub fn with_http_client(http_client: Arc<dyn HttpClient>) -> Self {
        Self {
            http_client,
            middleware: Arc::new(vec![
                #[cfg(feature = "middleware-logger")]
                Arc::new(crate::middleware::Logger::new()),
            ]),
        }
    }

    /// Push middleware onto the middleware stack.
    ///
    /// See the [middleware] submodule for more information on middleware.
    ///
    /// [middleware]: ../middleware/index.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let req = surf::get("https://httpbin.org/get");
    /// let client = surf::client()
    ///     .with(surf::middleware::Redirect::default());
    /// let res = client.send(req).await?;
    /// # Ok(()) }
    /// ```
    pub fn with(mut self, middleware: impl Middleware) -> Self {
        let m = Arc::get_mut(&mut self.middleware)
            .expect("Registering middleware is not possible after the Client has been used");
        m.push(Arc::new(middleware));
        self
    }

    /// Send a Request using this client.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let req = surf::get("https://httpbin.org/get");
    /// let client = surf::client();
    /// let res = client.send(req).await?;
    /// # Ok(()) }
    /// ```
    pub fn send(&self, req: impl Into<Request>) -> BoxFuture<'static, Result<Response>> {
        let req: Request = req.into();
        let http_client = self.http_client.clone();
        let middleware = self.middleware.clone();
        Box::pin(async move {
            let next = Next::new(&middleware, &|req, client| {
                Box::pin(async move {
                    let req: http_types::Request = req.into();
                    client.http_client.send(req).await.map(Into::into)
                })
            });

            let res = next.run(req, Client::with_http_client(http_client)).await?;
            Ok(Response::new(res.into()))
        })
    }

    /// Submit the request and get the response body as bytes.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let req = surf::get("https://httpbin.org/get");
    /// let bytes = surf::client().recv_bytes(req).await?;
    /// assert!(bytes.len() > 0);
    /// # Ok(()) }
    /// ```
    pub async fn recv_bytes(&self, req: impl Into<Request>) -> Result<Vec<u8>> {
        let mut res = self.send(req.into()).await?;
        Ok(res.body_bytes().await?)
    }

    /// Submit the request and get the response body as a string.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let req = surf::get("https://httpbin.org/get");
    /// let string = surf::client().recv_string(req).await?;
    /// assert!(string.len() > 0);
    /// # Ok(()) }
    /// ```
    pub async fn recv_string(&self, req: impl Into<Request>) -> Result<String> {
        let mut res = self.send(req.into()).await?;
        Ok(res.body_string().await?)
    }

    /// Submit the request and decode the response body from json into a struct.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use serde::{Deserialize, Serialize};
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// #[derive(Deserialize, Serialize)]
    /// struct Ip {
    ///     ip: String
    /// }
    ///
    /// let req = surf::get("https://api.ipify.org?format=json");
    /// let Ip { ip } = surf::client().recv_json(req).await?;
    /// assert!(ip.len() > 10);
    /// # Ok(()) }
    /// ```
    pub async fn recv_json<T: serde::de::DeserializeOwned>(
        &self,
        req: impl Into<Request>,
    ) -> Result<T> {
        let mut res = self.send(req.into()).await?;
        Ok(res.body_json::<T>().await?)
    }

    /// Submit the request and decode the response body from form encoding into a struct.
    ///
    /// # Errors
    ///
    /// Any I/O error encountered while reading the body is immediately returned
    /// as an `Err`.
    ///
    /// If the body cannot be interpreted as valid json for the target type `T`,
    /// an `Err` is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use serde::{Deserialize, Serialize};
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// #[derive(Deserialize, Serialize)]
    /// struct Body {
    ///     apples: u32
    /// }
    ///
    /// let req = surf::get("https://api.example.com/v1/response");
    /// let Body { apples } = surf::client().recv_form(req).await?;
    /// # Ok(()) }
    /// ```
    pub async fn recv_form<T: serde::de::DeserializeOwned>(
        &self,
        req: impl Into<Request>,
    ) -> Result<T> {
        let mut res = self.send(req.into()).await?;
        Ok(res.body_form::<T>().await?)
    }

    /// Perform an HTTP `GET` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.get("https://httpbin.org/get").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn get(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Get, uri).with_client(self.clone())
    }

    /// Perform an HTTP `HEAD` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.head("https://httpbin.org/head").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn head(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Head, uri).with_client(self.clone())
    }

    /// Perform an HTTP `POST` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.post("https://httpbin.org/post").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn post(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Post, uri).with_client(self.clone())
    }

    /// Perform an HTTP `PUT` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.put("https://httpbin.org/put").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn put(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Put, uri).with_client(self.clone())
    }

    /// Perform an HTTP `DELETE` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.delete("https://httpbin.org/delete").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn delete(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Delete, uri).with_client(self.clone())
    }

    /// Perform an HTTP `CONNECT` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.connect("https://httpbin.org/connect").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn connect(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Connect, uri).with_client(self.clone())
    }

    /// Perform an HTTP `OPTIONS` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.options("https://httpbin.org/options").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn options(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Options, uri).with_client(self.clone())
    }

    /// Perform an HTTP `TRACE` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.trace("https://httpbin.org/trace").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn trace(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Trace, uri).with_client(self.clone())
    }

    /// Perform an HTTP `PATCH` request using the `Client` connection.
    ///
    /// # Panics
    ///
    /// This will panic if a malformed URL is passed.
    ///
    /// # Errors
    ///
    /// Returns errors from the middleware, http backend, and network sockets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    /// let client = surf::client();
    /// let string = client.patch("https://httpbin.org/patch").recv_string().await?;
    /// # Ok(()) }
    /// ```
    pub fn patch(&self, uri: impl AsRef<str>) -> RequestBuilder {
        let uri = uri.as_ref().parse().unwrap();
        RequestBuilder::new(Method::Patch, uri).with_client(self.clone())
    }
}
