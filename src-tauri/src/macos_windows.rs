//! 基于 CoreGraphics 合成器的 macOS 窗口发现。
//!
//! Electron 风格应用常用全屏透明根窗口承载可见的应用面板。直接使用
//! CoreGraphics 原始边界会把整个显示器识别成窗口；这里缓存每个候选窗口的
//! 非透明内容边界，并用更准确的矩形参与命中测试。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use core::ffi::c_void;

use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType, CGPoint, CGRect, CGSize};
#[allow(deprecated)]
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGWindowImageOption, CGWindowListCopyWindowInfo, CGWindowListCreateImage,
    CGWindowListOption,
};

use crate::platform::{MonitorGeom, WindowHit};

/// 属于系统界面而非用户应用的窗口。
const EXCLUDED_OWNERS: &[&str] = &[
    "Window Server",
    "Dock",
    "Control Center",
    "Notification Center",
    "Spotlight",
];

/// 低于该 Alpha 值的像素视为透明窗体边缘或阴影。
const CONTENT_ALPHA_THRESHOLD: u8 = 32;

/// 透明边框通常只有几个逻辑点；小于该比例的间隙应吸附回窗口真实边缘。
const CONTENT_EDGE_SNAP_RATIO: f64 = 0.01;
const CONTENT_EDGE_SNAP_MIN: f64 = 4.0;
const CONTENT_EDGE_SNAP_MAX: f64 = 16.0;

/// Electron 根窗口通常会用一圈透明画布承载真正的内容。只有至少三侧
/// 存在明显透明边距时才采用内容边界；普通原生窗口的圆角或单侧材质缺口
/// 不应改变 CoreGraphics 给出的真实窗口位置。
const TRANSPARENT_MARGIN_MIN: f64 = 16.0;
const TRANSPARENT_MARGIN_SIDES: usize = 3;
const TRANSPARENT_CONTENT_RATIO: f64 = 0.92;

#[derive(Clone, Copy, Debug)]
struct GlobalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl GlobalRect {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

#[derive(Clone)]
struct RawWindow {
    id: u32,
    title: String,
    app_name: String,
    bounds: GlobalRect,
}

#[derive(Clone)]
struct ResolvedWindow {
    hit: WindowHit,
    contains_pointer: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ContentCacheKey {
    window_id: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

static CONTENT_BOUNDS: OnceLock<Mutex<HashMap<ContentCacheKey, Option<GlobalRect>>>> =
    OnceLock::new();

/// 借用字典值但不增加引用计数。
fn value<'a>(dict: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    let mut out: *const c_void = std::ptr::null();
    let found = unsafe { dict.value_if_present(key as *const CFString as *const c_void, &mut out) };
    if !found || out.is_null() {
        return None;
    }
    Some(unsafe { &*(out as *const CFType) })
}

fn num(dict: &CFDictionary, key: &CFString) -> Option<f64> {
    value(dict, key)
        .and_then(|v| CFType::downcast_ref::<CFNumber>(v))
        .and_then(|n| n.as_f64())
}

fn text(dict: &CFDictionary, key: &CFString) -> String {
    value(dict, key)
        .and_then(|v| CFType::downcast_ref::<CFString>(v))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn dict_of(value: &CFType) -> Option<&CFDictionary> {
    CFType::downcast_ref::<CFDictionary>(value)
}

fn monitor_rect(monitor: &MonitorGeom) -> GlobalRect {
    GlobalRect {
        x: monitor.x,
        y: monitor.y,
        width: monitor.width,
        height: monitor.height,
    }
}

/// 菜单栏位于 Window Server 的专用窗口层级。
fn menu_bar_at_point(monitor: &MonitorGeom, x: f64, y: f64) -> Option<GlobalRect> {
    let list = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    )?;
    let k_owner = CFString::from_static_str("kCGWindowOwnerName");
    let k_name = CFString::from_static_str("kCGWindowName");
    let k_layer = CFString::from_static_str("kCGWindowLayer");
    let k_bounds = CFString::from_static_str("kCGWindowBounds");
    let k_x = CFString::from_static_str("X");
    let k_y = CFString::from_static_str("Y");
    let k_w = CFString::from_static_str("Width");
    let k_h = CFString::from_static_str("Height");

    for index in 0..list.count() {
        let entry = unsafe { list.as_opaque().value_at_index(index) };
        if entry.is_null() {
            continue;
        }
        let Some(dict) = dict_of(unsafe { &*(entry as *const CFType) }) else {
            continue;
        };
        let owner = text(dict, &k_owner);
        let name = text(dict, &k_name);
        let layer = num(dict, &k_layer).unwrap_or(-1.0);
        if owner != "Window Server" || name != "Menubar" || layer != 24.0 {
            continue;
        }
        let Some(bounds) = value(dict, &k_bounds).and_then(dict_of) else {
            continue;
        };
        let rect = GlobalRect {
            x: num(bounds, &k_x).unwrap_or(0.0),
            y: num(bounds, &k_y).unwrap_or(0.0),
            width: num(bounds, &k_w).unwrap_or(0.0),
            height: num(bounds, &k_h).unwrap_or(0.0),
        };
        if rect.contains(x, y) && rect.intersects(monitor_rect(monitor)) {
            return Some(rect);
        }
    }
    None
}

/// 按前到后顺序返回可见应用窗口。
fn visible_windows(monitor: &MonitorGeom) -> Vec<RawWindow> {
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    ) else {
        return Vec::new();
    };

