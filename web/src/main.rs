//! orbitui in the browser: the same app, drawn by Ratzilla.
//!
//! The URL can set the area of interest and the opening screen, so that a
//! forecast can be shared: `?lat=-3.10&lon=-60.02&name=Manaus&view=agenda`
//! (also `map=1`, `days=7` and `geojson=<url>`). GeoJSON files dropped on
//! the page become the area of interest.

use orbitui::app::{App, KeyCode, KeyEvent, Settings, View};
use orbitui::orbit::Observer;
use orbitui::{config, ui};
use ratzilla::backend::webgl2::WebGl2BackendOptions;
use ratzilla::event::KeyCode as Code;
use ratzilla::ratatui::Terminal;
use ratzilla::ratatui::backend::Backend;
use ratzilla::ratatui::symbols::Marker;
use ratzilla::{CanvasBackend, FontAtlasConfig, WebEventHandler, WebGl2Backend, WebRenderer};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Monospace fonts with braille glyphs, in order of preference; the browser
/// falls back to its own monospace font.
const FONTS: [&str; 5] = ["JetBrains Mono", "Cascadia Mono", "DejaVu Sans Mono", "Menlo", "Consolas"];

fn main() {
    console_error_panic_hook::set_once();
    let params = query();
    let days = param(&params, "days").and_then(|v| v.parse::<f64>().ok()).unwrap_or(3.0).clamp(1.0, 14.0);
    let settings = Settings { days, min_el: 10.0, offline: false };
    let mut app = App::new(observer(&params), settings, Marker::Braille, false);
    if param(&params, "view").as_deref() == Some("agenda") {
        app.view = View::Agenda;
    }
    app.globe.flat = param(&params, "map").is_some_and(|v| v == "1");
    if let Some(url) = param(&params, "geojson") {
        app.fetch_geojson(url);
    }
    let app = Rc::new(RefCell::new(app));
    accept_dropped_files(&app);

    // The page's CSS sizes the canvas to the window; the backend follows it.
    let options = WebGl2BackendOptions::new()
        .font_atlas_config(FontAtlasConfig::dynamic(&FONTS, 15.0))
        .disable_auto_css_resize();
    if let Some(terminal) = WebGl2Backend::new_with_options(options).ok().and_then(|b| Terminal::new(b).ok()) {
        run(terminal, app);
    } else {
        // No WebGL2: the 2D canvas is slower but available everywhere.
        let backend = CanvasBackend::new().expect("no canvas to draw on");
        run(Terminal::new(backend).expect("no canvas to draw on"), app);
    }
}

fn run<B: Backend + WebEventHandler + 'static>(mut terminal: Terminal<B>, app: Rc<RefCell<App>>) {
    let keys = Rc::clone(&app);
    terminal
        .on_key_event(move |key| {
            if let Some(key) = convert(key) {
                keys.borrow_mut().on_key(key);
            }
        })
        .ok();
    terminal.draw_web(move |frame| {
        let mut app = app.borrow_mut();
        app.tick();
        ui::draw(frame, &mut app);
    });
}

fn convert(key: ratzilla::event::KeyEvent) -> Option<KeyEvent> {
    let code = match key.code {
        Code::Char(c) => KeyCode::Char(c),
        Code::Enter => KeyCode::Enter,
        Code::Esc => KeyCode::Esc,
        Code::Backspace => KeyCode::Backspace,
        Code::Tab if key.shift => KeyCode::BackTab,
        Code::Tab => KeyCode::Tab,
        Code::Up => KeyCode::Up,
        Code::Down => KeyCode::Down,
        Code::Left => KeyCode::Left,
        Code::Right => KeyCode::Right,
        Code::PageUp => KeyCode::PageUp,
        Code::PageDown => KeyCode::PageDown,
        Code::Home => KeyCode::Home,
        Code::End => KeyCode::End,
        _ => return None,
    };
    Some(KeyEvent { code, ctrl: key.ctrl })
}

/// Files dropped on the page: GeoJSON becomes the area of interest.
fn accept_dropped_files(app: &Rc<RefCell<App>>) {
    let Some(window) = web_sys::window() else { return };
    let over = Closure::<dyn FnMut(web_sys::DragEvent)>::new(|e: web_sys::DragEvent| e.prevent_default());
    window.add_event_listener_with_callback("dragover", over.as_ref().unchecked_ref()).ok();
    over.forget();

    let app = Rc::clone(app);
    let drop = Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |e: web_sys::DragEvent| {
        e.prevent_default();
        let Some(files) = e.data_transfer().and_then(|d| d.files()) else { return };
        for i in 0..files.length() {
            let Some(file) = files.get(i) else { continue };
            let app = Rc::clone(&app);
            wasm_bindgen_futures::spawn_local(async move {
                let name = file.name();
                let Ok(text) = wasm_bindgen_futures::JsFuture::from(file.text()).await else { return };
                let text = text.as_string().unwrap_or_default();
                let mut app = app.borrow_mut();
                if let Err(e) = app.set_geojson(&text, &config::file_stem(&name)) {
                    app.set_status(format!("{name}: {e}"));
                }
            });
        }
    });
    window.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref()).ok();
    drop.forget();
}

fn query() -> Option<web_sys::UrlSearchParams> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()
}

fn param(params: &Option<web_sys::UrlSearchParams>, key: &str) -> Option<String> {
    params.as_ref()?.get(key)
}

/// The AOI from `?lat=&lon=&name=`, else the last one used.
fn observer(params: &Option<web_sys::UrlSearchParams>) -> Observer {
    let num = |k: &str| param(params, k).and_then(|v| v.parse::<f64>().ok());
    if let (Some(lat), Some(lon)) = (num("lat"), num("lon"))
        && lat.abs() <= 90.0
        && lon.abs() <= 180.0
    {
        let obs = Observer::new(param(params, "name").unwrap_or_else(|| "AOI".into()), lat, lon, 0.0);
        config::save(&obs);
        return obs;
    }
    config::load().unwrap_or_else(config::default_observer)
}
