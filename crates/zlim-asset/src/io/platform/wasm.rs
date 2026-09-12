//! Wasm asset reader that fetches assets over HTTP.

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
    /// Fetches the bytes at `path`.
    pub(crate) async fn fetch_bytes(
        &self,
        path: PathBuf,
    ) -> Result<impl Reader + use<>, AssetReaderError> {
        let path = path.to_str().unwrap();
        let fetch_path = self
            .request_mapper
            .as_ref()
            .map_or_else(|| Cow::Borrowed(path), |mapper| mapper(path));

        // The JS global scope includes a self-reference via a specializing name, which can be used to determine the type of global context available.
        let global: Global = js_sys::global().unchecked_into();
        let promise = if !global.window().is_undefined() {
            let window: web_sys::Window = global.unchecked_into();
            window.fetch_with_str(&fetch_path)
        } else if !global.worker().is_undefined() {
            let worker: web_sys::WorkerGlobalScope = global.unchecked_into();
            worker.fetch_with_str(&fetch_path)
        } else {
            let error = std::io::Error::other("Unsupported JavaScript global context");
            return Err(AssetReaderError::Io(error.into()));
        };
        let resp_value = JsFuture::from(promise)
            .await
            .map_err(js_value_to_err("fetch path"))?;
        let resp = resp_value
            .dyn_into::<Response>()
            .map_err(js_value_to_err("convert fetch to Response"))?;
        match resp.status() {
            200 => {
                let data = JsFuture::from(resp.array_buffer().unwrap()).await.unwrap();
                let bytes = Uint8Array::new(&data).to_vec();
                let reader = VecReader::new(bytes);
                Ok(reader)
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
