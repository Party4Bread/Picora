"""Create portable archives and an ad-hoc-signed macOS disk image from `cargo build --release`. No publishing."""
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
system = platform.system()
stage = root / "target" / "package" / system / "Picora"
if stage.exists():
    shutil.rmtree(stage)
stage.mkdir(parents=True, exist_ok=True)
exe = root / "target" / "release" / ("picora.exe" if system == "Windows" else "picora")
if system == "Darwin":
    app = stage / "Picora.app" / "Contents"
    (app / "MacOS").mkdir(parents=True, exist_ok=True)
    (app / "Resources").mkdir(exist_ok=True)
    shutil.copy2(exe, app / "MacOS" / "picora")
    with (app / "Info.plist").open("wb") as f:
        plistlib.dump({"CFBundleName": "Picora", "CFBundleDisplayName": "Picora", "CFBundleExecutable": "picora", "CFBundleIdentifier": "app.picora.desktop", "CFBundleVersion": version, "CFBundleShortVersionString": version, "CFBundlePackageType": "APPL", "LSUIElement": True, "NSHighResolutionCapable": True}, f)
    # Seal the complete bundle, not just the linker-signed Mach-O executable.
    # Ad-hoc signing provides integrity, NOT Developer ID/Gatekeeper trust.
    subprocess.run(["codesign", "--force", "--sign", "-", str(app.parent)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app.parent)], check=True)
else:
    shutil.copy2(exe, stage / exe.name)
    if system == "Linux":
        for name in ["app.picora.desktop.desktop", "app.picora.desktop.svg"]:
            shutil.copy2(root / "packaging" / name, stage / name)
for name in ["README.md", "docs/COMPATIBILITY.md", "docs/UPDATES.md", "docs/WINDOWS-DEBUG.md", "assets/OFL.txt"]:
    shutil.copy2(root / name, stage / Path(name).name)
shutil.copytree(root / "assets" / "licenses", stage / "licenses", dirs_exist_ok=True)
licenses = stage / "licenses" / "eframe"
licenses.mkdir(parents=True, exist_ok=True)
for name in ["LICENSE-MIT", "LICENSE-APACHE", "PICORA-PATCH.md"]:
    shutil.copy2(root / "vendor" / "eframe" / name, licenses / name)
out = root / "dist"
out.mkdir(exist_ok=True)
arch = {"AMD64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine())
if arch is None:
    raise RuntimeError(f"Unsupported architecture: {platform.machine()}")
os_name = "macos" if system == "Darwin" else system.lower()
name = f"picora-{version}-{os_name}-{arch}"
if system == "Linux":
    with tarfile.open(out / f"{name}.tar.gz", "w:gz") as archive:
        archive.add(stage, arcname="Picora")
else:
    with zipfile.ZipFile(out / f"{name}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for file in stage.rglob("*"):
            if file.is_file():
                archive.write(file, file.relative_to(stage.parent))
if system == "Darwin":
    # Keep the updater ZIP layout unchanged. The DMG is for first installation.
    disk = root / "target" / "package" / "macos-dmg"
    if disk.exists():
        shutil.rmtree(disk)
    disk.mkdir(parents=True)
    subprocess.run(["ditto", str(stage / "Picora.app"), str(disk / "Picora.app")], check=True)
    (disk / "Applications").symlink_to("/Applications")
    shutil.copy2(root / "docs" / "MACOS-INSTALL.md", disk / "READ-ME.txt")
    subprocess.run(["hdiutil", "create", "-volname", "Picora", "-srcfolder", str(disk),
                    "-ov", "-format", "UDZO", str(out / f"{name}.dmg")], check=True)
print(out / name)