    let k_number = CFString::from_static_str("kCGWindowNumber");
    let k_owner = CFString::from_static_str("kCGWindowOwnerName");
    let k_name = CFString::from_static_str("kCGWindowName");
    let k_layer = CFString::from_static_str("kCGWindowLayer");
    let k_pid = CFString::from_static_str("kCGWindowOwnerPID");
    let k_alpha = CFString::from_static_str("kCGWindowAlpha");
    let k_bounds = CFString::from_static_str("kCGWindowBounds");
    let k_x = CFString::from_static_str("X");
    let k_y = CFString::from_static_str("Y");
    let k_w = CFString::from_static_str("Width");
    let k_h = CFString::from_static_str("Height");

    let screen = monitor_rect(monitor);
    let mut out = Vec::new();
    for index in 0..list.count() {
        let entry = unsafe { list.as_opaque().value_at_index(index) };
        if entry.is_null() {
            continue;
        }
        let Some(dict) = dict_of(unsafe { &*(entry as *const CFType) }) else {
            continue;
        };

        let owner = text(dict, &k_owner);
        if EXCLUDED_OWNERS.contains(&owner.as_str()) {
            continue;
        }
        let pid = num(dict, &k_pid).map(|pid| pid as i32).unwrap_or(-1);
        // 排除 Snaply 自己的截图覆盖层。
        if pid == std::process::id() as i32 {
            continue;
        }
        if num(dict, &k_alpha).unwrap_or(1.0) <= 0.01 {
            continue;
        }
        // 只保留普通应用窗口；菜单和系统浮层使用其他层级。
        if num(dict, &k_layer).unwrap_or(-1.0) != 0.0 {
            continue;
        }
        let Some(bounds) = value(dict, &k_bounds).and_then(dict_of) else {
            continue;
        };
        let (bx, by) = (
            num(bounds, &k_x).unwrap_or(0.0),
            num(bounds, &k_y).unwrap_or(0.0),
        );
        let (bw, bh) = (
            num(bounds, &k_w).unwrap_or(0.0),
            num(bounds, &k_h).unwrap_or(0.0),
        );
        if !bx.is_finite()
            || !by.is_finite()
            || !bw.is_finite()
            || !bh.is_finite()
            || bw < 8.0
            || bh < 8.0
        {
            continue;
        }
        let bounds = GlobalRect {
            x: bx,
            y: by,
            width: bw,
            height: bh,
        };
        if !bounds.intersects(screen) {
            continue;
        }

        out.push(RawWindow {
            id: num(dict, &k_number).unwrap_or(0.0) as u32,
            title: text(dict, &k_name),
            app_name: owner,
            bounds,
        });
    }
    out
}

fn cache_key(window: &RawWindow) -> ContentCacheKey {
    ContentCacheKey {
        window_id: window.id,
        x: window.bounds.x.round() as i32,
        y: window.bounds.y.round() as i32,
        width: window.bounds.width.round() as i32,
        height: window.bounds.height.round() as i32,
    }
}

fn content_bounds(window: &RawWindow) -> Option<GlobalRect> {
    let key = cache_key(window);
    let cache = CONTENT_BOUNDS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .copied()
    {
        return cached;
    }

    let captured = capture_content_bounds(window);
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, captured);
    captured
}

