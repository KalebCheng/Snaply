# 图标源文件

| 文件 | 用途 |
| --- | --- |
| `app-icon.svg` | 应用图标主稿（1024×1024） |
| `tray.svg` | 托盘单色模板图标（仅 alpha，macOS 会自动反色） |
| `preview.svg` / `preview.png` | 历史设计稿预览；当前正式图标以 `app-icon.svg` 和 `tray.svg` 为准 |

生成产物在 `../icons/`，`tauri.conf.json` 里引用的就是这些文件。

## 设计说明

- **构图**：暖米色超椭圆底板上使用「关于」页面的 Snaply 标记：四角取景框、
  对角捕获线和箭头尖端，保持两处品牌图形完全一致。
- **配色**：底板 `#FFF9F0 → #FFE9D2 → #F9CFA6` 暖色渐变，
  标记使用 `#FF982E → #F97316 → #D94B05` 橙色渐变。
- **形状**：按 macOS 图标栅格绘制 —— 1024 画布、824×824 主体（四周各 100px 留白）、
  连续圆角并带柔和投影。
- **托盘**：使用同一 Snaply 标记的单色版本，44px（22pt @2x）。纯 alpha 模板，
  macOS 自动适配浅/深色菜单栏；Windows / Linux 用黑色线条。
- 设置窗口「关于」里的 `SnaplyMark`（`src/settings/SettingsIcons.tsx`）与应用图标、托盘图标同构。

## 重新生成

```bash
# 应用图标：icns / ico / png / Windows Store logo 全量重出
npx tauri icon src-tauri/icon-src/app-icon.svg -o src-tauri/icons

# 托盘图标（需要 librsvg：brew install librsvg）
rsvg-convert -w 44 -h 44 src-tauri/icon-src/tray.svg -o src-tauri/icons/tray@2x.png

# tauri icon 会顺带生成 iOS / Android 图标，桌面项目用不到
rm -rf src-tauri/icons/android src-tauri/icons/ios
```
