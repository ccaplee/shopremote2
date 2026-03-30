fn main() {
    // Windows 플랫폼 specific 빌드 설정
    #[cfg(windows)]
    {
        use std::io::Write;

        // Windows 리소스 파일 설정 (아이콘, 언어, 매니페스트)
        let mut res = winres::WindowsResource::new();

        // 애플리케이션 아이콘 설정
        res.set_icon("../../res/icon.ico")
            // 언어 설정: 영어(미국)
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            // Windows 매니페스트 파일 설정 (DPI 인식, COM 등록정보)
            .set_manifest_file("../../res/manifest.xml");

        // 리소스 컴파일
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}