/// 单独抓取该窗口，并找出其边界内的非透明内容区域。
///
/// 直接抓取窗口而不是采样桌面图像，避免把透明边缘后面的桌面内容算进来。
#[allow(deprecated)]
fn capture_content_bounds(window: &RawWindow) -> Option<GlobalRect> {
    if !crate::platform::has_screen_permission() {
        return None;
    }
    let capture_rect = CGRect::new(
        CGPoint::new(window.bounds.x, window.bounds.y),
        CGSize::new(window.bounds.width, window.bounds.height),
    );
    let image = CGWindowListCreateImage(
        capture_rect,
        CGWindowListOption::OptionIncludingWindow,
        window.id,
        CGWindowImageOption::BoundsIgnoreFraming | CGWindowImageOption::BestResolution,
    )?;

    let image_width = CGImage::width(Some(&image));
    let image_height = CGImage::height(Some(&image));
    if image_width == 0 || image_height == 0 {
        return None;
    }

    // 缩小后的 Alpha 掩码让首次悬停保持轻量，映射回窗口坐标后误差约为一个逻辑点。
    let max_edge = 768usize;
    let scale = (max_edge as f64 / image_width.max(image_height) as f64).min(1.0);
    let sample_width = ((image_width as f64 * scale).round() as usize).max(1);
    let sample_height = ((image_height as f64 * scale).round() as usize).max(1);
    let bytes_per_row = sample_width.checked_mul(4)?;
    let byte_len = bytes_per_row.checked_mul(sample_height)?;
    let mut pixels = vec![0u8; byte_len];

    let color_space = CGColorSpace::new_device_rgb()?;
    let bitmap_info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            sample_width,
            sample_height,
            8,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
        )
    }?;
    CGContext::draw_image(
        Some(&context),
        CGRect::new(
            CGPoint::ZERO,
            CGSize::new(sample_width as f64, sample_height as f64),
        ),
        Some(&image),
    );

    let (min_x, min_y) = opaque_bounds(
        &pixels,
        sample_width,
        sample_height,
        CONTENT_ALPHA_THRESHOLD,
    )?;
    let max_x = max_opaque_x(
        &pixels,
        sample_width,
        sample_height,
        CONTENT_ALPHA_THRESHOLD,
    )?;
    let max_y = max_opaque_y(
        &pixels,
        sample_width,
        sample_height,
        CONTENT_ALPHA_THRESHOLD,
    )?;

    let content = GlobalRect {
        x: window.bounds.x + min_x as f64 / sample_width as f64 * window.bounds.width,
        y: window.bounds.y + min_y as f64 / sample_height as f64 * window.bounds.height,
        width: (max_x - min_x + 1) as f64 / sample_width as f64 * window.bounds.width,
        height: (max_y - min_y + 1) as f64 / sample_height as f64 * window.bounds.height,
    };
    let content = snap_content_edges(window.bounds, content);

    if content.width < 8.0 || content.height < 8.0 {
        None
    } else {
        Some(content)
    }
}

/// 拍摄窗口 Alpha 时，圆角和阴影会在边缘留下几个透明点。
/// 把很小的间隙吸附回真实窗口边界，避免左侧边框莫名向右缩进。
fn snap_content_edges(bounds: GlobalRect, mut content: GlobalRect) -> GlobalRect {
    let snap_x = (bounds.width * CONTENT_EDGE_SNAP_RATIO)
        .clamp(CONTENT_EDGE_SNAP_MIN, CONTENT_EDGE_SNAP_MAX);
    let snap_y = (bounds.height * CONTENT_EDGE_SNAP_RATIO)
        .clamp(CONTENT_EDGE_SNAP_MIN, CONTENT_EDGE_SNAP_MAX);

    let left_gap = content.x - bounds.x;
    if left_gap > 0.0 && left_gap <= snap_x {
        content.x = bounds.x;
        content.width += left_gap;
    }
    let right_gap = bounds.x + bounds.width - (content.x + content.width);
    if right_gap > 0.0 && right_gap <= snap_x {
        content.width += right_gap;
    }
    let top_gap = content.y - bounds.y;
    if top_gap > 0.0 && top_gap <= snap_y {
        content.y = bounds.y;
        content.height += top_gap;
    }
    let bottom_gap = bounds.y + bounds.height - (content.y + content.height);
    if bottom_gap > 0.0 && bottom_gap <= snap_y {
        content.height += bottom_gap;
    }
    content
}

