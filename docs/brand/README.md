# RigDeck 品牌规范 / Brand Guide

## 品牌表达

- 中文口号：**一处装配，让每个 Agent 各就其位。**
- English slogan: **One deck. Every agent, perfectly equipped.**
- 图形语义：圆角 `D` 同时表示 Deck/装配甲板；内部三个相连模块依次代表 Skill、Prompt 和 MCP。

## 颜色与用法

| Token | 值 | 用法 |
|---|---|---|
| `brand-primary` | `#1B4D7E` | 品牌轮廓、主操作、标题强调 |
| `brand-accent` | `#2DD4BF` | 节点、高亮、状态强调；不单独用于白底小字号文本 |
| `foreground` | 主题令牌 | 正文与图标，不在组件中硬编码 |

UI 组件只能引用设计令牌；SVG 品牌源文件可以固定品牌标准色。主色 `#1B4D7E` 与白色背景对比度满足 WCAG AA/AAA 正文要求；accent 依赖深色轮廓或深色背景保证可辨识。

## 安全留白与最小尺寸

- 图形标志四周至少保留一个模块节点直径的留白。
- UI 最小尺寸 16×16；该尺寸使用已验证的标准 mark，不添加 wordmark。
- 小于 32px 时不得移除深蓝轮廓，否则 teal 节点在浅色背景上对比不足。
- 单色场景使用 `rigdeck-mark-mono-dark.svg` 或 `rigdeck-mark-mono-light.svg`。

## 资产清单

- [x] Master SVG：`rigdeck-mark.svg`
- [x] 深/浅单色 SVG
- [x] 横向组合标志
- [x] Windows 多分辨率 `.ico`
- [x] macOS `.icns`
- [x] PNG：16、32、48、64、128、256、512、1024
- [x] 桌面 favicon
- [x] CLI mark
- [x] GitHub social preview（SVG + 1280×640 PNG）

## 可复现生成

```powershell
Set-Location apps/desktop
npm ci
npm run brand:build

Set-Location ../..
python scripts/generate-platform-icons.py
```

唯一设计源是 `docs/brand/assets/*.svg`；PNG/ICO/ICNS 是派生产物，不应单独手工编辑。

