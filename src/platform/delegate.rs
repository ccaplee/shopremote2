use std::{ffi::c_void, rc::Rc};

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{NSApp, NSApplication, NSApplicationActivationPolicy::*, NSMenu, NSMenuItem},
    base::{id, nil, YES},
    foundation::{NSAutoreleasePool, NSString},
};
use objc::runtime::{Class, NO};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Object, Sel, BOOL},
    sel, sel_impl,
};
use sciter::{make_args, Host};

use hbb_common::log;

// macOS Delegate에서 앱 핸들러를 저장하는 인스턴스 변수명
static APP_HANDLER_IVAR: &str = "GoDeskAppHandler";

// 메뉴 항목과 이벤트의 태그 상수 정의
const TERMINATE_TAG: u32 = 0;      // 앱 종료
const SHOW_ABOUT_TAG: u32 = 1;     // About 대화상자 표시
const SHOW_SETTINGS_TAG: u32 = 2;  // Settings 대화상자 표시
const RUN_ME_TAG: u32 = 3;         // 새 창 실행
const AWAKE: u32 = 4;              // 앱 활성화

/// macOS 앱 이벤트 처리를 위한 핸들러 트레이트
/// 메뉴 항목 선택, 이벤트 등을 처리
pub trait AppHandler {
    /// 주어진 명령 코드를 처리하는 메서드
    fn command(&mut self, cmd: u32);
}

/// macOS 앱 델리게이트의 상태를 저장하는 구조체
/// 앱 이벤트 처리를 위한 핸들러를 포함
struct DelegateState {
    handler: Option<Box<dyn AppHandler>>,
}

impl DelegateState {
    /// 주어진 명령을 처리
    /// TERMINATE_TAG이면 앱을 종료하고, 그 외에는 핸들러에 위임
    fn command(&mut self, command: u32) {
        if command == TERMINATE_TAG {
            unsafe {
                let () = msg_send!(NSApp(), terminate: nil);
            }
        } else if let Some(inner) = self.handler.as_mut() {
            inner.command(command)
        }
    }
}

// 앱 시작 완료 여부를 나타내는 전역 플래그
static mut LAUNCHED: bool = false;

/// Sciter Host에 대한 AppHandler 구현
/// UI 함수 호출을 통해 About, Settings 등의 화면 표시
impl AppHandler for Rc<Host> {
    /// 명령에 따라 적절한 UI 함수 호출
    fn command(&mut self, cmd: u32) {
        if cmd == SHOW_ABOUT_TAG {
            let _ = self.call_function("awake", &make_args![]);
            let _ = self.call_function("showAbout", &make_args![]);
        } else if cmd == SHOW_SETTINGS_TAG {
            let _ = self.call_function("awake", &make_args![]);
            let _ = self.call_function("showSettings", &make_args![]);
        } else if cmd == AWAKE {
            let _ = self.call_function("awake", &make_args![]);
        }
    }
}

