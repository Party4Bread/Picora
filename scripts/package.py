"""Create portable unsigned archives from `cargo build --release`. No publishing."""
from pathlib import Path
import platform
import plistlib
import shutil
import tarfile
import tomllib
import zipfile

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
system = platform.system()
stage = root / "target" / "package" / system / "Picora"
stage.mkdir(parents=True, exist_ok=True)
exe = root / "target" / "release" / ("picora.exe" if system == "Windows" else "picora")
if system == "Darwin":
    app = stage / "Picora.app" / "Contents"
    (app / "MacOS").mkdir(parents=True, exist_ok=True)
    (app / "Resources").mkdir(exist_ok=True)
    shutil.copy2(exe, app / "MacOS" / "picora")
    with (app / "Info.plist").open("wb") as f:
        plistlib.dump({"CFBundleName": "Picora", "CFBundleDisplayName": "Picora", "CFBundleExecutable": "picora", "CFBundleIdentifier": "app.picora.desktop", "CFBundleVersion": version, "CFBundleShortVersionString": version, "CFBundlePackageType": "APPL", "LSUIElement": True, "NSHighResolutionCapable": True}, f)
else:
    shutil.copy2(exe, stage / exe.name)
    if system == "Linux":
        for name in ["app.picora.desktop.desktop", "app.picora.desktop.svg"]:
            shutil.copy2(root / "packaging" / name, stage / name)
for name in ["README.md", "docs/COMPATIBILITY.md", "docs/WINDOWS-DEBUG.md", "assets/OFL.txt"]:
    shutil.copy2(root / name, stage / Path(name).name)
licenses = stage / "licenses" / "eframe"
licenses.mkdir(parents=True, exist_ok=True)
for name in ["LICENSE-MIT", "LICENSE-APACHE", "PICORA-PATCH.md"]:
    shutil.copy2(root / "vendor" / "eframe" / name, licenses / name)
out = root / "dist"
out.mkdir(exist_ok=True)
name = f"picora-{version}-{system.lower()}-{platform.machine()}"
if system == "Linux":
    with tarfile.open(out / f"{name}.tar.gz", "w:gz") as archive:
        archive.add(stage, arcname="Picora")
else:
    with zipfile.ZipFile(out / f"{name}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for file in stage.rglob("*"):
            if file.is_file():
                archive.write(file, file.relative_to(stage.parent))
print(out / name)
