//! Wasm asset reader that fetches assets over HTTP.

// `SendFuture` below is the only unsafe code here: it asserts `Send` for the single-threaded wasm
// futures that the `AssetReader` trait requires to be `Send`.
#![expect(unsafe_code, reason = "`SendFuture` asserts `Send` for wasm futures")]

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use js_sys::{JSON, Uint8Array};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

use crate::io::{AssetReader, AssetReaderError, Reader, VecReader};
use crate::utils::append_meta_extension;
use crate::utils::{EmptyPathStream, PathStream};

/// Represents the global object in the JavaScript context
#[wasm_bindgen]
extern "C" {
    /// The [Global](https://developer.mozilla.org/en-US/docs/Glossary/Global_object) object.
    type Global;

    /// The [window](https://developer.mozilla.org/en-US/docs/Web/API/Window) global object.
    #[wasm_bindgen(method, getter, js_name = Window)]
    fn window(this: &Global) -> JsValue;

    /// The [WorkerGlobalScope](https://developer.mozilla.org/en-US/docs/Web/API/WorkerGlobalScope) global object.
    #[wasm_bindgen(method, getter, js_name = WorkerGlobalScope)]
    fn worker(this: &Global) -> JsValue;
}

// -----------------------------------------------------------------------------
// SendFuture

/// A future that is always `Send`.
///
/// [`AssetReader`] requires the futures it is handed to be `Send`, but the wasm types are not:
/// `JsFuture` holds an `Rc<RefCell<..>>`, and `JsValue` is single-threaded as well. wasm runs on one
/// thread and these futures never leave it, so the bound is asserted here instead of rewriting every
/// reader around it: each future that faces JS is wrapped in this container where it is awaited, and
/// what comes out of it (a response, bytes) is handled by the caller.
#[repr(transparent)]
pub(crate) struct SendFuture<F>(pub F);

// SAFETY: the wrapped future is only ever driven on the one thread wasm has. It is not `Sync`
// either, so nothing can share it with another thread behind our back.
unsafe impl<F> Send for SendFuture<F> {}

impl<F: Future> Future for SendFuture<F> {
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: this is a plain wrapper, so the projection pins exactly the field it holds and
        // moves nothing: the field stays pinned for as long as the wrapper does.
        let this = unsafe { self.map_unchecked_mut(|this| &mut this.0) };
        this.poll(cx)
    }
}

// -----------------------------------------------------------------------------
// HttpWasmAssetReader

/// Reader implementation for loading assets via HTTP in Wasm.
pub struct HttpWasmAssetReader {
    root_path: PathBuf,
    request_mapper: Option<Box<dyn Fn(&str) -> Cow<'_, str> + Send + Sync + 'static>>,
}

impl HttpWasmAssetReader {
    /// Creates a new `HttpWasmAssetReader`. The path provided will be used to build URLs to query for assets.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            root_path: path.as_ref().to_path_buf(),
            request_mapper: None,
        }
    }

    /// Sets a mapper function to modify the request URL for each asset fetch.
    pub fn with_request_mapper<F>(mut self, mapper: F) -> Self
    where
        F: Fn(&str) -> Cow<'_, str> + Send + Sync + 'static,
    {
        self.request_mapper = Some(Box::new(mapper));
        self
    }
}

fn js_value_to_err(context: &str) -> impl FnOnce(JsValue) -> std::io::Error + '_ {
    move |value| {
        let message = match JSON::stringify(&value) {
            Ok(js_str) => format!("Failed to {context}: {js_str}"),
            Err(_) => {
                format!("Failed to {context} and also failed to stringify the JSValue of the error")
            }
        };

        std::io::Error::other(message)
    }
}

impl HttpWasmAssetReader {
    /// Applies the request mapper to `path`, yielding the URL to request.
    fn map_request<'a>(&'a self, path: &'a str) -> Cow<'a, str> {
        self.request_mapper
            .as_ref()
            .map_or_else(|| Cow::Borrowed(path), |mapper| mapper(path))
    }

    /// Starts the `fetch` for `fetch_path` and returns its response.
    ///
    /// The request is always a plain `GET`: `fetch_with_str` cannot carry another method, and
    /// nothing here needs one.
    async fn fetch_response(fetch_path: &str) -> Result<Response, AssetReaderError> {
        // The JS global scope includes a self-reference via a specializing name, which can be used to determine the type of global context available.
        let global: Global = js_sys::global().unchecked_into();
        let has_window = !global.window().is_undefined();
        let is_worker = !global.worker().is_undefined();

        if !has_window && !is_worker {
            let error = std::io::Error::other("Unsupported JavaScript global context");
            return Err(AssetReaderError::Io(error));
        }

        let promise = if has_window {
            global
                .unchecked_into::<web_sys::Window>()
                .fetch_with_str(fetch_path)
        } else {
            global
                .unchecked_into::<web_sys::WorkerGlobalScope>()
                .fetch_with_str(fetch_path)
        };

        let resp_value = JsFuture::from(promise)
            .await
            .map_err(js_value_to_err("fetch path"))?;
        let resp = resp_value
            .dyn_into::<Response>()
            .map_err(js_value_to_err("convert fetch to Response"))?;
        Ok(resp)
    }

    /// Fetches the bytes at `path`.
    // Must be `pub(crate)`, used by WebAssetPlugin.
    pub(crate) async fn fetch_bytes(
        &self,
        path: PathBuf,
    ) -> Result<impl Reader + use<>, AssetReaderError> {
        let path_str = path.to_str().unwrap();
        let fetch_path = self.map_request(path_str);

        let resp = SendFuture(Self::fetch_response(&fetch_path)).await?;

        match resp.status() {
            200 => {
                let buf = resp.array_buffer().unwrap();
                let data = SendFuture(JsFuture::from(buf)).await.unwrap();
                let bytes = Uint8Array::new(&data).to_vec();
                Ok(VecReader::new(bytes))
            }
            // Some web servers, including itch.io's CDN, return 403 when a requested file isn't present.
            // TODO: remove handling of 403 as not found when it's easier to configure
            403 | 404 => Err(AssetReaderError::NotFound((*fetch_path).into())),
            status => Err(AssetReaderError::HttpError(status)),
        }
    }
}

impl AssetReader for HttpWasmAssetReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let path = self.root_path.join(path);
        self.fetch_bytes(path).await
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let meta_path = append_meta_extension(&self.root_path.join(path));
        self.fetch_bytes(meta_path).await
    }

    async fn read_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        let stream: Box<PathStream> = Box::new(EmptyPathStream);
        zlim_log::error!("Reading directories is not supported with the HttpWasmAssetReader");
        Ok(stream)
    }

    async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
        zlim_log::error!("Reading directories is not supported with the HttpWasmAssetReader");
        Ok(false)
    }
}