/// macOS NSApplication 델리게이트를 설정하는 함수
/// 메뉴 항목 처리, URL 스킴 처리, 앱 이벤트 처리 등을 담당
/// 참고: https://github.com/xi-editor/druid/blob/master/druid-shell/src/platform/mac/application.rs
unsafe fn set_delegate(handler: Option<Box<dyn AppHandler>>) {
    let Some(mut decl) = ClassDecl::new("AppDelegate", class!(NSObject)) else {
        log::error!("Failed to new AppDelegate");
        return;
    };
    decl.add_ivar::<*mut c_void>(APP_HANDLER_IVAR);

    // 앱이 시작을 완료했을 때 호출되는 콜백
    decl.add_method(
        sel!(applicationDidFinishLaunching:),
        application_did_finish_launching as extern "C" fn(&mut Object, Sel, id),
    );

    // 제목 없는 파일 열기 요청 처리
    decl.add_method(
        sel!(applicationShouldOpenUntitledFile:),
        application_should_handle_open_untitled_file as extern "C" fn(&mut Object, Sel, id) -> BOOL,
    );

    // 앱이 활성화되었을 때 호출
    decl.add_method(
        sel!(applicationDidBecomeActive:),
        application_did_become_active as extern "C" fn(&mut Object, Sel, id) -> BOOL,
    );

    // 앱이 숨김 상태에서 표시되었을 때 호출
    decl.add_method(
        sel!(applicationDidUnhide:),
        application_did_become_unhide as extern "C" fn(&mut Object, Sel, id) -> BOOL,
    );

    // 앱의 아이콘을 클릭할 때 다시 열기 요청 처리
    decl.add_method(
        sel!(applicationShouldHandleReopen:),
        application_should_handle_reopen as extern "C" fn(&mut Object, Sel, id) -> BOOL,
    );

    // 앱이 종료될 때 호출
    decl.add_method(
        sel!(applicationWillTerminate:),
        application_will_terminate as extern "C" fn(&mut Object, Sel, id) -> BOOL,
    );

    // 메뉴 항목 선택 시 호출
    decl.add_method(
        sel!(handleMenuItem:),
        handle_menu_item as extern "C" fn(&mut Object, Sel, id),
    );
    // URL 스킴 처리
    decl.add_method(
        sel!(application:openURLs:),
        handle_open_urls as extern "C" fn(&Object, Sel, id, id) -> (),
    );
    let decl = decl.register();
    let delegate: id = msg_send![decl, alloc];
    let () = msg_send![delegate, init];
    let state = DelegateState { handler };
    let handler_ptr = Box::into_raw(Box::new(state));
    (*delegate).set_ivar(APP_HANDLER_IVAR, handler_ptr as *mut c_void);
    // URL 스킴 핸들러 설정
    let Some(cls) = Class::get("NSAppleEventManager") else {
        log::error!("Failed to get NSAppleEventManager");
        return;
    };
    let manager: *mut Object = msg_send![cls, sharedAppleEventManager];
    let _: () = msg_send![manager,
                              setEventHandler: delegate
                              andSelector: sel!(handleEvent:withReplyEvent:)
                              forEventClass: fruitbasket::kInternetEventClass
                              andEventID: fruitbasket::kAEGetURL];
    let () = msg_send![NSApp(), setDelegate: delegate];
}

/// 앱 시작 완료 후 호출되는 콜백
/// LAUNCHED 플래그를 true로 설정하고 앱을 활성화
extern "C" fn application_did_finish_launching(_this: &mut Object, _: Sel, _notification: id) {
    unsafe {
        LAUNCHED = true;
    }
    unsafe {
        // 다른 앱들을 무시하고 이 앱을 최전면에 표시
        let () = msg_send![NSApp(), activateIgnoringOtherApps: YES];
    }
}

/// 제목 없는 파일 열기 요청 처리
/// macOS가 앱을 시작할 때 호출되며, 앱이 이미 시작된 경우 AWAKE 명령 실행
extern "C" fn application_should_handle_open_untitled_file(
    this: &mut Object,
    _: Sel,
    _sender: id,
) -> BOOL {
    unsafe {
        if !LAUNCHED {
            // 앱이 아직 시작 중이면 YES를 반환하여 기본 처리 허용
            return YES;
        }
        // 플랫폼별 처리 수행
        crate::platform::macos::handle_application_should_open_untitled_file();
        let inner: *mut c_void = *this.get_ivar(APP_HANDLER_IVAR);
        let inner = &mut *(inner as *mut DelegateState);
        // 앱을 활성화하도록 명령
        (*inner).command(AWAKE);
    }
    YES
}

/// 앱의 아이콘을 클릭할 때 다시 열기 요청 처리
extern "C" fn application_should_handle_reopen(_this: &mut Object, _: Sel, _sender: id) -> BOOL {
    YES
}

/// 앱이 활성 상태가 되었을 때 호출
extern "C" fn application_did_become_active(_this: &mut Object, _: Sel, _sender: id) -> BOOL {
    YES
}

/// 앱이 숨겨진 상태에서 표시되었을 때 호출
extern "C" fn application_did_become_unhide(_this: &mut Object, _: Sel, _sender: id) -> BOOL {
    YES
}

/// 앱이 종료될 때 호출되는 콜백
extern "C" fn application_will_terminate(_this: &mut Object, _: Sel, _sender: id) -> BOOL {
    YES
}

/// 메뉴 항목 선택 처리 (모든 창이 닫힌 경우 포함)
/// 선택된 메뉴 항목의 태그 값을 확인하여 적절한 명령 실행
extern "C" fn handle_menu_item(this: &mut Object, _: Sel, item: id) {
    unsafe {
        let tag: isize = msg_send![item, tag];
        let tag = tag as u32;
        if tag == RUN_ME_TAG {
            // 새 창 생성 명령 실행
            crate::run_me(Vec::<String>::new()).ok();
        } else {
            let inner: *mut c_void = *this.get_ivar(APP_HANDLER_IVAR);
            let inner = &mut *(inner as *mut DelegateState);
            // 해당 태그 명령 실행
            (*inner).command(tag as u32);
        }
    }
}

