"""从 1024px 品牌 PNG 生成 Windows ICO 与 macOS ICNS。

SVG 到 PNG 由 resvg 负责；这里仅做确定性的容器封装，避免两个渲染器
对同一矢量源产生不同抗锯齿结果。
"""

from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "docs" / "brand" / "assets"
TAURI = ROOT / "apps" / "desktop" / "src-tauri" / "icons"
SOURCE = BRAND / "rigdeck-mark-1024.png"


def main() -> None:
    image = Image.open(SOURCE).convert("RGBA")
    ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]

    image.save(BRAND / "rigdeck.ico", format="ICO", sizes=ico_sizes)
    image.save(TAURI / "icon.ico", format="ICO", sizes=ico_sizes)

    # Pillow 会从 1024px master 生成 ICNS 所需的标准多分辨率条目。
    image.save(BRAND / "rigdeck.icns", format="ICNS")
    image.save(TAURI / "icon.icns", format="ICNS")
    print("Windows ICO 与 macOS ICNS 已生成。")


if __name__ == "__main__":
    main()