/// 决定使用原始窗口边界还是 Alpha 内容边界。
///
/// macOS 原生窗口的 `kCGWindowBounds` 已经是用户看到的窗口外框。窗口截图
/// 中的圆角、阴影或材质抗锯齿可能让单侧 Alpha 提前消失，若直接采用内容
/// 边界会把整个窗口向左或向右缩进。透明根窗口则相反，原始边界接近整块
/// 显示器而内容只占其中一部分，此时才需要使用内容边界。
fn window_bounds_for_hit(raw: GlobalRect, content: Option<GlobalRect>) -> GlobalRect {
    let Some(content) = content else {
        return raw;
    };
    if !content.intersects(raw) {
        return raw;
    }

    let left = (content.x - raw.x).max(0.0);
    let top = (content.y - raw.y).max(0.0);
    let right = (raw.x + raw.width - content.x - content.width).max(0.0);
    let bottom = (raw.y + raw.height - content.y - content.height).max(0.0);
    let substantial_margins = [left, top, right, bottom]
        .into_iter()
        .filter(|margin| *margin >= TRANSPARENT_MARGIN_MIN)
        .count();
    let content_ratio = (content.width * content.height) / (raw.width * raw.height).max(1.0);

    if substantial_margins >= TRANSPARENT_MARGIN_SIDES && content_ratio <= TRANSPARENT_CONTENT_RATIO
    {
        content
    } else {
        raw
    }
}

fn opaque_bounds(
    rgba: &[u8],
    width: usize,
    height: usize,
    threshold: u8,
) -> Option<(usize, usize)> {
    for y in 0..height {
        for x in 0..width {
            if rgba[(y * width + x) * 4 + 3] >= threshold {
                return Some((x, y));
            }
        }
    }
    None
}

fn max_opaque_x(rgba: &[u8], width: usize, height: usize, threshold: u8) -> Option<usize> {
    for x in (0..width).rev() {
        for y in 0..height {
            if rgba[(y * width + x) * 4 + 3] >= threshold {
                return Some(x);
            }
        }
    }
    None
}

fn max_opaque_y(rgba: &[u8], width: usize, height: usize, threshold: u8) -> Option<usize> {
    for y in (0..height).rev() {
        for x in 0..width {
            if rgba[(y * width + x) * 4 + 3] >= threshold {
                return Some(y);
            }
        }
    }
    None
}

fn resolve_window(raw: &RawWindow, content: GlobalRect, monitor: &MonitorGeom) -> WindowHit {
    WindowHit {
        title: raw.title.clone(),
        app_name: raw.app_name.clone(),
        x: content.x - monitor.x,
        y: content.y - monitor.y,
        width: content.width,
        height: content.height,
        layer: 1,
    }
}

fn choose_resolved(candidates: &[ResolvedWindow]) -> Option<WindowHit> {
    candidates
        .iter()
        .find(|candidate| candidate.contains_pointer)
        .map(|candidate| candidate.hit.clone())
}

/// 返回指针下方最上层的可见应用窗口。
pub fn windows_at_point(monitor: &MonitorGeom, cx: f64, cy: f64) -> Vec<WindowHit> {
    if !cx.is_finite() || !cy.is_finite() {
        return Vec::new();
    }
    let global_x = monitor.x + cx;
    let global_y = monitor.y + cy;
    let mut candidates = Vec::new();

    if let Some(menu_bar) = menu_bar_at_point(monitor, global_x, global_y) {
        return vec![WindowHit {
            title: "系统菜单栏".into(),
            app_name: "菜单栏".into(),
            x: menu_bar.x - monitor.x,
            y: menu_bar.y - monitor.y,
            width: menu_bar.width,
            height: menu_bar.height,
            layer: 1,
        }];
    }

    for raw in visible_windows(monitor) {
        if !raw.bounds.contains(global_x, global_y) {
            continue;
        }
        let content = window_bounds_for_hit(raw.bounds, content_bounds(&raw));
        candidates.push(ResolvedWindow {
            hit: resolve_window(&raw, content, monitor),
            contains_pointer: content.contains(global_x, global_y),
        });
    }

    choose_resolved(&candidates)
        .map(|hit| vec![hit])
        .unwrap_or_else(|| {
            vec![WindowHit {
                title: "全屏".into(),
                app_name: "桌面".into(),
                x: 0.0,
                y: 0.0,
                width: monitor.width,
                height: monitor.height,
                layer: 1,
            }]
        })
}

