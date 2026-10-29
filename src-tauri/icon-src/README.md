# 图标源文件

| 文件 | 用途 |
| --- | --- |
| `app-icon.svg` | 应用图标主稿（1024×1024） |
| `tray.svg` | 托盘单色模板图标（仅 alpha，macOS 会自动反色） |
| `preview.svg` / `preview.png` | 设计稿预览（配色 / 尺寸阶梯 / 托盘深浅菜单栏），自包含，图标与托盘图形已内联 |

生成产物在 `../icons/`，`tauri.conf.json` 里引用的就是这些文件。

## 设计说明

- **构图**：深色超椭圆底板上，一枚偏左上的银色金属镜环，镜面里斜穿一道棱镜光。
  不做相机/取景框这种具象符号，用几何和材质表达「光被截下来」的那一下。
- **配色**：底板 `#55596A → #23252C → #0A0B0D` 径向渐变（左上受光），
  镜环 `#FFFFFF → #C2C9D6 → #7A8392 → #E6EAF1 → #8A93A2` 银色金属渐变，
  棱镜光 `#7FF0D0 → #8FB6FF → #C9A0FF → #FFB3D9`。整体只靠明度区分，去掉颜色也能成立。
- **形状**：按 macOS 图标栅格绘制 —— 1024 画布、824×824 主体（四周各 100px 留白、
  按 50–90% 缩放居中），连续圆角（近似 iOS squircle），徽标元素收在圆角内切范围内。
  底部叠一层柔和投影，与系统 App 并排时大小和重心一致。
- **托盘**：截屏取景框与中心焦点，44px（22pt @2x）。纯 alpha 模板，
  macOS 自动适配浅/深色菜单栏；Windows / Linux 用黑色线条。
- 设置窗口「关于」里的 `SnaplyMark`（`src/settings/SettingsIcons.tsx`）与主稿同构，改图标时一起改。

## 重新生成

```bash
# 应用图标：icns / ico / png / Windows Store logo 全量重出
npx tauri icon src-tauri/icon-src/app-icon.svg -o src-tauri/icons

# 托盘图标（需要 librsvg：brew install librsvg）
rsvg-convert -w 44 -h 44 src-tauri/icon-src/tray.svg -o src-tauri/icons/tray@2x.png

# tauri icon 会顺带生成 iOS / Android 图标，桌面项目用不到
rm -rf src-tauri/icons/android src-tauri/icons/ios
```
