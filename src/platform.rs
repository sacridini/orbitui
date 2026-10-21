//! What differs between the terminal and the browser: network, storage and
//! handing files to the user.
//!
//! Keys of the store are file names; those under `cache/` hold data that can
//! be fetched again, the others the user's settings.

pub use imp::*;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::PathBuf;
    use std::time::Duration;

    /// Background threads are available (forecasts run off the UI thread).
    pub const THREADS: bool = true;

    /// GETs `url` in the background and calls `done` with the body.
    pub fn get(url: String, done: impl FnOnce(Result<String, String>) + Send + 'static) {
        std::thread::spawn(move || {
            let body = ureq::get(&url)
                .set("User-Agent", concat!("orbitui/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .call()
                .map_err(|e| e.to_string())
                .and_then(|r| r.into_string().map_err(|e| e.to_string()));
            done(body);
        });
    }

    pub fn location(key: &str) -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "orbitui")?;
        Some(match key.strip_prefix("cache/") {
            Some(name) => dirs.cache_dir().join(name),
            None => dirs.config_dir().join(key),
        })
    }

    pub fn load(key: &str) -> Option<String> {
        std::fs::read_to_string(location(key)?).ok()
    }

    pub fn save(key: &str, value: &str) {
        let Some(path) = location(key) else { return };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(path, value).ok();
    }

    pub fn read_file(path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
    }

    /// Writes a file for the user in the working directory.
    pub fn deliver(name: &str, content: &str) -> Result<String, String> {
        std::fs::write(name, content).map_err(|e| format!("{name}: {e}"))?;
        Ok(name.into())
    }

    /// Where `deliver` puts files.
    pub fn delivery_place() -> String {
        std::env::current_dir().map(|d| format!(" in {}", d.display())).unwrap_or_default()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Response, Storage, Url};

    /// No threads in the browser: forecasts run in slices between frames.
    pub const THREADS: bool = false;

    fn js_error(e: JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    async fn fetch_text(url: &str) -> Result<String, String> {
        let window = web_sys::window().ok_or("no window")?;
        let response = JsFuture::from(window.fetch_with_str(url)).await.map_err(js_error)?;
        let response: Response = response.dyn_into().map_err(js_error)?;
        if !response.ok() {
            return Err(format!("HTTP {} for {url}", response.status()));
        }
        let text = JsFuture::from(response.text().map_err(js_error)?).await.map_err(js_error)?;
        text.as_string().ok_or_else(|| "response is not text".into())
    }

    /// GETs `url` in the background and calls `done` with the body.
    pub fn get(url: String, done: impl FnOnce(Result<String, String>) + Send + 'static) {
        wasm_bindgen_futures::spawn_local(async move { done(fetch_text(&url).await) });
    }

    fn storage() -> Option<Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load(key: &str) -> Option<String> {
        storage()?.get_item(&format!("orbitui.{key}")).ok()?
    }

    pub fn save(key: &str, value: &str) {
        if let Some(s) = storage() {
            // Quota errors only cost us the cache.
            s.set_item(&format!("orbitui.{key}"), value).ok();
        }
    }

    pub fn read_file(_path: &str) -> Result<String, String> {
        Err("drop the GeoJSON file on the page instead".into())
    }

    /// Hands a file to the user as a browser download.
    pub fn deliver(name: &str, content: &str) -> Result<String, String> {
        let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let parts = js_sys::Array::of1(&JsValue::from_str(content));
        let options = BlobPropertyBag::new();
        options.set_type("text/plain;charset=utf-8");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &options).map_err(js_error)?;
        let url = Url::create_object_url_with_blob(&blob).map_err(js_error)?;
        let anchor: HtmlAnchorElement =
            document.create_element("a").map_err(js_error)?.dyn_into().map_err(|_| "no anchor element")?;
        anchor.set_href(&url);
        anchor.set_download(name);
        anchor.click();
        Url::revoke_object_url(&url).ok();
        Ok(name.into())
    }

    /// Where `deliver` puts files.
    pub fn delivery_place() -> String {
        " as downloads".into()
    }
}
