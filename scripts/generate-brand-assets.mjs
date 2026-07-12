import { createRequire } from "node:module";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, "..");

// 这个脚本位于仓库根目录，而依赖锁在桌面前端包中。
// createRequire 让 Node 按 apps/desktop/package.json 的位置解析依赖，
// 因此无需在仓库根目录再维护一份 node_modules。
const requireFromDesktop = createRequire(join(root, "apps", "desktop", "package.json"));
const { Resvg } = requireFromDesktop("@resvg/resvg-js");

const brandDir = join(root, "docs", "brand", "assets");
const tauriIconsDir = join(root, "apps", "desktop", "src-tauri", "icons");
const publicDir = join(root, "apps", "desktop", "public");
mkdirSync(tauriIconsDir, { recursive: true });
mkdirSync(publicDir, { recursive: true });

function render(svgName, output, width) {
  const svg = readFileSync(join(brandDir, svgName), "utf8");
  const image = new Resvg(svg, {
    fitTo: { mode: "width", value: width },
    font: { loadSystemFonts: true },
  });
  writeFileSync(output, image.render().asPng());
}

for (const size of [16, 32, 48, 64, 128, 256, 512, 1024]) {
  render("rigdeck-mark.svg", join(brandDir, `rigdeck-mark-${size}.png`), size);
}

render("rigdeck-social-preview.svg", join(brandDir, "rigdeck-social-preview.png"), 1280);
render("rigdeck-mark.svg", join(publicDir, "favicon.png"), 32);
render("rigdeck-mark.svg", join(tauriIconsDir, "32x32.png"), 32);
render("rigdeck-mark.svg", join(tauriIconsDir, "128x128.png"), 128);
render("rigdeck-mark.svg", join(tauriIconsDir, "128x128@2x.png"), 256);

console.log("品牌 PNG 与 Tauri 基础图标已生成。下一步运行 generate-platform-icons.py 生成 ICO/ICNS。");