/// URL 스킴 처리 콜백 (예: rdp:// 등)
/// 수신된 URL을 별도 스레드에서 처리
#[no_mangle]
extern "C" fn handle_open_urls(_self: &Object, _cmd: Sel, _: id, urls: id) -> () {
    use cocoa::foundation::NSArray;
    use cocoa::foundation::NSURL;
    use std::ffi::CStr;
    unsafe {
        // 수신된 모든 URL에 대해 반복 처리
        for i in 0..urls.count() {
            let theurl = CStr::from_ptr(urls.objectAtIndex(i).absoluteString().UTF8String())
                .to_string_lossy()
                .into_owned();
            log::debug!("URL received: {}", theurl);
            // 별도 스레드에서 URL 스킴 처리 실행
            std::thread::spawn(move || crate::handle_url_scheme(theurl));
        }
    }
}

/// 서비스 다시 열기 로직 사용자 정의
/// 주 Rustdesk 프로세스 호출
#[no_mangle]
fn service_should_handle_reopen(
    _obj: &Object,
    _sel: Sel,
    _sender: id,
    _has_visible_windows: BOOL,
) -> BOOL {
    log::debug!("Invoking the main rustdesk process");
    std::thread::spawn(move || crate::handle_url_scheme("".to_string()));
    // 기본 로직을 방지하고 커스텀 로직 사용
    NO
}

/// 메뉴 항목 생성 헬퍼 함수
/// 주어진 제목, 단축키, 태그로 NSMenuItem 객체 생성
unsafe fn make_menu_item(title: &str, key: &str, tag: u32) -> *mut Object {
    let title = NSString::alloc(nil).init_str(title);
    let action = sel!(handleMenuItem:);
    let key = NSString::alloc(nil).init_str(key);
    // NSMenuItem 객체 생성 및 초기화
    let object = NSMenuItem::alloc(nil)
        .initWithTitle_action_keyEquivalent_(title, action, key)
        .autorelease();
    // 메뉴 항목의 태그 설정 (나중에 식별하기 위함)
    let () = msg_send![object, setTag: tag];
    object
}

/// macOS 메뉴바 생성 및 설정
/// is_index: true이면 인덱스 화면(About, Settings), false이면 일반 창 메뉴 표시
pub fn make_menubar(host: Rc<Host>, is_index: bool) {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        // 앱 델리게이트 설정
        set_delegate(Some(Box::new(host)));
        // 메뉴바 생성
        let menubar = NSMenu::new(nil).autorelease();
        let app_menu_item = NSMenuItem::new(nil).autorelease();
        menubar.addItem_(app_menu_item);
        // 앱 메뉴 생성
        let app_menu = NSMenu::new(nil).autorelease();

        if !is_index {
            // 일반 창인 경우: "새 창" 메뉴 항목 추가
            let new_item = make_menu_item("New Window", "n", RUN_ME_TAG);
            app_menu.addItem_(new_item);
        } else {
            // 인덱스 화면인 경우: "About", "Settings" 메뉴 항목 추가
            // (앱 시작 시 인자 없이 실행된 메인 패널)
            let about_item = make_menu_item("About", "", SHOW_ABOUT_TAG);
            app_menu.addItem_(about_item);
            let separator = NSMenuItem::separatorItem(nil).autorelease();
            app_menu.addItem_(separator);
            let settings_item = make_menu_item("Settings", "s", SHOW_SETTINGS_TAG);
            app_menu.addItem_(settings_item);
        }
        // 구분선 추가
        let separator = NSMenuItem::separatorItem(nil).autorelease();
        app_menu.addItem_(separator);
        // "종료" 메뉴 항목 추가
        let quit_item = make_menu_item(
            &format!("Quit {}", crate::get_app_name()),
            "q",
            TERMINATE_TAG,
        );
        app_menu_item.setSubmenu_(app_menu);
        // 미사용 코드 (향후 활성화 가능):
        /*
        if !enabled {
            let () = msg_send![quit_item, setEnabled: NO];
        }

        if selected {
            let () = msg_send![quit_item, setState: 1_isize];
        }
        let () = msg_send![item, setTag: id as isize];
        */
        app_menu.addItem_(quit_item);
        // 메뉴바를 앱에 설정
        NSApp().setMainMenu_(menubar);
    }
}

/// macOS 독(Dock)에 앱 아이콘 표시
/// 앱의 활성화 정책을 일반(Regular)으로 설정하여 Dock에 나타나도록 함
pub fn show_dock() {
    unsafe {
        NSApp().setActivationPolicy_(NSApplicationActivationPolicyRegular);
    }
}
