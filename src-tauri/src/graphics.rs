use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxGraphicsFix {
    None,
    DisableExplicitSync,
    DisableDmabuf,
}

pub fn linux_graphics_fix(nvidia: bool, wayland: bool) -> LinuxGraphicsFix {
    if !nvidia {
        return LinuxGraphicsFix::None;
    }
    if wayland {
        LinuxGraphicsFix::DisableExplicitSync
    } else {
        LinuxGraphicsFix::DisableDmabuf
    }
}


#[cfg(target_os = "linux")]
pub fn is_flicker_prone_feature(identifier: &str) -> bool {
    // Keep damage propagation enabled. Disabling it makes WebKit repaint the
    // whole window for theme changes and continuously animated elements, which
    // spikes GPU usage even on high-end cards.
    matches!(identifier, "UseSkiaForComposition")
}

#[cfg(target_os = "linux")]
pub fn configure_webkit_memory(webview: webkit2gtk::WebView) {
    use webkit2gtk::{CacheModel, SettingsExt, WebContextExt, WebViewExt};

    if let Some(context) = webview.context() {
        context.set_cache_model(CacheModel::DocumentViewer);
    }
    if let Some(settings) = webview.settings() {
        settings.set_enable_page_cache(false);
    }
}

#[cfg(target_os = "linux")]
pub fn disable_webkit_flicker_features(webview: webkit2gtk::WebView) {
    use glib::object::ObjectType;
    use std::ffi::{c_char, c_void, CStr};
    use webkit2gtk::WebViewExt;

    type Settings = c_void;
    type FeatureList = c_void;
    type Feature = c_void;

    extern "C" {
        fn webkit_settings_get_all_features() -> *mut FeatureList;
        fn webkit_feature_list_get_length(list: *mut FeatureList) -> usize;
        fn webkit_feature_list_get(list: *mut FeatureList, index: usize) -> *mut Feature;
        fn webkit_feature_list_unref(list: *mut FeatureList);
        fn webkit_feature_get_identifier(feature: *mut Feature) -> *const c_char;
        fn webkit_settings_set_feature_enabled(
            settings: *mut Settings,
            feature: *mut Feature,
            enabled: i32,
        );
    }

    let Some(settings) = webview.settings() else {
        return;
    };
    let settings = settings.as_ptr() as *mut Settings;

    unsafe {
        let features = webkit_settings_get_all_features();
        if features.is_null() {
            return;
        }
        let length = webkit_feature_list_get_length(features);
        for index in 0..length {
            let feature = webkit_feature_list_get(features, index);
            if feature.is_null() {
                continue;
            }
            let identifier = webkit_feature_get_identifier(feature);
            if identifier.is_null() {
                continue;
            }
            let identifier = CStr::from_ptr(identifier).to_string_lossy();
            if is_flicker_prone_feature(identifier.as_ref()) {
                webkit_settings_set_feature_enabled(settings, feature, 0);
                eprintln!("[graphics] disabled WebKit feature {identifier}");
            }
        }
        webkit_feature_list_unref(features);
    }
}

fn session_is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

pub fn nvidia_module_loaded() -> bool {
    Path::new("/sys/module/nvidia").exists()
}

fn disable_dmabuf_renderer() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

pub fn apply_graphics_workarounds() {
    let nvidia = nvidia_module_loaded();
    if nvidia && std::env::var_os("WEBKIT_USE_SKIA_FOR_COMPOSITION").is_none() {
        std::env::set_var("WEBKIT_USE_SKIA_FOR_COMPOSITION", "0");
    }
    // Keep WebKit's GPU tile rasterization. Forcing CPU tile rasterization
    // avoids the Skia GPU tile cache, but makes full-window repaints (theme
    // changes and opening Appearance) take hundreds of milliseconds on NVIDIA.
    // Users who prefer the lower VRAM footprint can opt in with
    // WEBKIT_SKIA_ENABLE_CPU_RENDERING=1.
    let wayland = session_is_wayland();
    match linux_graphics_fix(nvidia, wayland) {
        LinuxGraphicsFix::None => {}
        LinuxGraphicsFix::DisableExplicitSync => {
            if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
                std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
            }
        }
        LinuxGraphicsFix::DisableDmabuf => disable_dmabuf_renderer(),
    }
    eprintln!(
        "[graphics] nvidia={nvidia} wayland={wayland} skia={:?} skia_cpu={:?} explicit_sync={:?} dmabuf_disabled={:?}",
        std::env::var("WEBKIT_USE_SKIA_FOR_COMPOSITION"),
        std::env::var("WEBKIT_SKIA_ENABLE_CPU_RENDERING"),
        std::env::var("__NV_DISABLE_EXPLICIT_SYNC"),
        std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER")
    );
}

#[cfg(test)]
mod tests {
    use super::{linux_graphics_fix, LinuxGraphicsFix};

    #[test]
    fn leaves_non_nvidia_systems_alone() {
        assert_eq!(linux_graphics_fix(false, true), LinuxGraphicsFix::None);
        assert_eq!(linux_graphics_fix(false, false), LinuxGraphicsFix::None);
    }

    #[test]
    fn uses_explicit_sync_fix_on_nvidia_wayland() {
        assert_eq!(
            linux_graphics_fix(true, true),
            LinuxGraphicsFix::DisableExplicitSync
        );
    }

    #[test]
    fn disables_dmabuf_on_nvidia_x11() {
        assert_eq!(
            linux_graphics_fix(true, false),
            LinuxGraphicsFix::DisableDmabuf
        );
    }

    #[test]
    fn selects_the_webkit_compositor_features_that_flash() {
        use super::is_flicker_prone_feature;

        assert!(is_flicker_prone_feature("UseSkiaForComposition"));
        assert!(!is_flicker_prone_feature(
            "UseDamagingInformationForCompositing"
        ));
        assert!(!is_flicker_prone_feature("PropagateDamagingInformation"));
        assert!(!is_flicker_prone_feature("WebGL"));
    }
}
