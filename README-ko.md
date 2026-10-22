# Picora — 데스크톱 스티커 선택기

[English](README.md) · **한국어**

**Rust로 만든 Windows, macOS, Linux용 데스크톱 스티커 앱입니다.** 스티커 팩을 검색하고 즐겨찾기에 저장한 뒤, 클립보드 이미지나 첨부 파일을 지원하는 앱에 붙여넣으세요.

Windows 이모지 패널처럼 **Ctrl+Shift+Space**로 Picora를 열 수 있습니다. 스티커를 선택하면 클립보드에 복사하고 이전 앱에 붙여넣기를 시도합니다. 메시지 전송을 위해 Enter를 누르지는 않습니다.

**[최신 버전 다운로드](https://github.com/Party4Bread/Picora/releases/latest)** · **[버그 신고](https://github.com/Party4Bread/Picora/issues)** · **[호환성 안내](docs/COMPATIBILITY.md)**

## 주요 기능

- **스티커 팩 검색:** 지원하는 스티커 저장소를 둘러보거나 URL로 팩을 가져옵니다.
- **라이브러리 정리:** 개별 스티커를 즐겨찾기에 추가하고, 사용자 그룹을 만들고, 마우스 오른쪽 버튼으로 관리합니다.
- **최근 스티커 확인:** 최근 사용한 스티커를 먼저 표시하고, 그 뒤에 새로 추가한 스티커를 표시합니다.
- **움직이는 스티커 사용:** 마우스를 올려 미리 보고, 정지 이미지나 애니메이션 파일로 복사합니다. 다운로드한 스티커는 오프라인에서도 사용할 수 있습니다.
- **키보드 조작:** 전역 단축키 변경과 방향키 탐색을 지원하며, 밝은 테마와 어두운 테마를 제공합니다.
- **앱 내 업데이트:** **Settings → Updates**에서 라이브러리를 유지한 채 GitHub 릴리스를 설치합니다.

## 지원하는 스티커 저장소

현재 Picora에서 지원하는 저장소입니다. 가져온 스티커는 하나의 라이브러리에서 함께 정리할 수 있습니다.

| 저장소 | 탐색 및 가져오기 | 로그인 |
| --- | --- | --- |
| [디시인사이드](https://dccon.dcinside.com/) | 팩 검색 또는 팩 URL로 가져오기 | 필요 없음 |
| [아카라이브(arca.live)](https://arca.live/e/) | 팩 검색 또는 팩 URL로 가져오기 | 필요하며, 웹사이트 인증이 표시될 수 있음 |
| [GIPHY](https://giphy.com/) | 온라인 GIF·스티커 검색, 정지 이미지 또는 GIF 링크 복사 | 개인 GIPHY API 키 |

GIPHY는 **Library → GIPHY**에서 개인 API 키를 입력한 후 **Use and save key**로 설정합니다. 키는 운영체제의 보안 저장소에 보관됩니다. GIPHY 검색 결과는 오프라인 팩과 분리되며, 이미지를 라이브러리 캐시에 저장하지 않습니다. 요청 한도는 API 요금제에 따라 다릅니다. 표시되는 로고는 [GIPHY 공식 표시 자료](https://media.giphy.com/giphy-attribution-marks.zip)입니다.

## 다운로드 및 설치

| 플랫폼 | 다운로드 파일 | 설치 방법 |
| --- | --- | --- |
| Windows x64 | `picora-<version>-windows-x64-setup.exe` | 설치 프로그램을 실행합니다. 포터블 ZIP도 제공됩니다. |
| macOS, Apple Silicon | `picora-<version>-macos-aarch64.dmg` | Picora를 응용 프로그램 폴더로 드래그합니다. |
| Linux x64 | `picora-<version>-linux-x86_64.tar.gz` | 압축을 풀고 `Picora/picora --library`를 실행합니다. |

파일은 [GitHub Releases](https://github.com/Party4Bread/Picora/releases/latest)에서 다운로드할 수 있습니다.

- **Windows:** 설치 프로그램에는 코드 서명이 없습니다. 웹사이트 로그인에는 WebView2가 필요합니다. 수동 업데이트나 제거 전에 Picora를 종료하세요. 제거해도 라이브러리와 설정은 유지됩니다.
- **macOS:** 임시 서명(ad-hoc)을 사용하며 Apple 공증을 받지 않았습니다. Gatekeeper가 실행을 차단하면 [macOS 설치 안내](docs/MACOS-INSTALL.md)를 확인하세요. Intel Mac용 빌드는 현재 제공하지 않습니다.
- **Linux:** [소스에서 빌드](#소스에서-빌드)에 나열된 시스템 라이브러리가 필요합니다. 선택기 창에는 X11 또는 XWayland가 필요합니다.

## 빠른 시작

1. 트레이 또는 메뉴 막대 아이콘에서 **Library → Discover**를 엽니다. 팩을 검색하거나 저장소 URL을 붙여넣고 **Add pack to favorites**를 선택합니다.
2. **Groups**에서 그룹을 만듭니다. 스티커를 마우스 오른쪽 버튼으로 클릭하면 즐겨찾기 추가, 그룹 지정, 라이브러리에서 제거가 가능합니다.
3. 채팅이나 편집기를 선택하고 **Ctrl+Shift+Space**를 누른 뒤 스티커를 선택합니다. 자동 붙여넣기가 되지 않으면 **Ctrl+V** 또는 **Cmd+V**를 사용하세요.

창을 닫아도 Picora는 트레이에서 계속 실행됩니다. 완전히 종료하려면 **Quit**을 선택하세요. 라이브러리 창에서는 자동 붙여넣기 없이 복사만 합니다. 로그인 시 자동 실행은 직접 켜야 합니다.

### 저장소 계정

**Accounts → Sign in / reconnect**를 열고 저장소 웹사이트에서 로그인하세요. 브라우저 인증이 표시되면 완료한 뒤 로그인 창을 닫습니다. **Check connection**으로 목록 접근 여부를 확인할 수 있습니다. 저장된 세션 상태와 연결 오류는 Accounts에 표시됩니다.

Picora는 운영체제가 보호하는 저장소에 세션 쿠키를 저장합니다. 저장에 실패해도 앱을 종료하기 전까지는 해당 세션을 사용할 수 있습니다. 일부 저장소는 로그인 없이 탐색할 수 있습니다.

### 움직이는 스티커

일반 선택 시 애니메이션 GIF/WebP를 파일로 붙여넣습니다. 지원되는 APNG와 MP4 스티커는 GIF로 변환할 수 있습니다. **Paste original file**은 원본 데이터를 그대로 유지하고, **Copy still image**는 한 프레임을 PNG로 복사합니다. 애니메이션 재생 여부와 첨부 파일 표시 방식은 붙여넣는 앱에 따라 달라집니다.

## 데스크톱 연동

| 환경 | 설정 |
| --- | --- |
| Windows | 전역 단축키와 자동 붙여넣기를 지원합니다. 관리자 권한으로 실행한 앱에서는 수동 붙여넣기가 필요할 수 있습니다. |
| macOS | 자동 붙여넣기를 사용하려면 Picora에 손쉬운 사용(Accessibility) 권한을 부여하세요. |
| Linux X11 | 클립보드 소유권을 유지하려면 Picora를 실행 상태로 두세요. |
| KDE Plasma / Wayland | 단축키 대화상자를 승인하고, 자동 붙여넣기를 위해 **Settings → Wayland keyboard access**를 켜세요. 네이티브 클립보드 복사에는 키보드 권한이 필요하지 않습니다. |
| 기타 Wayland 환경 | 포털과 클립보드 지원은 환경마다 다릅니다. 단축키 포털을 사용할 수 없다면 데스크톱 설정에서 `picora --show`에 단축키를 지정하세요. |

클립보드 형식, 입력기, 플랫폼별 제한은 [호환성 안내](docs/COMPATIBILITY.md)를 확인하세요.

## 소스에서 빌드

최신 Rust stable 도구 모음을 설치하고 저장소를 복제한 뒤 실행합니다.

```sh
git clone https://github.com/Party4Bread/Picora.git
cd Picora
cargo run --release --locked -- --library
```

Debian/Ubuntu에서는 먼저 빌드 의존성을 설치합니다.

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libx11-dev libxrandr-dev libxi-dev libxdo-dev libgl1-mesa-dev libdbus-1-dev
```

Linux 릴리스 실행에는 GTK 3, WebKitGTK 4.1, AppIndicator와 관련 X11 라이브러리가 필요합니다. macOS 빌드에는 Apple 개발 도구가 필요하며 시스템 WebKit을 사용합니다. Windows 빌드에는 Visual Studio C++ Build Tools와 최신 Rust MSVC 도구 모음(`rustup update stable`)이 필요합니다.

`vendor/eframe`에는 Windows의 숨겨진 창 처리 수정이 포함되어 있으므로 삭제하지 마세요. 자세한 내용은 [패치 출처](vendor/eframe/PICORA-PATCH.md)를 확인하세요.

### 명령줄 실행과 로컬 데이터

```sh
picora --show        # 선택기 열기
picora --library     # 라이브러리 열기
picora --background  # 트레이에서 시작
```

데이터 디렉터리마다 하나의 인스턴스만 실행됩니다. 개발용 데이터를 분리하려면 `PICORA_DATA_DIR`을 설정하세요.

```sh
PICORA_DATA_DIR=./test-data cargo run --locked -- --library
```

PowerShell에서는 다음과 같이 실행합니다.

```powershell
$env:PICORA_DATA_DIR = "$PWD\test-data"
cargo run --locked -- --library
```

### 테스트와 패키징

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
python scripts/package.py
```

패키징 결과는 `dist/`에 플랫폼별 압축 파일로 생성됩니다. macOS에서는 DMG도 생성합니다. Windows 설치 프로그램을 만들려면 Inno Setup 6과 Python 3.11 이상을 설치하세요.

```powershell
./scripts/build-installer.ps1
```

데스크톱 및 실제 네트워크 검증은 명시적으로 실행해야 합니다.

```sh
cargo test --locked --test arca_live -- --ignored --nocapture
cargo test --locked --test live live_dcinside_search_pack_and_image -- --ignored
cargo test --locked --test windows_viewports -- --ignored --nocapture
python scripts/smoke_wayland_clipboard.py
```

아카라이브(arca.live) 검증은 저장된 세션을 사용합니다. Windows 창 테스트에는 대화형 데스크톱이 필요하고, Wayland 스크립트에는 화면 없이 실행하는 KWin이 필요합니다. macOS 브라우저 클립보드를 검증하려면 `cargo build --locked --example clipboard_fixture`로 빌드한 뒤 [smoke_macos_chrome.py](scripts/smoke_macos_chrome.py)의 안내를 따르세요.

패키징과 설치 프로그램 스크립트는 CI에서 사용합니다. `test_update_manifest.py`는 릴리스 메타데이터를 검증하고, `tests/update_process.rs`는 `cargo test`의 일부로 업데이트 교체와 복구 시나리오를 실행합니다. 플랫폼별 검증 스크립트는 화면 없는 CI에서 확인할 수 없는 데스크톱 동작을 검사합니다. [릴리스와 복구 문서](docs/UPDATES.md)도 참고하세요.

Linux 압축 파일에는 데스크톱 연동 파일이 포함되어 있습니다. 최신 Wayland 포털에서 전역 단축키를 등록하려면 Picora를 식별할 데스크톱 항목이 필요합니다. 실행 파일을 `PATH`에 등록하고, 데스크톱 항목을 `~/.local/share/applications/`에, 아이콘을 `~/.local/share/icons/hicolor/scalable/apps/`에 설치하세요.

## 프로젝트

Picora는 Rust와 egui를 사용합니다. 저장소 어댑터, SQLite 저장소, 미디어 캐시, 네이티브 데스크톱 연동은 별도 모듈로 구성되며, 웹사이트 로그인은 Wry 보조 프로세스에서 실행됩니다.

[Noto Sans KR](https://github.com/notofonts/noto-cjk/tree/main/Sans/SubsetOTF/KR)은 [SIL Open Font License](assets/OFL.txt)에 따라 포함되어 있습니다. 스티커 이미지는 사용자의 요청에 따라 원본 저장소에서 다운로드합니다. Picora는 지원하는 저장소와 제휴 관계가 아닙니다.