/// 新截图会话开始时清空 Alpha 内容边界缓存。
pub fn clear_content_bounds_cache() {
    CONTENT_BOUNDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读取真实 CoreGraphics 窗口列表并校验解析出的字段。
    #[test]
    fn reads_visible_windows_without_crashing() {
        let monitor = MonitorGeom {
            id: 0,
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let windows = visible_windows(&monitor);
        for w in &windows {
            assert!(w.bounds.width.is_finite() && w.bounds.height.is_finite());
            assert!(w.bounds.x.is_finite() && w.bounds.y.is_finite());
        }
        let _ = windows_at_point(&monitor, 100.0, 100.0);
    }

    #[test]
    fn opaque_bounds_ignore_transparent_window_margins() {
        let width = 20;
        let height = 20;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 5..15 {
            for x in 4..17 {
                rgba[(y * width + x) * 4 + 3] = 255;
            }
        }
        assert_eq!(opaque_bounds(&rgba, width, height, 32), Some((4, 5)));
        assert_eq!(max_opaque_x(&rgba, width, height, 32), Some(16));
        assert_eq!(max_opaque_y(&rgba, width, height, 32), Some(14));
    }

    #[test]
    fn detached_title_bar_remains_its_own_region() {
        let candidates = vec![
            ResolvedWindow {
                hit: WindowHit {
                    title: "标题栏".into(),
                    app_name: "Example".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 900.0,
                    height: 40.0,
                    layer: 1,
                },
                contains_pointer: true,
            },
            ResolvedWindow {
                hit: WindowHit {
                    title: "主窗口".into(),
                    app_name: "Example".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 900.0,
                    height: 700.0,
                    layer: 1,
                },
                contains_pointer: true,
            },
        ];
        assert_eq!(choose_resolved(&candidates).unwrap().title, "标题栏");
    }

    #[test]
    fn normal_topmost_window_keeps_front_to_back_order() {
        let candidates = vec![
            ResolvedWindow {
                hit: WindowHit {
                    title: "顶层小窗".into(),
                    app_name: "Top".into(),
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 80.0,
                    layer: 1,
                },
                contains_pointer: true,
            },
            ResolvedWindow {
                hit: WindowHit {
                    title: "底层大窗".into(),
                    app_name: "Bottom".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 500.0,
                    height: 400.0,
                    layer: 1,
                },
                contains_pointer: true,
            },
        ];
        assert_eq!(choose_resolved(&candidates).unwrap().title, "顶层小窗");
    }

    #[test]
    fn snap_content_edges_removes_small_transparent_margin() {
        let bounds = GlobalRect {
            x: 876.0,
            y: -1410.0,
            width: 2560.0,
            height: 1410.0,
        };
        let content = GlobalRect {
            x: 882.666,
            y: -1410.0,
            width: 2553.334,
            height: 1410.0,
        };

        let snapped = snap_content_edges(bounds, content);
        assert_eq!(snapped.x, bounds.x);
        assert_eq!(snapped.width, bounds.width);
        assert_eq!(snapped.y, bounds.y);
        assert_eq!(snapped.height, bounds.height);
    }

    #[test]
    fn native_window_keeps_raw_bounds_when_one_side_is_trimmed() {
        let bounds = GlobalRect {
            x: 186.0,
            y: 326.0,
            width: 753.0,
            height: 665.0,
        };
        let content = GlobalRect {
            x: 198.0,
            y: 326.0,
            width: 741.0,
            height: 665.0,
        };

        assert_rect_eq(window_bounds_for_hit(bounds, Some(content)), bounds);
    }

    #[test]
    fn transparent_root_window_uses_visible_content_bounds() {
        let bounds = GlobalRect {
            x: 0.0,
            y: 0.0,
            width: 1800.0,
            height: 1169.0,
        };
        let content = GlobalRect {
            x: 260.0,
            y: 120.0,
            width: 1280.0,
            height: 820.0,
        };

        assert_rect_eq(window_bounds_for_hit(bounds, Some(content)), content);
    }

    fn assert_rect_eq(actual: GlobalRect, expected: GlobalRect) {
        assert_eq!(actual.x, expected.x);
        assert_eq!(actual.y, expected.y);
        assert_eq!(actual.width, expected.width);
        assert_eq!(actual.height, expected.height);
    }
}
