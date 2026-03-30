/// Windows 플랫폼 C++ 코드 컴파일
/// Windows API 연동을 위한 네이티브 코드 빌드
#[cfg(windows)]
fn build_windows() {
    let file = "src/platform/windows.cc";
    let file2 = "src/platform/windows_delete_test_cert.cc";
    // C++ 파일 컴파일 및 정적 라이브러리 생성
    cc::Build::new().file(file).file(file2).compile("windows");
    // WtsApi32 라이브러리 링크 (Windows 세션 API)
    println!("cargo:rustc-link-lib=WtsApi32");
    println!("cargo:rerun-if-changed={}", file);
    println!("cargo:rerun-if-changed={}", file2);
}

/// macOS 플랫폼 Objective-C++ 코드 컴파일
/// macOS 특화 기능 구현 (InputMonitoring 등)
#[cfg(target_os = "macos")]
fn build_mac() {
    let file = "src/platform/macos.mm";
    let mut b = cc::Build::new();
    // macOS 버전 감지 및 호환성 처리
    if let Ok(os_version::OsVersion::MacOS(v)) = os_version::detect() {
        let v = v.version;
        if v.contains("10.14") {
            // macOS 10.14 Mojave 호환성 플래그
            b.flag("-DNO_InputMonitoringAuthStatus=1");
        }
    }
    // C++17 표준 및 파일 컴파일
    b.flag("-std=c++17").file(file).compile("macos");
    println!("cargo:rerun-if-changed={}", file);
}

/// Windows 릴리스 빌드 시 매니페스트 및 리소스 설정
/// 아이콘과 매니페스트 파일을 실행 파일에 포함
#[cfg(all(windows, feature = "inline"))]
fn build_manifest() {
    use std::io::Write;
    // 릴리스 빌드에서만 실행
    if std::env::var("PROFILE").unwrap() == "release" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("res/manifest.xml");
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

/// Android 플랫폼 종속성 설치
/// vcpkg 경로 설정 및 필요한 라이브러리 링크
fn install_android_deps() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os != "android" {
        return;
    }
    // 대상 아키텍처를 Android vcpkg 형식으로 변환
    let mut target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86_64" {
        target_arch = "x64".to_owned();
    } else if target_arch == "x86" {
        target_arch = "x86".to_owned();
    } else if target_arch == "aarch64" {
        target_arch = "arm64".to_owned();
    } else {
        target_arch = "arm".to_owned();
    }
    // vcpkg 설치 경로 구성
    let target = format!("{}-android", target_arch);
    let vcpkg_root = std::env::var("VCPKG_ROOT").unwrap();
    let mut path: std::path::PathBuf = vcpkg_root.into();
    if let Ok(vcpkg_root) = std::env::var("VCPKG_INSTALLED_ROOT") {
        path = vcpkg_root.into();
    } else {
        path.push("installed");
    }
    path.push(target);
    // 라이브러리 검색 경로 및 링크 설정
    println!(
        "cargo:rustc-link-search={}",
        path.join("lib").to_str().unwrap()
    );
    // Android 오디오 및 네이티브 라이브러리 링크
    println!("cargo:rustc-link-lib=ndk_compat");
    println!("cargo:rustc-link-lib=oboe");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=OpenSLES");
}

/// 빌드 스크립트 메인 함수
/// 플랫폼별 설정 및 컴파일 수행
fn main() {
    // 버전 정보 생성
    hbb_common::gen_version();
    // Android 종속성 설치
    install_android_deps();
    // Windows 매니페스트 및 리소스 빌드
    #[cfg(all(windows, feature = "inline"))]
    build_manifest();
    // Windows 네이티브 코드 컴파일
    #[cfg(windows)]
    build_windows();
    // macOS 네이티브 코드 컴파일
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" {
        #[cfg(target_os = "macos")]
        build_mac();
        // macOS ApplicationServices 프레임워크 링크
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
    // 빌드 스크립트 변경 시 재빌드
    println!("cargo:rerun-if-changed=build.rs");
}
